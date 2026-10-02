use super::{CaptureLocations, Config, Error, ErrorKind, MatchOptions, Regex, escape};

fn ucfg() -> Config {
    Config {
        utf: true,
        ucp: true,
        multi_line: true,
        ..Config::default()
    }
}

fn bcfg() -> Config {
    Config {
        multi_line: true,
        ..Config::default()
    }
}

fn re(p: &str) -> Regex {
    Regex::new(p, ucfg()).unwrap()
}

fn find(p: &str, h: &str) -> Option<(usize, usize)> {
    re(p).find_at(h.as_bytes(), 0).unwrap()
}

fn find_bytes(p: &str, h: &[u8]) -> Option<(usize, usize)> {
    re(p).find_at(h, 0).unwrap()
}

fn find_all_with(r: &Regex, h: &[u8]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut last_end = 0usize;
    let mut last_match: Option<usize> = None;
    while last_end <= h.len() {
        let Some(m) = r.find_at(h, last_end).unwrap() else {
            break;
        };
        if m.0 == m.1 {
            last_end = m.1 + 1;
            if Some(m.1) == last_match {
                continue;
            }
        } else {
            last_end = m.1;
        }
        last_match = Some(m.1);
        out.push(m);
    }
    out
}

fn find_all(p: &str, h: &str) -> Vec<(usize, usize)> {
    find_all_with(&re(p), h.as_bytes())
}

fn compile_err(p: &str) -> String {
    Regex::new(p, ucfg()).unwrap_err().to_string()
}

fn caps(p: &str, h: &str) -> Vec<Option<(usize, usize)>> {
    let r = re(p);
    let mut locs = r.capture_locations();
    if r.captures_read_at(&mut locs, h.as_bytes(), 0)
        .unwrap()
        .is_none()
    {
        return Vec::new();
    }
    (0..locs.len()).map(|i| locs.get(i)).collect()
}

#[test]
fn literal_basic() {
    assert_eq!(find("abc", "xxabcxx"), Some((2, 5)));
    assert_eq!(find("abc", "xxabxx"), None);
    assert_eq!(find("", "abc"), Some((0, 0)));
}

#[test]
fn find_at_respects_start() {
    let r = re("ab");
    assert_eq!(r.find_at(b"ab ab", 1).unwrap(), Some((3, 5)));
    assert_eq!(r.find_at(b"ab ab", 5).unwrap(), None);
    assert!(r.is_match_at(b"xxab", 2).unwrap());
    assert!(!r.is_match_at(b"xxab", 3).unwrap());
}

#[test]
#[should_panic(expected = "must be <= haystack length")]
fn find_at_panics_past_end() {
    let _ = re("a").find_at(b"abc", 4);
}

#[test]
fn character_classes() {
    assert_eq!(find("[a-c]+", "xxabcbd"), Some((2, 6)));
    assert_eq!(find("[^a-c]+", "abcxyz"), Some((3, 6)));
    assert_eq!(find("[[:digit:]]+", "ab123c"), Some((2, 5)));
    assert_eq!(find("[[:^alpha:]]+", "ab12cd"), Some((2, 4)));
    assert_eq!(find("[]a]+", "x]a]x"), Some((1, 4)));
    assert_eq!(find("[\\w-]+", "@a-b@"), Some((1, 4)));
    assert_eq!(find("\\d+", "x\u{663}\u{664}y"), Some((1, 5)));
}

#[test]
fn extended_class_operators() {
    assert_eq!(find("(?[ [a-z] - [aeiou] ])+", "aebcde"), Some((2, 5)));
    assert_eq!(find("(?[\\w & !\\d])+", "12ab3"), Some((2, 4)));
}

#[test]
fn unicode_properties() {
    assert_eq!(find("\\p{Greek}+", "ab\u{3b1}\u{3b2}c"), Some((2, 6)));
    assert_eq!(find("\\p{Lu}", "ab\u{c9}c"), Some((2, 4)));
    assert_eq!(find("\\P{L}+", "ab12cd"), Some((2, 4)));
    assert_eq!(find("\\p{Han}", "x\u{65e5}"), Some((1, 4)));
    assert_eq!(find("\\p{sc:Latin}+", "\u{3b1}abc"), Some((2, 5)));
    assert_eq!(find("\\p{Xan}+", "--a1--"), Some((2, 4)));
    assert_eq!(find("\\p{Any}", "\u{1f600}"), Some((0, 4)));
    assert_eq!(find("\\p{^L}", "ab1"), Some((2, 3)));
    assert_eq!(find("\\p{bc=AL}", "a\u{627}"), Some((1, 3)));
}

#[test]
fn unicode_17_scripts() {
    assert!(Regex::new("\\p{Sidetic}", ucfg()).is_ok());
    assert!(Regex::new("\\p{Tolong_Siki}", ucfg()).is_ok());
}

#[test]
fn caseless_matching() {
    let c = Config {
        caseless: true,
        ..ucfg()
    };
    let r = Regex::new("strasse", c).unwrap();
    assert_eq!(r.find_at(b"STRASSE", 0).unwrap(), Some((0, 7)));
    let k = Regex::new("k", c).unwrap();
    assert_eq!(k.find_at("\u{212a}".as_bytes(), 0).unwrap(), Some((0, 3)));
    assert_eq!(find("(?i)\u{3c3}", "\u{3a3}"), Some((0, 2)));
    assert_eq!(find("(?i)\u{3c3}", "\u{3c2}"), Some((0, 2)));
    assert_eq!(find("(?i)[a-c]+", "xABCx"), Some((1, 4)));
    assert_eq!(find("(?ir)k", "\u{212a}k"), Some((3, 4)));
}

#[test]
fn quantifiers() {
    assert_eq!(find("a{2,3}", "aaaa"), Some((0, 3)));
    assert_eq!(find("a{,2}b", "aaab"), Some((1, 4)));
    assert_eq!(find("a+?", "aaa"), Some((0, 1)));
    assert_eq!(find("a*?b", "aab"), Some((0, 3)));
    assert_eq!(find("a++a", "aaa"), None);
    assert_eq!(find("(?:ab){2}", "abababx"), Some((0, 4)));
    assert_eq!(find("x{2,}?", "xxxx"), Some((0, 2)));
    assert_eq!(find("a{0}b", "ab"), Some((1, 2)));
}

#[test]
fn captures_and_names() {
    assert_eq!(
        caps("(a)(b)?(c)", "ac"),
        vec![Some((0, 2)), Some((0, 1)), None, Some((1, 2))]
    );
    let r = re("(?<year>\\d{4})-(?<mon>\\d\\d)");
    assert_eq!(r.captures_len(), 3);
    assert_eq!(
        r.capture_names(),
        &[None, Some("year".to_string()), Some("mon".to_string())]
    );
    let mut locs: CaptureLocations = r.capture_locations();
    assert_eq!(locs.len(), 3);
    assert_eq!(
        r.captures_read_at(&mut locs, b"on 2024-05", 0).unwrap(),
        Some((3, 10))
    );
    assert_eq!(locs.get(1), Some((3, 7)));
    assert_eq!(locs.get(2), Some((8, 10)));
    assert_eq!(locs.get(3), None);
}

#[test]
fn branch_reset_and_duplicate_names() {
    assert_eq!(caps("(?|(a)|(b))", "b"), vec![Some((0, 1)), Some((0, 1))]);
    assert_eq!(find("(?J)(?<n>a)|(?<n>b)", "b"), Some((0, 1)));
}

#[test]
fn backreferences() {
    assert_eq!(find("(\\w+)\\s+\\1", "the the cat"), Some((0, 7)));
    assert_eq!(find("(?i)(ab)\\1", "abAB"), Some((0, 4)));
    assert_eq!(find("(?<w>x)\\k<w>", "axxa"), Some((1, 3)));
    assert_eq!(find("(a)|b\\1", "b"), None);
    assert_eq!(find("(a)\\g{-1}", "xaa"), Some((1, 3)));
}

#[test]
fn lookarounds() {
    assert_eq!(find("foo(?=bar)", "foobaz foobar"), Some((7, 10)));
    assert_eq!(find("foo(?!bar)", "foobar foobaz"), Some((7, 10)));
    assert_eq!(find("(?<=foo)bar", "xbar foobar"), Some((8, 11)));
    assert_eq!(find("(?<!foo)bar", "foobar xbar"), Some((8, 11)));
    assert_eq!(find("(?<=a|bc)d", "bcd"), Some((2, 3)));
    assert_eq!(find("(?<=\\d{1,3})x", "12x"), Some((2, 3)));
    assert_eq!(find("(*napla:\\w+)\\w", "ab"), Some((0, 1)));
}

#[test]
fn lookbehind_sees_before_start() {
    let r = re("(?<=a)b");
    assert_eq!(r.find_at(b"ab", 1).unwrap(), Some((1, 2)));
    let w = re("\\bb");
    assert_eq!(w.find_at(b"ab", 1).unwrap(), None);
}

#[test]
fn anchors_and_start_offset() {
    let r = Regex::new(
        "^a",
        Config {
            utf: true,
            ucp: true,
            ..Config::default()
        },
    )
    .unwrap();
    assert_eq!(r.find_at(b"ba", 1).unwrap(), None);
    assert_eq!(re("^a").find_at(b"b\na", 1).unwrap(), Some((2, 3)));
    assert_eq!(re("\\Ga").find_at(b"aaa", 1).unwrap(), Some((1, 2)));
    assert_eq!(re("\\Aa").find_at(b"aaa", 1).unwrap(), None);
    assert_eq!(find("a$", "a\nb"), Some((0, 1)));
    assert_eq!(find("\\w+\\z", "ab\ncd"), Some((3, 5)));
    assert_eq!(find("\\w+\\Z", "ab\n"), Some((0, 2)));
}

#[test]
fn atomic_and_possessive_groups() {
    assert_eq!(find("(?>a+)b", "aaab"), Some((0, 4)));
    assert_eq!(find("(?>a+)a", "aaa"), None);
    assert_eq!(find("(?:a|ab)c", "abc"), Some((0, 3)));
    assert_eq!(find("(?>a|ab)c", "abc"), None);
}

#[test]
fn conditionals() {
    assert_eq!(find("(a)?(?(1)b|c)", "ab"), Some((0, 2)));
    assert_eq!(find("(a)?(?(1)b|c)", "c"), Some((0, 1)));
    assert_eq!(find("(?(?=\\d)\\d+|[a-z]+)", "ab12"), Some((0, 2)));
    assert_eq!(find("(?(DEFINE)(?<d>\\d))(?&d)+", "x123"), Some((1, 4)));
    assert_eq!(find("(?(VERSION>=10.48)yes|no)", "yes no"), Some((0, 3)));
}

#[test]
fn recursion_and_subroutines() {
    assert_eq!(find("\\((?:[^()]++|(?R))*\\)", "x(a(b)c)y"), Some((1, 8)));
    assert_eq!(find("^((.)(?1)\\2|.?)$", "racecar"), Some((0, 7)));
    assert_eq!(find("^((.)(?1)\\2|.?)$", "abca"), None);
    assert_eq!(
        find(
            "(sens|respons)e and (?1)ibility",
            "sense and responsibility"
        ),
        Some((0, 24))
    );
}

#[test]
fn recursion_with_returned_groups() {
    let p = "(?(DEFINE)(?<w>(?|(?<s>Sat)urday|(?<s>Sun)day)))(?&w(<s>)),\\k<s>";
    assert_eq!(find(p, "Sunday,Sun"), Some((0, 10)));
    assert_eq!(find(p, "Sunday,Sat"), None);
    assert_eq!(
        find(
            "(?(DEFINE)(?<w>(?|(?<s>Sat)urday|(?<s>Sun)day)))(?&w),\\k<s>",
            "Sunday,Sun"
        ),
        None
    );
}

#[test]
fn accept_inside_called_group_returns_from_the_call() {
    assert_eq!(
        find_all("(?=((*ACCEPT)))(?1)", "ab"),
        vec![(0, 0), (1, 1), (2, 2)]
    );
    assert_eq!(
        find_all("(?=(a(*ACCEPT)|-))(?1)", "ab-.c"),
        vec![(0, 1), (2, 3)]
    );
}

#[test]
fn infinite_recursion_is_an_error() {
    let e = re("((?1)|a)").find_at(b"aa", 0).unwrap_err();
    assert_eq!(e.kind(), ErrorKind::Match);
    assert_eq!(
        e.to_string(),
        "PCRE2: error matching: JIT stack limit reached"
    );
}

#[test]
fn backtracking_verbs() {
    assert_eq!(find("a+(*COMMIT)b", "aac aab"), None);
    assert_eq!(find("(*COMMIT)abc", "xyzabc"), Some((3, 6)));
    assert_eq!(find("a+(*SKIP)b|a+c", "aaac"), None);
    assert_eq!(find("aaa(*PRUNE)b|aac", "aaac"), Some((1, 4)));
    assert_eq!(find("a(*THEN)b|ac", "ac"), Some((0, 2)));
    assert_eq!(find("a(*FAIL)|b", "ab"), Some((1, 2)));
    assert_eq!(find("o(*ACCEPT)x", "foo"), Some((1, 2)));
    assert_eq!(find("a(*SKIP:X)b|(*MARK:X)c", "ac"), Some((1, 2)));
}

#[test]
fn keep_out_and_empty_matches() {
    assert_eq!(find("foo\\Kbar", "foobar"), Some((3, 6)));
    assert_eq!(find_all("x*", "axb"), vec![(0, 0), (1, 2), (3, 3)]);
    assert_eq!(
        find_all("\\b", "ab cd"),
        vec![(0, 0), (2, 2), (3, 3), (5, 5)]
    );
}

#[test]
fn options_and_modes() {
    assert_eq!(find("(?s)a.b", "a\nb"), Some((0, 3)));
    assert_eq!(find("a.b", "a\nb"), None);
    assert_eq!(find("(?x) a b # comment", "ab"), Some((0, 2)));
    assert_eq!(find("(?i:a)b", "Ab AB"), Some((0, 2)));
    assert_eq!(find("(?U)a+", "aaa"), Some((0, 1)));
    assert_eq!(find("(?xx)[a b]+", "a b"), Some((0, 1)));
    let c = Config {
        extended: true,
        dotall: true,
        ..ucfg()
    };
    assert_eq!(
        Regex::new("a . b", c).unwrap().find_at(b"a\nb", 0).unwrap(),
        Some((0, 3))
    );
}

#[test]
fn crlf_newline_convention() {
    let c = Config {
        crlf: true,
        ..ucfg()
    };
    let r = Regex::new("a$", c).unwrap();
    assert_eq!(r.find_at(b"a\r\nb", 0).unwrap(), Some((0, 1)));
    let d = Regex::new("^b", c).unwrap();
    assert_eq!(d.find_at(b"a\rb", 0).unwrap(), Some((2, 3)));
}

#[test]
fn start_of_pattern_options() {
    assert_eq!(find("(*CR)a$", "a\rb"), Some((0, 1)));
    assert_eq!(find("(*NO_START_OPT)(*COMMIT)abc", "xyzabc"), None);
    let e = re("(*LIMIT_MATCH=10)(a+)+b")
        .find_at(b"aaaaaaaaaaaaaaaaaaaacb", 0)
        .unwrap_err();
    assert_eq!(e.to_string(), "PCRE2: error matching: match limit exceeded");
}

#[test]
fn match_limit_is_enforced() {
    let r = re("(a+)+$");
    let mut h = vec![b'a'; 64];
    h.push(b'!');
    let e = r.find_at(&h, 0).unwrap_err();
    assert_eq!(e.to_string(), "PCRE2: error matching: match limit exceeded");
}

#[test]
fn deep_nesting_does_not_overflow_the_stack() {
    let p = format!("{}a{}", "(?:".repeat(200), ")".repeat(200));
    assert_eq!(find(&p, "xa"), Some((1, 2)));
    let long = "a".repeat(200_000);
    assert_eq!(find("(?:a|b)*c", &long), None);
    assert_eq!(find("(a|b)*", &long), Some((0, 200_000)));
}

#[test]
fn special_escapes() {
    assert_eq!(find("\\R", "a\r\nb"), Some((1, 3)));
    assert_eq!(find("\\h+", "a \t b"), Some((1, 4)));
    assert_eq!(find("\\v", "a\u{2028}"), Some((1, 4)));
    assert_eq!(find("\\N+", "ab\ncd"), Some((0, 2)));
    assert_eq!(find("\\x{263a}", "\u{263a}"), Some((0, 3)));
    assert_eq!(find("\\o{101}\\101\\x41", "AAA"), Some((0, 3)));
    assert_eq!(find("\\cA", "\u{1}"), Some((0, 1)));
    assert_eq!(find("\\Qa.b\\E+", "a.bb"), Some((0, 4)));
    assert_eq!(find("\\N{U+41}", "A"), Some((0, 1)));
}

#[test]
fn grapheme_clusters() {
    assert_eq!(find("\\X", "e\u{301}x"), Some((0, 3)));
    assert_eq!(find("\\X", "\u{1f468}\u{200d}\u{1f469}"), Some((0, 11)));
    assert_eq!(
        find("\\X", "\u{1f1ef}\u{1f1f5}\u{1f1fa}\u{1f1f8}"),
        Some((0, 16))
    );
}

#[test]
fn script_runs() {
    assert_eq!(find("(*sr:\\w+)", "ab\u{3b1}"), Some((0, 2)));
    assert_eq!(find("(*atomic_script_run:\\d+)", "12"), Some((0, 2)));
}

#[test]
fn scan_substring() {
    assert_eq!(find("(\\w+) (*scs:(1)a\\w*)", "x abc"), None);
    assert_eq!(find("(\\w+) (*scs:(1)a\\w*)", "abc x"), Some((0, 4)));
}

#[test]
fn invalid_utf8_never_matches_characters() {
    assert_eq!(find_bytes(".", b"\xff"), None);
    assert_eq!(find_bytes("a.c", b"a\xffc abc"), Some((4, 7)));
    assert_eq!(find_bytes("\\w+", b"\xc3(ab"), Some((2, 4)));
    assert_eq!(find_bytes("[^a]", b"\x80a"), None);
    assert_eq!(find_bytes("\\xff", b"\xff"), None);
}

#[test]
fn invalid_utf8_empty_matches_inside_sequences() {
    let r = re("x?");
    assert_eq!(find_all_with(&r, b"\xc3\xa9"), vec![(0, 0), (1, 1), (2, 2)]);
}

#[test]
fn byte_mode_matches_raw_bytes() {
    let r = Regex::new("\\xff.", bcfg()).unwrap();
    assert_eq!(r.find_at(b"a\xff\xfe", 0).unwrap(), Some((1, 3)));
    let w = Regex::new("\\w+", bcfg()).unwrap();
    assert_eq!(w.find_at("\u{e9}ab".as_bytes(), 0).unwrap(), Some((2, 4)));
}

#[test]
fn utf_without_ucp_rejects_invalid_subjects() {
    let c = Config {
        utf: true,
        ..Config::default()
    };
    let r = Regex::new("a", c).unwrap();
    let e = r.find_at(b"a\xff", 0).unwrap_err();
    assert_eq!(
        e.to_string(),
        "PCRE2: error matching: UTF-8 error: illegal byte (0xfe or 0xff)"
    );
    let m = r.find_at("\u{e9}a".as_bytes(), 1).unwrap_err();
    assert_eq!(
        m.to_string(),
        "PCRE2: error matching: bad offset into UTF string"
    );
}

#[test]
fn compile_errors_match_pcre2_text() {
    assert_eq!(
        compile_err("a**"),
        "PCRE2: error compiling pattern at offset 3: quantifier does not follow a repeatable item"
    );
    assert_eq!(
        compile_err("(a"),
        "PCRE2: error compiling pattern at offset 2: missing closing parenthesis"
    );
    assert_eq!(
        compile_err(")"),
        "PCRE2: error compiling pattern at offset 1: unmatched closing parenthesis"
    );
    assert_eq!(
        compile_err("[a"),
        "PCRE2: error compiling pattern at offset 2: missing terminating ] for character class"
    );
    assert_eq!(
        compile_err("\\2(a)"),
        "PCRE2: error compiling pattern at offset 2: reference to non-existent subpattern"
    );
    assert_eq!(
        compile_err("(?<=a+)b"),
        "PCRE2: error compiling pattern at offset 0: length of lookbehind assertion is not limited"
    );
    assert_eq!(
        compile_err("\\p{Nope}"),
        "PCRE2: error compiling pattern at offset 8: unknown property after \\P or \\p"
    );
    assert_eq!(
        compile_err("x{3,2}"),
        "PCRE2: error compiling pattern at offset 5: numbers out of order in {} quantifier"
    );
    assert_eq!(
        compile_err("\\g{1"),
        "PCRE2: error compiling pattern at offset 4: syntax error in subpattern number (missing terminator?)"
    );
    assert_eq!(
        compile_err("(?1(x))(a)"),
        "PCRE2: error compiling pattern at offset 4: expected capture group number or name"
    );
    assert_eq!(
        compile_err("(*scs:x)"),
        "PCRE2: error compiling pattern at offset 6: missing opening parenthesis"
    );
    let e: Error = Regex::new("(", ucfg()).unwrap_err();
    assert_eq!(e.kind(), ErrorKind::Compile);
    assert_eq!(e.offset(), Some(1));
}

#[test]
fn escape_quotes_metacharacters() {
    assert_eq!(escape("a.b*c"), "a\\.b\\*c");
    assert_eq!(escape("(x)[y]{z}"), "\\(x\\)\\[y\\]\\{z\\}");
    assert_eq!(escape("#-^$|?+\\"), "\\#\\-\\^\\$\\|\\?\\+\\\\");
    let r = re(&escape("1+1=2?"));
    assert_eq!(r.find_at(b"is 1+1=2?", 0).unwrap(), Some((3, 9)));
}

#[test]
fn regex_is_send_sync_clone() {
    fn assert_traits<T: Send + Sync + Clone>() {}
    assert_traits::<Regex>();
    let r = re("a+");
    let c = r.clone();
    let h = std::thread::spawn(move || c.find_at(b"baa", 0).unwrap());
    assert_eq!(h.join().unwrap(), Some((1, 3)));
    assert_eq!(r.find_at(b"aa", 0).unwrap(), Some((0, 2)));
}

#[test]
fn prefilters_do_not_change_results() {
    let h = "fn main() { let unwrap = x.unwrap(); panic!(); todo!() }";
    assert_eq!(
        find_all("unsafe|unwrap|expect|panic|todo", h),
        vec![(16, 22), (27, 33), (37, 42), (47, 51)]
    );
    assert_eq!(find_all("(?<=fn )\\w+(?=\\()", h), vec![(3, 7)]);
    assert_eq!(find_all("\\w{5,}", h), vec![(16, 22), (27, 33), (37, 42)]);
    assert_eq!(find_all("(?i)PANIC|TODO", h), vec![(37, 42), (47, 51)]);
}

fn with(p: &str, c: &Config, h: &[u8], opts: MatchOptions) -> Option<(usize, usize)> {
    Regex::new(p, *c).unwrap().find_at_with(h, 0, opts).unwrap()
}

#[test]
fn dollar_endonly_ignores_final_newline() {
    let c = Config {
        dollar_endonly: true,
        ..Config::default()
    };
    let none = MatchOptions::default();
    assert_eq!(with("a$", &c, b"a\n", none), None);
    assert_eq!(with("a$", &c, b"a", none), Some((0, 1)));
    let m = Config {
        multi_line: true,
        ..c
    };
    assert_eq!(with("a$", &m, b"a\n", none), Some((0, 1)));
}

#[test]
fn not_bol_and_not_eol_affect_only_circumflex_and_dollar() {
    let c = Config::default();
    let m = Config {
        multi_line: true,
        ..c
    };
    let end_off = MatchOptions {
        not_eol: true,
        ..MatchOptions::default()
    };
    let start_off = MatchOptions {
        not_bol: true,
        ..MatchOptions::default()
    };
    assert_eq!(with("a$", &c, b"a", end_off), None);
    assert_eq!(with("a$", &c, b"a\n", end_off), None);
    assert_eq!(with("a\\z", &c, b"a", end_off), Some((0, 1)));
    assert_eq!(with("b$", &m, b"a\nb", end_off), None);
    assert_eq!(with("a$", &m, b"a\nb", end_off), Some((0, 1)));
    assert_eq!(with("^a", &c, b"a", start_off), None);
    assert_eq!(with("\\Aa", &c, b"a", start_off), Some((0, 1)));
    assert_eq!(with("^b", &m, b"a\nb", start_off), Some((2, 3)));
    assert_eq!(with("^a", &m, b"a\nb", start_off), None);
    let r = Regex::new("^(a)$", c).unwrap();
    let mut locs = r.capture_locations();
    assert_eq!(
        r.captures_read_at_with(&mut locs, b"a", 0, start_off)
            .unwrap(),
        None
    );
    assert!(
        r.is_match_at_with(b"a", 0, MatchOptions::default())
            .unwrap()
    );
    assert!(!r.is_match_at_with(b"a", 0, end_off).unwrap());
}

#[test]
fn ascii_bsd_restricts_only_backslash_d() {
    let c = Config {
        utf: true,
        ucp: true,
        ascii_bsd: true,
        ..Config::default()
    };
    let none = MatchOptions::default();
    let h = "\u{661}1".as_bytes();
    assert_eq!(with("\\d", &c, h, none), Some((2, 3)));
    assert_eq!(with("\\D", &c, h, none), Some((0, 2)));
    assert_eq!(with("[[:digit:]]", &c, h, none), Some((0, 2)));
    let u = Config {
        ascii_bsd: false,
        ..c
    };
    assert_eq!(with("\\d", &u, h, none), Some((0, 2)));
}

#[test]
fn match_line_wraps_the_whole_pattern() {
    let c = Config {
        match_line: true,
        ..Config::default()
    };
    let none = MatchOptions::default();
    assert_eq!(with("ab|c", &c, b"ab", none), Some((0, 2)));
    assert_eq!(with("ab|c", &c, b"abc", none), None);
    assert_eq!(with("ab|c", &c, b"c", none), Some((0, 1)));
    assert_eq!(with("ab|c", &c, b"ab\n", none), Some((0, 2)));
    assert_eq!(with("(?m)a", &c, b"a\nb", none), None);
    assert_eq!(with("(a)|b(*ACCEPT)c", &c, b"bz", none), Some((0, 1)));
    assert_eq!(with("x(?R)?y", &c, b"xxyy", none), None);
    let m = Config {
        multi_line: true,
        ..c
    };
    assert_eq!(with("ab|c", &m, b"x\nc\n", none), Some((2, 3)));
    let e = Config {
        dollar_endonly: true,
        ..c
    };
    assert_eq!(with("ab|c", &e, b"ab\n", none), None);
}

#[test]
fn jit_unsupported_patterns_use_interpreter_semantics() {
    let code = |p: &str, c: &Config| {
        Regex::new(p, *c)
            .unwrap()
            .find_at(b"b", 0)
            .unwrap_err()
            .code()
    };
    assert_eq!(code("(*napla:(*ACCEPT))(a|(?1))", &ucfg()), -52);
    assert_eq!(code("(?=(*ACCEPT))(a|(?1))", &ucfg()), -46);
    assert_eq!(code("(*NO_JIT)(a|(?1))", &bcfg()), -52);
    assert_eq!(code("(a|(?1))", &bcfg()), -46);
}

use happy_cracking::crypto::strgen::{
    BruteParams, CharsetPreset, HARD_CANDIDATE_LIMIT, MAX_EXHAUSTIVE_LEN, MaskParams, RandomParams,
    Xoshiro256StarStar, brute_keyspace, dedup_chars, expand_mask, mask_keyspace, preset_charset,
    write_brute, write_mask, write_random,
};
use std::time::Instant;

fn lines(bytes: &[u8]) -> Vec<String> {
    String::from_utf8(bytes.to_vec())
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn brute_charset_range_is_lexicographic() {
    let chars: Vec<char> = "ab".chars().collect();
    let mut out = Vec::new();
    let n = write_brute(
        &mut out,
        &BruteParams {
            chars: &chars,
            min_len: 1,
            max_len: 2,
            prefix: "",
            suffix: "",
            limit: HARD_CANDIDATE_LIMIT,
        },
    )
    .unwrap();
    assert_eq!(n, 6);
    assert_eq!(lines(&out), ["a", "b", "aa", "ab", "ba", "bb"]);
}

#[test]
fn brute_dedups_charset_preserving_order() {
    assert_eq!(dedup_chars("aabca"), ['a', 'b', 'c']);
    let chars = dedup_chars("aab");
    let mut out = Vec::new();
    write_brute(
        &mut out,
        &BruteParams {
            chars: &chars,
            min_len: 1,
            max_len: 1,
            prefix: "",
            suffix: "",
            limit: 100,
        },
    )
    .unwrap();
    assert_eq!(lines(&out), ["a", "b"]);
}

#[test]
fn brute_prefix_and_suffix_stay_fixed() {
    let chars: Vec<char> = "01".chars().collect();
    let mut out = Vec::new();
    write_brute(
        &mut out,
        &BruteParams {
            chars: &chars,
            min_len: 2,
            max_len: 2,
            prefix: "flag{",
            suffix: "}",
            limit: 100,
        },
    )
    .unwrap();
    assert_eq!(
        lines(&out),
        ["flag{00}", "flag{01}", "flag{10}", "flag{11}"]
    );
}

#[test]
fn brute_unicode_charset() {
    let chars: Vec<char> = "あい".chars().collect();
    let mut out = Vec::new();
    write_brute(
        &mut out,
        &BruteParams {
            chars: &chars,
            min_len: 1,
            max_len: 2,
            prefix: "",
            suffix: "",
            limit: 100,
        },
    )
    .unwrap();
    assert_eq!(lines(&out), ["あ", "い", "ああ", "あい", "いあ", "いい"]);
}

#[test]
fn brute_zero_length_emits_affixes_once() {
    let chars: Vec<char> = "a".chars().collect();
    let mut out = Vec::new();
    let n = write_brute(
        &mut out,
        &BruteParams {
            chars: &chars,
            min_len: 0,
            max_len: 0,
            prefix: "fixed",
            suffix: "",
            limit: 10,
        },
    )
    .unwrap();
    assert_eq!(n, 1);
    assert_eq!(lines(&out), ["fixed"]);
}

#[test]
fn brute_keyspace_sums_lengths() {
    assert_eq!(brute_keyspace(2, 1, 2).unwrap(), 6);
    assert_eq!(brute_keyspace(10, 0, 0).unwrap(), 1);
}

#[test]
fn brute_rejects_empty_charset_and_bad_range() {
    let err = write_brute(
        &mut Vec::new(),
        &BruteParams {
            chars: &[],
            min_len: 1,
            max_len: 1,
            prefix: "",
            suffix: "",
            limit: 10,
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains("empty"));

    let chars = vec!['a'];
    let err = write_brute(
        &mut Vec::new(),
        &BruteParams {
            chars: &chars,
            min_len: 3,
            max_len: 1,
            prefix: "",
            suffix: "",
            limit: 10,
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains("max-len"));
}

#[test]
fn brute_rejects_oversized_length_quickly() {
    let chars: Vec<char> = "ab".chars().collect();
    let started = Instant::now();
    let err = write_brute(
        &mut Vec::new(),
        &BruteParams {
            chars: &chars,
            min_len: 1,
            max_len: 10_000_000,
            prefix: "",
            suffix: "",
            limit: HARD_CANDIDATE_LIMIT,
        },
    )
    .unwrap_err();
    assert!(started.elapsed().as_secs() < 2);
    assert!(err.to_string().contains(&MAX_EXHAUSTIVE_LEN.to_string()));
}

#[test]
fn brute_rejects_keyspace_over_limit() {
    let chars: Vec<char> = "ab".chars().collect();
    let err = write_brute(
        &mut Vec::new(),
        &BruteParams {
            chars: &chars,
            min_len: 1,
            max_len: 20,
            prefix: "",
            suffix: "",
            limit: 1000,
        },
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("1000"));
    assert!(msg.contains("Denial of Service"));
}

#[test]
fn mask_literals_and_digit_class() {
    let positions = expand_mask("a?d", &[None, None, None, None]).unwrap();
    assert_eq!(mask_keyspace(&positions).unwrap(), 10);
    let mut out = Vec::new();
    write_mask(
        &mut out,
        &MaskParams {
            mask: "a?d",
            custom: [None, None, None, None],
            limit: 100,
        },
    )
    .unwrap();
    let got = lines(&out);
    assert_eq!(got.first().map(String::as_str), Some("a0"));
    assert_eq!(got.last().map(String::as_str), Some("a9"));
    assert_eq!(got.len(), 10);
}

#[test]
fn mask_fixed_and_custom_charset() {
    let mut out = Vec::new();
    write_mask(
        &mut out,
        &MaskParams {
            mask: "id?1?1",
            custom: [Some("ab"), None, None, None],
            limit: 100,
        },
    )
    .unwrap();
    assert_eq!(lines(&out), ["idaa", "idab", "idba", "idbb"]);
}

#[test]
fn mask_escaped_question_and_unicode_literal() {
    let mut out = Vec::new();
    write_mask(
        &mut out,
        &MaskParams {
            mask: "猫??",
            custom: [None, None, None, None],
            limit: 10,
        },
    )
    .unwrap();
    assert_eq!(lines(&out), ["猫?"]);
}

#[test]
fn mask_classes_have_expected_sizes() {
    let lower = expand_mask("?l", &[None, None, None, None]).unwrap();
    let upper = expand_mask("?u", &[None, None, None, None]).unwrap();
    let hex = expand_mask("?h", &[None, None, None, None]).unwrap();
    let all = expand_mask("?a", &[None, None, None, None]).unwrap();
    assert_eq!(lower[0].len(), 26);
    assert_eq!(upper[0].len(), 26);
    assert_eq!(hex[0].len(), 16);
    assert_eq!(all[0].len(), 95);
    assert_eq!(lower[0][0], 'a');
    assert_eq!(hex[0][0], '0');
}

#[test]
fn mask_rejects_bad_syntax_and_missing_custom() {
    assert!(expand_mask("?z", &[None, None, None, None]).is_err());
    assert!(expand_mask("abc?", &[None, None, None, None]).is_err());
    assert!(expand_mask("", &[None, None, None, None]).is_err());
    let err = expand_mask("?1", &[None, None, None, None]).unwrap_err();
    assert!(err.to_string().contains("?1"));
}

#[test]
fn mask_rightmost_position_varies_fastest() {
    let mut out = Vec::new();
    write_mask(
        &mut out,
        &MaskParams {
            mask: "?d?d",
            custom: [None, None, None, None],
            limit: 200,
        },
    )
    .unwrap();
    let got = lines(&out);
    assert_eq!(&got[..3], ["00", "01", "02"]);
    assert_eq!(got[10], "10");
    assert_eq!(got.len(), 100);
}

#[test]
fn random_is_deterministic_and_respects_charset() {
    let chars: Vec<char> = preset_charset(CharsetPreset::Digits).chars().collect();
    let params = RandomParams {
        chars: &chars,
        min_len: 8,
        max_len: 8,
        count: 5,
        seed: 42,
        prefix: "pw",
        suffix: "!",
    };
    let mut a = Vec::new();
    let mut b = Vec::new();
    write_random(&mut a, &params).unwrap();
    write_random(&mut b, &params).unwrap();
    assert_eq!(a, b);
    let got = lines(&a);
    assert_eq!(got.len(), 5);
    for line in &got {
        assert!(line.starts_with("pw") && line.ends_with('!'));
        let body = &line[2..line.len() - 1];
        assert_eq!(body.chars().count(), 8);
        assert!(body.chars().all(|c| chars.contains(&c)));
    }
    let mut other = Vec::new();
    write_random(&mut other, &RandomParams { seed: 99, ..params }).unwrap();
    assert_ne!(a, other);
}

#[test]
fn random_length_stays_inside_range() {
    let chars: Vec<char> = "xyz".chars().collect();
    let mut out = Vec::new();
    write_random(
        &mut out,
        &RandomParams {
            chars: &chars,
            min_len: 2,
            max_len: 4,
            count: 30,
            seed: 7,
            prefix: "",
            suffix: "",
        },
    )
    .unwrap();
    let got = lines(&out);
    assert_eq!(got.len(), 30);
    assert!(got.iter().all(|s| {
        let n = s.chars().count();
        (2..=4).contains(&n) && s.chars().all(|c| chars.contains(&c))
    }));
}

#[test]
fn random_rejects_zero_count_and_empty_charset() {
    let chars = vec!['a'];
    let err = write_random(
        &mut Vec::new(),
        &RandomParams {
            chars: &chars,
            min_len: 1,
            max_len: 1,
            count: 0,
            seed: 1,
            prefix: "",
            suffix: "",
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains("count"));

    let err = write_random(
        &mut Vec::new(),
        &RandomParams {
            chars: &[],
            min_len: 1,
            max_len: 1,
            count: 1,
            seed: 1,
            prefix: "",
            suffix: "",
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains("empty"));
}

#[test]
fn random_unicode_round_length() {
    let chars: Vec<char> = "あいう".chars().collect();
    let mut out = Vec::new();
    write_random(
        &mut out,
        &RandomParams {
            chars: &chars,
            min_len: 3,
            max_len: 3,
            count: 4,
            seed: 1,
            prefix: "[",
            suffix: "]",
        },
    )
    .unwrap();
    for line in lines(&out) {
        assert!(line.starts_with('[') && line.ends_with(']'));
        let body: Vec<char> = line.chars().skip(1).take(3).collect();
        assert_eq!(body.len(), 3);
        assert!(body.iter().all(|c| chars.contains(c)));
    }
}

#[test]
fn xoshiro_matches_reference_vectors() {
    let mut rng = Xoshiro256StarStar::from_state([1, 2, 3, 4]);
    assert_eq!(rng.next_u64(), 0x2d00);
    assert_eq!(rng.next_u64(), 0);
    assert_eq!(rng.next_u64(), 0x5a00_7080);
}

#[test]
fn presets_cover_common_sets() {
    assert_eq!(preset_charset(CharsetPreset::Digits), "0123456789");
    assert_eq!(preset_charset(CharsetPreset::Lower).len(), 26);
    assert_eq!(preset_charset(CharsetPreset::Hex).len(), 16);
    assert!(preset_charset(CharsetPreset::Alnum).contains('Z'));
    assert!(preset_charset(CharsetPreset::All).contains(' '));
}

#[test]
fn ascii_brute_emits_a_million_candidates_quickly() {
    let chars: Vec<char> = "0123456789abcdef".chars().collect();
    let started = Instant::now();
    let mut out = Vec::new();
    let n = write_brute(
        &mut out,
        &BruteParams {
            chars: &chars,
            min_len: 5,
            max_len: 5,
            prefix: "",
            suffix: "",
            limit: HARD_CANDIDATE_LIMIT,
        },
    )
    .unwrap();
    assert_eq!(n, 1_048_576);
    assert!(out.ends_with(b"fffff\n"));
    assert!(out.starts_with(b"00000\n"));
    assert!(started.elapsed().as_secs() < 3);
}

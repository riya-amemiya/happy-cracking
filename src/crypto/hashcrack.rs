use anyhow::{Context, Result};
use clap::{Subcommand, ValueEnum};
use md4::Md4;
use md5::Md5;
use rayon::prelude::*;
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};
use std::path::PathBuf;

const MAX_BRUTE_SPACE: u128 = 1_000_000_000;

const MAX_CHARSET_LEN: usize = 256;

/// Stack buffer for ASCII mask/brute candidates (`?l`/`?u`/`?d`/`?s`/`?a`/`?h`).
/// 32 bytes covers typical CTF masks; longer or non-ASCII inputs fall back to `String`.
const MAX_STACK_MASK: usize = 32;

#[derive(Clone, Copy, ValueEnum)]
pub enum HashAlgo {
    Md5,
    Sha1,
    Sha256,
    Sha512,
    Md4,
    Ntlm,
}

impl HashAlgo {
    fn hex_len(self) -> usize {
        match self {
            HashAlgo::Md5 | HashAlgo::Md4 | HashAlgo::Ntlm => 32,
            HashAlgo::Sha1 => 40,
            HashAlgo::Sha256 => 64,
            HashAlgo::Sha512 => 128,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
pub enum SaltPosition {
    Prefix,
    Suffix,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum CharsetPreset {
    Lower,
    Upper,
    Digits,
    Alnum,
    All,
}

#[derive(Subcommand)]
pub enum HashcrackAction {
    #[command(about = "Dictionary attack against a target hash using a wordlist file")]
    Dict {
        #[arg(help = "Target hash (case-insensitive hex)")]
        hash: String,
        #[arg(short, long, help = "Wordlist file, one candidate per line")]
        wordlist: PathBuf,
        #[arg(
            short,
            long,
            value_enum,
            help = "Hash algorithm (auto-detect if omitted)"
        )]
        algo: Option<HashAlgo>,
        #[arg(short, long, help = "Salt to combine with each candidate")]
        salt: Option<String>,
        #[arg(
            long,
            value_enum,
            default_value = "suffix",
            help = "Where to place the salt relative to the candidate"
        )]
        salt_position: SaltPosition,
    },
    #[command(about = "Incremental brute-force attack against a target hash")]
    Brute {
        #[arg(help = "Target hash (case-insensitive hex)")]
        hash: String,
        #[arg(short, long, value_enum, help = "Hash algorithm")]
        algo: HashAlgo,
        #[arg(
            short,
            long,
            default_value = "abcdefghijklmnopqrstuvwxyz0123456789",
            help = "Characters to try (ignored when --preset is given)"
        )]
        charset: String,
        #[arg(
            short,
            long,
            value_enum,
            help = "Predefined charset overriding --charset"
        )]
        preset: Option<CharsetPreset>,
        #[arg(long, default_value = "1", help = "Minimum candidate length")]
        min_len: usize,
        #[arg(long, default_value = "4", help = "Maximum candidate length")]
        max_len: usize,
        #[arg(short, long, help = "Salt to combine with each candidate")]
        salt: Option<String>,
        #[arg(
            long,
            value_enum,
            default_value = "suffix",
            help = "Where to place the salt relative to the candidate"
        )]
        salt_position: SaltPosition,
    },
    #[command(about = "Reverse-lookup a hash in a precomputed table file")]
    Lookup {
        #[arg(help = "Target hash (case-insensitive hex)")]
        hash: String,
        #[arg(short, long, help = "Table file with `hash<sep>plaintext` lines")]
        table: PathBuf,
    },
    #[command(about = "Dictionary attack with hashcat-style rules applied to each word")]
    Rule {
        #[arg(help = "Target hash (case-insensitive hex)")]
        hash: String,
        #[arg(short, long, help = "Wordlist file")]
        wordlist: PathBuf,
        #[arg(
            short,
            long,
            help = "Rules file (one rule per line); if omitted, built-in rules are used"
        )]
        rules: Option<PathBuf>,
        #[arg(
            short,
            long,
            value_enum,
            help = "Hash algorithm (auto-detect if omitted)"
        )]
        algo: Option<HashAlgo>,
    },
    #[command(about = "Mask attack (?l ?u ?d ?s ?a and literals, hashcat-style)")]
    Mask {
        #[arg(help = "Target hash (case-insensitive hex)")]
        hash: String,
        #[arg(short, long, help = "Mask, e.g. '?l?l?l?d?d' or 'flag{?d?d?d}'")]
        mask: String,
        #[arg(short, long, value_enum, help = "Hash algorithm")]
        algo: HashAlgo,
    },
    #[command(about = "Hybrid attack: wordlist candidates with numeric suffixes")]
    Hybrid {
        #[arg(help = "Target hash (case-insensitive hex)")]
        hash: String,
        #[arg(short, long, help = "Wordlist file")]
        wordlist: PathBuf,
        #[arg(
            short,
            long,
            value_enum,
            help = "Hash algorithm (auto-detect if omitted)"
        )]
        algo: Option<HashAlgo>,
        #[arg(long, default_value = "0", help = "Minimum suffix digits (inclusive)")]
        min_digits: u32,
        #[arg(long, default_value = "2", help = "Maximum suffix digits (inclusive)")]
        max_digits: u32,
        #[arg(long, help = "Also try prefixes instead of only suffixes")]
        also_prefix: bool,
    },
}

pub fn run(action: HashcrackAction) -> Result<()> {
    match action {
        HashcrackAction::Dict {
            hash,
            wordlist,
            algo,
            salt,
            salt_position,
        } => run_dict(&hash, &wordlist, algo, salt.as_deref(), salt_position),
        HashcrackAction::Brute {
            hash,
            algo,
            charset,
            preset,
            min_len,
            max_len,
            salt,
            salt_position,
        } => {
            let charset = match preset {
                Some(p) => preset_charset(p).to_string(),
                None => charset,
            };
            run_brute(
                &hash,
                algo,
                &charset,
                min_len,
                max_len,
                salt.as_deref(),
                salt_position,
            )
        }
        HashcrackAction::Lookup { hash, table } => run_lookup(&hash, &table),
        HashcrackAction::Rule {
            hash,
            wordlist,
            rules,
            algo,
        } => run_rule(&hash, &wordlist, rules.as_ref(), algo),
        HashcrackAction::Mask { hash, mask, algo } => run_mask(&hash, &mask, algo),
        HashcrackAction::Hybrid {
            hash,
            wordlist,
            algo,
            min_digits,
            max_digits,
            also_prefix,
        } => run_hybrid(&hash, &wordlist, algo, min_digits, max_digits, also_prefix),
    }
}

fn run_dict(
    hash: &str,
    wordlist: &PathBuf,
    algo: Option<HashAlgo>,
    salt: Option<&str>,
    pos: SaltPosition,
) -> Result<()> {
    let target = normalize_hash(hash);
    let candidates = read_wordlist(wordlist)?;

    let algos = match algo {
        Some(a) => vec![a],
        None => algos_for_hex_len(target.len()),
    };
    if algos.is_empty() {
        anyhow::bail!(
            "Could not auto-detect an algorithm for a {}-character hash; pass --algo explicitly",
            target.len()
        );
    }

    for a in algos {
        if let Some(found) = find_in_candidates(&target, a, salt, pos, &candidates) {
            println!("Found: {}", found);
            return Ok(());
        }
    }
    println!("Not found");
    Ok(())
}

fn run_brute(
    hash: &str,
    algo: HashAlgo,
    charset: &str,
    min_len: usize,
    max_len: usize,
    salt: Option<&str>,
    pos: SaltPosition,
) -> Result<()> {
    let target = normalize_hash(hash);
    match brute_force(&target, algo, charset, min_len, max_len, salt, pos)? {
        Some(found) => println!("Found: {}", found),
        None => println!("Not found"),
    }
    Ok(())
}

fn run_lookup(hash: &str, table: &PathBuf) -> Result<()> {
    let target = normalize_hash(hash);
    match lookup_in_table_file(&target, table)? {
        Some(plain) => println!("Found: {}", plain),
        None => println!("Not found"),
    }
    Ok(())
}

fn hash_digest<D: Digest>(input: &[u8]) -> impl AsRef<[u8]> {
    let mut hasher = D::new();
    hasher.update(input);
    hasher.finalize()
}

fn ntlm_digest(input: &str) -> impl AsRef<[u8]> {
    let mut hasher = Md4::new();
    for unit in input.encode_utf16() {
        hasher.update(unit.to_le_bytes());
    }
    hasher.finalize()
}

fn digest_matches(algo: HashAlgo, input: &str, target: &[u8]) -> bool {
    match algo {
        HashAlgo::Md5 => hash_digest::<Md5>(input.as_bytes()).as_ref() == target,
        HashAlgo::Sha1 => hash_digest::<Sha1>(input.as_bytes()).as_ref() == target,
        HashAlgo::Sha256 => hash_digest::<Sha256>(input.as_bytes()).as_ref() == target,
        HashAlgo::Sha512 => hash_digest::<Sha512>(input.as_bytes()).as_ref() == target,
        HashAlgo::Md4 => hash_digest::<Md4>(input.as_bytes()).as_ref() == target,
        HashAlgo::Ntlm => ntlm_digest(input).as_ref() == target,
    }
}

fn decode_target_digest(target: &str) -> Option<Vec<u8>> {
    hex::decode(normalize_hash(target)).ok()
}

fn candidate_matches(
    algo: HashAlgo,
    word: &str,
    salt: Option<&str>,
    pos: SaltPosition,
    target: &[u8],
) -> bool {
    match salt {
        None => digest_matches(algo, word, target),
        Some(_) => digest_matches(algo, &apply_salt(word, salt, pos), target),
    }
}

pub fn compute_hash(algo: HashAlgo, input: &str) -> String {
    match algo {
        HashAlgo::Md5 => hex::encode(hash_digest::<Md5>(input.as_bytes())),
        HashAlgo::Sha1 => hex::encode(hash_digest::<Sha1>(input.as_bytes())),
        HashAlgo::Sha256 => hex::encode(hash_digest::<Sha256>(input.as_bytes())),
        HashAlgo::Sha512 => hex::encode(hash_digest::<Sha512>(input.as_bytes())),
        HashAlgo::Md4 => hex::encode(hash_digest::<Md4>(input.as_bytes())),
        HashAlgo::Ntlm => hex::encode(ntlm_digest(input)),
    }
}

fn apply_salt(word: &str, salt: Option<&str>, pos: SaltPosition) -> String {
    match salt {
        None => word.to_string(),
        Some(s) => match pos {
            SaltPosition::Prefix => format!("{}{}", s, word),
            SaltPosition::Suffix => format!("{}{}", word, s),
        },
    }
}

fn normalize_hash(hash: &str) -> String {
    hash.trim().to_ascii_lowercase()
}

fn algos_for_hex_len(hex_len: usize) -> Vec<HashAlgo> {
    [
        HashAlgo::Md5,
        HashAlgo::Md4,
        HashAlgo::Ntlm,
        HashAlgo::Sha1,
        HashAlgo::Sha256,
        HashAlgo::Sha512,
    ]
    .into_iter()
    .filter(|a| a.hex_len() == hex_len)
    .collect()
}

fn preset_charset(preset: CharsetPreset) -> &'static str {
    match preset {
        CharsetPreset::Lower => "abcdefghijklmnopqrstuvwxyz",
        CharsetPreset::Upper => "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
        CharsetPreset::Digits => "0123456789",
        CharsetPreset::Alnum => "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789",
        CharsetPreset::All => {
            "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 !\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~"
        }
    }
}

fn read_wordlist(path: &PathBuf) -> Result<Vec<String>> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("Failed to read wordlist: {}", path.display()))?;
    Ok(bytes
        .split(|&b| b == b'\n')
        .map(|line| {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            String::from_utf8_lossy(line).into_owned()
        })
        .filter(|line| !line.is_empty())
        .collect())
}

pub fn find_in_candidates(
    target: &str,
    algo: HashAlgo,
    salt: Option<&str>,
    pos: SaltPosition,
    candidates: &[String],
) -> Option<String> {
    let target = decode_target_digest(target)?;
    candidates.par_iter().find_map_any(|word| {
        if candidate_matches(algo, word, salt, pos, &target) {
            Some(word.clone())
        } else {
            None
        }
    })
}

pub fn brute_force(
    target: &str,
    algo: HashAlgo,
    charset: &str,
    min_len: usize,
    max_len: usize,
    salt: Option<&str>,
    pos: SaltPosition,
) -> Result<Option<String>> {
    let chars: Vec<char> = charset.chars().collect();
    if chars.is_empty() {
        anyhow::bail!("Charset must not be empty");
    }
    if chars.len() > MAX_CHARSET_LEN {
        anyhow::bail!(
            "Charset too large ({} characters); maximum is {}",
            chars.len(),
            MAX_CHARSET_LEN
        );
    }
    if min_len == 0 {
        anyhow::bail!("--min-len must be at least 1");
    }
    if max_len < min_len {
        anyhow::bail!("--max-len ({}) must be >= --min-len ({})", max_len, min_len);
    }

    let base = chars.len() as u128;
    let mut total: u128 = 0;
    for len in min_len..=max_len {
        let count = base
            .checked_pow(len as u32)
            .context("Brute-force search space overflowed")?;
        total = total
            .checked_add(count)
            .context("Brute-force search space overflowed")?;
        if total > MAX_BRUTE_SPACE {
            anyhow::bail!(
                "Search space exceeds the limit of {} candidates; narrow the charset or length range",
                MAX_BRUTE_SPACE
            );
        }
    }

    // Decode after parameter validation so a malformed target cannot hide
    // charset/length/search-space errors behind Ok(None).
    let Some(target) = decode_target_digest(target) else {
        return Ok(None);
    };

    let ascii_charset: Option<Vec<u8>> = if chars.iter().all(|c| c.is_ascii()) {
        Some(chars.iter().map(|&c| c as u8).collect())
    } else {
        None
    };

    for len in min_len..=max_len {
        let count = base.pow(len as u32);
        let found = if let (Some(charset), true) = (ascii_charset.as_deref(), len <= MAX_STACK_MASK)
        {
            (0..count).into_par_iter().find_map_any(|index| {
                let mut buf = [0u8; MAX_STACK_MASK];
                fill_ascii_index(index, charset, len, &mut buf);
                let word = std::str::from_utf8(&buf[..len]).expect("ASCII charset bytes");
                if candidate_matches(algo, word, salt, pos, &target) {
                    Some(word.to_owned())
                } else {
                    None
                }
            })
        } else {
            (0..count).into_par_iter().find_map_any(|index| {
                let word = index_to_word(index, &chars, len);
                if candidate_matches(algo, &word, salt, pos, &target) {
                    Some(word)
                } else {
                    None
                }
            })
        };
        if let Some(found) = found {
            return Ok(Some(found));
        }
    }
    Ok(None)
}

/// Write mixed-radix `index` into `buf[..len]` using an ASCII charset.
/// Digits are MSD-first (same order as `index_to_word`).
fn fill_ascii_index(index: u128, charset: &[u8], len: usize, buf: &mut [u8]) {
    let base = charset.len() as u128;
    let mut remaining = index;
    for i in (0..len).rev() {
        buf[i] = charset[(remaining % base) as usize];
        remaining /= base;
    }
}

fn index_to_word(index: u128, chars: &[char], len: usize) -> String {
    let base = chars.len() as u128;
    let mut digits = vec![chars[0]; len];
    let mut remaining = index;
    for slot in digits.iter_mut().rev() {
        *slot = chars[(remaining % base) as usize];
        remaining /= base;
    }
    digits.into_iter().collect()
}

pub fn lookup_in_pairs<'a, I, S>(target: &str, pairs: I) -> Option<String>
where
    I: IntoIterator<Item = (S, S)>,
    S: AsRef<str> + 'a,
{
    let target = normalize_hash(target);
    for (hash, plain) in pairs {
        if normalize_hash(hash.as_ref()) == target {
            return Some(plain.as_ref().to_string());
        }
    }
    None
}

pub fn parse_table_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let (hash, plain) = if let Some((h, p)) = line.split_once(':') {
        (h, p)
    } else {
        line.split_once(char::is_whitespace)?
    };
    let hash = hash.trim();
    let plain = plain.trim();
    if hash.is_empty() {
        return None;
    }
    Some((hash.to_string(), plain.to_string()))
}

fn lookup_in_table_file(target: &str, path: &PathBuf) -> Result<Option<String>> {
    use std::io::BufRead;
    let file = std::fs::File::open(path)
        .with_context(|| format!("Failed to open table: {}", path.display()))?;
    let target = normalize_hash(target);
    let reader = std::io::BufReader::new(file);
    for line in reader.lines() {
        let line = line.context("Failed to read table line")?;
        if let Some((hash, plain)) = parse_table_line(&line)
            && normalize_hash(&hash) == target
        {
            return Ok(Some(plain));
        }
    }
    Ok(None)
}

const BUILTIN_RULES: &[&str] = &[
    ":", // identity
    "l", // lowercase
    "u", // uppercase
    "c", // capitalize
    "r", // reverse
    "d", // duplicate
    "$1",
    "$2",
    "$!",
    "$1$2$3",
    "^!",
    "c$1",
    "c$!",
    "u$1",
    "l$1",
    "l$2$0$2$4",
];

fn run_rule(
    hash: &str,
    wordlist: &PathBuf,
    rules_path: Option<&PathBuf>,
    algo: Option<HashAlgo>,
) -> Result<()> {
    let target = normalize_hash(hash);
    let words = read_wordlist(wordlist)?;
    let rules: Vec<String> = if let Some(path) = rules_path {
        read_wordlist(path)?
    } else {
        BUILTIN_RULES.iter().map(|s| (*s).to_string()).collect()
    };
    let algos = match algo {
        Some(a) => vec![a],
        None => algos_for_hex_len(target.len()),
    };
    if algos.is_empty() {
        anyhow::bail!(
            "Could not auto-detect an algorithm for a {}-character hash; pass --algo explicitly",
            target.len()
        );
    }

    for a in algos {
        if let Some(found) = rule_attack(&target, a, &words, &rules) {
            println!("Found: {}", found);
            return Ok(());
        }
    }
    println!("Not found");
    Ok(())
}

fn run_mask(hash: &str, mask: &str, algo: HashAlgo) -> Result<()> {
    let target = normalize_hash(hash);
    match mask_attack(&target, algo, mask)? {
        Some(found) => println!("Found: {}", found),
        None => println!("Not found"),
    }
    Ok(())
}

fn run_hybrid(
    hash: &str,
    wordlist: &PathBuf,
    algo: Option<HashAlgo>,
    min_digits: u32,
    max_digits: u32,
    also_prefix: bool,
) -> Result<()> {
    let target = normalize_hash(hash);
    let words = read_wordlist(wordlist)?;
    let algos = match algo {
        Some(a) => vec![a],
        None => algos_for_hex_len(target.len()),
    };
    if algos.is_empty() {
        anyhow::bail!(
            "Could not auto-detect an algorithm for a {}-character hash; pass --algo explicitly",
            target.len()
        );
    }
    for a in algos {
        if let Some(found) = hybrid_attack(&target, a, &words, min_digits, max_digits, also_prefix)?
        {
            println!("Found: {}", found);
            return Ok(());
        }
    }
    println!("Not found");
    Ok(())
}

/// Apply a minimal hashcat-like rule to a word.
/// Supported: `:` identity, `l` lower, `u` upper, `c` capitalize, `t` toggle,
/// `r` reverse, `d` duplicate, `$X` append, `^X` prepend.
pub fn apply_rule(word: &str, rule: &str) -> String {
    let mut out = word.to_string();
    let chars: Vec<char> = rule.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        match chars[i] {
            ':' => {}
            'l' => out = out.to_ascii_lowercase(),
            'u' => out = out.to_ascii_uppercase(),
            'c' => {
                let mut cs: Vec<char> = out.chars().collect();
                if let Some(first) = cs.first_mut() {
                    *first = first.to_ascii_uppercase();
                }
                for ch in cs.iter_mut().skip(1) {
                    *ch = ch.to_ascii_lowercase();
                }
                out = cs.into_iter().collect();
            }
            't' => {
                out = out
                    .chars()
                    .map(|c| {
                        if c.is_ascii_uppercase() {
                            c.to_ascii_lowercase()
                        } else {
                            c.to_ascii_uppercase()
                        }
                    })
                    .collect();
            }
            'r' => out = out.chars().rev().collect(),
            'd' => out = format!("{}{}", out, out),
            '$' => {
                i += 1;
                if i < chars.len() {
                    out.push(chars[i]);
                }
            }
            '^' => {
                i += 1;
                if i < chars.len() {
                    out.insert(0, chars[i]);
                }
            }
            _ => {
                // Unknown rule atom: ignore
            }
        }
        i += 1;
    }
    out
}

pub fn rule_attack(
    target: &str,
    algo: HashAlgo,
    words: &[String],
    rules: &[String],
) -> Option<String> {
    let target = decode_target_digest(target)?;
    let rules: Vec<&str> = rules.iter().map(|s| s.as_str()).collect();
    words.par_iter().find_map_any(|word| {
        for rule in &rules {
            let candidate = apply_rule(word, rule);
            if digest_matches(algo, &candidate, &target) {
                return Some(candidate);
            }
        }
        None
    })
}

/// Expand a mask into character class choices per position.
/// `?l` lower, `?u` upper, `?d` digits, `?s` special, `?a` all printable ascii,
/// `?h` hex lower, `??` literal `?`, otherwise literal char.
pub fn expand_mask(mask: &str) -> Result<Vec<Vec<char>>> {
    let chars: Vec<char> = mask.chars().collect();
    let mut positions: Vec<Vec<char>> = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '?' {
            i += 1;
            if i >= chars.len() {
                anyhow::bail!("Mask ends with dangling '?'");
            }
            let class = match chars[i] {
                'l' => "abcdefghijklmnopqrstuvwxyz".chars().collect(),
                'u' => "ABCDEFGHIJKLMNOPQRSTUVWXYZ".chars().collect(),
                'd' => "0123456789".chars().collect(),
                's' => " !\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~".chars().collect(),
                'a' => (0x20u8..=0x7eu8).map(|b| b as char).collect(),
                'h' => "0123456789abcdef".chars().collect(),
                '?' => vec!['?'],
                other => anyhow::bail!("Unknown mask class '?{}'", other),
            };
            positions.push(class);
        } else {
            positions.push(vec![chars[i]]);
        }
        i += 1;
    }
    if positions.is_empty() {
        anyhow::bail!("Mask is empty");
    }
    Ok(positions)
}

pub fn mask_attack(target: &str, algo: HashAlgo, mask: &str) -> Result<Option<String>> {
    let positions = expand_mask(mask)?;
    let mut total: u128 = 1;
    for pos in &positions {
        total = total
            .checked_mul(pos.len() as u128)
            .context("Mask search space overflowed")?;
        if total > MAX_BRUTE_SPACE {
            anyhow::bail!(
                "Mask search space exceeds the limit of {} candidates",
                MAX_BRUTE_SPACE
            );
        }
    }

    // Decode after mask/space validation so invalid hex cannot swallow
    // expand_mask or overflow errors.
    let Some(target) = decode_target_digest(target) else {
        return Ok(None);
    };

    let npos = positions.len();
    // Place value of position i = product of bases to its right.
    let mut places = vec![1u128; npos];
    for i in (0..npos.saturating_sub(1)).rev() {
        places[i] = places[i + 1]
            .checked_mul(positions[i + 1].len() as u128)
            .context("Mask place-value overflowed")?;
    }

    let stack_ok = npos <= MAX_STACK_MASK && positions.iter().flatten().all(|c| c.is_ascii());

    let found = if stack_ok {
        (0..total).into_par_iter().find_map_any(|index| {
            let mut rem = index;
            let mut buf = [0u8; MAX_STACK_MASK];
            for i in 0..npos {
                let d = (rem / places[i]) as usize;
                rem %= places[i];
                buf[i] = positions[i][d] as u8;
            }
            let word = std::str::from_utf8(&buf[..npos]).expect("ASCII mask bytes");
            if digest_matches(algo, word, &target) {
                Some(word.to_owned())
            } else {
                None
            }
        })
    } else {
        (0..total).into_par_iter().find_map_any(|index| {
            let mut rem = index;
            let mut word = String::with_capacity(npos);
            for i in 0..npos {
                let d = (rem / places[i]) as usize;
                rem %= places[i];
                word.push(positions[i][d]);
            }
            if digest_matches(algo, &word, &target) {
                Some(word)
            } else {
                None
            }
        })
    };
    Ok(found)
}

pub fn hybrid_attack(
    target: &str,
    algo: HashAlgo,
    words: &[String],
    min_digits: u32,
    max_digits: u32,
    also_prefix: bool,
) -> Result<Option<String>> {
    if max_digits > 6 {
        anyhow::bail!("--max-digits must be <= 6 (got {})", max_digits);
    }
    if min_digits > max_digits {
        anyhow::bail!("--min-digits must be <= --max-digits");
    }

    let mut total: u128 = 0;
    for d in min_digits..=max_digits {
        let count = 10u128.pow(d);
        let variants = if also_prefix { 2u128 } else { 1u128 };
        // include bare word once when d range covers 0
        total = total
            .checked_add(
                (words.len() as u128)
                    .checked_mul(count)
                    .and_then(|v| v.checked_mul(variants))
                    .context("Hybrid search space overflowed")?,
            )
            .context("Hybrid search space overflowed")?;
        if total > MAX_BRUTE_SPACE {
            anyhow::bail!(
                "Hybrid search space exceeds the limit of {} candidates",
                MAX_BRUTE_SPACE
            );
        }
    }

    // Decode after digit-range and space checks so invalid hex cannot hide them.
    let Some(target) = decode_target_digest(target) else {
        return Ok(None);
    };

    let found = words.par_iter().find_map_any(|word| {
        let mut buf = String::with_capacity(word.len() + max_digits as usize);
        for digits in min_digits..=max_digits {
            let max_n = 10u32.pow(digits);
            for n in 0..max_n {
                buf.clear();
                buf.push_str(word);
                append_zero_padded(&mut buf, n, digits);
                if digest_matches(algo, &buf, &target) {
                    return Some(buf);
                }
                if also_prefix && digits > 0 {
                    buf.clear();
                    append_zero_padded(&mut buf, n, digits);
                    buf.push_str(word);
                    if digest_matches(algo, &buf, &target) {
                        return Some(buf);
                    }
                }
            }
        }
        None
    });
    Ok(found)
}

/// Append `n` as a `width`-digit decimal with leading zeros (`width == 0` is a
/// no-op).
fn append_zero_padded(buf: &mut String, mut n: u32, width: u32) {
    if width == 0 {
        return;
    }
    // `hybrid_attack` rejects width > 6, so 6 bytes always suffice.
    let width = width as usize;
    let mut tmp = [b'0'; 6];
    for i in (0..width).rev() {
        tmp[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    for &b in &tmp[..width] {
        buf.push(b as char);
    }
}

pub fn builtin_rules() -> Vec<&'static str> {
    BUILTIN_RULES.to_vec()
}

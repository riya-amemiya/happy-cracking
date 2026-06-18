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

pub fn compute_hash(algo: HashAlgo, input: &str) -> String {
    match algo {
        HashAlgo::Md5 => {
            let mut hasher = Md5::new();
            hasher.update(input.as_bytes());
            format!("{:x}", hasher.finalize())
        }
        HashAlgo::Sha1 => {
            let mut hasher = Sha1::new();
            hasher.update(input.as_bytes());
            format!("{:x}", hasher.finalize())
        }
        HashAlgo::Sha256 => {
            let mut hasher = Sha256::new();
            hasher.update(input.as_bytes());
            format!("{:x}", hasher.finalize())
        }
        HashAlgo::Sha512 => {
            let mut hasher = Sha512::new();
            hasher.update(input.as_bytes());
            format!("{:x}", hasher.finalize())
        }
        HashAlgo::Md4 => {
            let mut hasher = Md4::new();
            hasher.update(input.as_bytes());
            format!("{:x}", hasher.finalize())
        }
        HashAlgo::Ntlm => {
            let mut hasher = Md4::new();
            for unit in input.encode_utf16() {
                hasher.update(unit.to_le_bytes());
            }
            format!("{:x}", hasher.finalize())
        }
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
    let target = normalize_hash(target);
    candidates.par_iter().find_map_any(|word| {
        let salted = apply_salt(word, salt, pos);
        if compute_hash(algo, &salted) == target {
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
    let target = normalize_hash(target);
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

    for len in min_len..=max_len {
        let count = base.pow(len as u32);
        if let Some(found) = (0..count).into_par_iter().find_map_any(|index| {
            let word = index_to_word(index, &chars, len);
            let salted = apply_salt(&word, salt, pos);
            if compute_hash(algo, &salted) == target {
                Some(word)
            } else {
                None
            }
        }) {
            return Ok(Some(found));
        }
    }
    Ok(None)
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

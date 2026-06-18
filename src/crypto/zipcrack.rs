use anyhow::{Context, Result};
use clap::Subcommand;
use rayon::prelude::*;
use std::io::{Cursor, Read};
use std::path::PathBuf;

const MAX_BRUTE_SPACE: u128 = 1_000_000_000;

#[derive(Subcommand)]
pub enum ZipcrackAction {
    #[command(about = "Dictionary attack against a password-protected zip")]
    Dict {
        #[arg(short, long, help = "Path to the encrypted zip file")]
        file: PathBuf,
        #[arg(short, long, help = "Path to the wordlist (one password per line)")]
        wordlist: PathBuf,
    },
    #[command(about = "Brute-force attack against a password-protected zip")]
    Brute {
        #[arg(short, long, help = "Path to the encrypted zip file")]
        file: PathBuf,
        #[arg(
            short,
            long,
            help = "Characters to try in each position",
            default_value = "abcdefghijklmnopqrstuvwxyz0123456789"
        )]
        charset: String,
        #[arg(long, help = "Minimum password length", default_value = "1")]
        min_len: usize,
        #[arg(long, help = "Maximum password length", default_value = "4")]
        max_len: usize,
    },
    #[command(about = "List entries in a zip (name, size, encryption)")]
    Info {
        #[arg(short, long, help = "Path to the zip file")]
        file: PathBuf,
    },
}

pub fn run(action: ZipcrackAction) -> Result<()> {
    match action {
        ZipcrackAction::Dict { file, wordlist } => {
            let bytes = std::fs::read(&file)
                .with_context(|| format!("Failed to read zip file: {}", file.display()))?;
            let list = std::fs::read_to_string(&wordlist)
                .with_context(|| format!("Failed to read wordlist: {}", wordlist.display()))?;
            let words: Vec<&str> = list
                .lines()
                .map(str::trim)
                .filter(|w| !w.is_empty())
                .collect();

            match dict_attack(&bytes, &words) {
                Some(password) => println!("Found password: {}", password),
                None => println!("Not found"),
            }
        }
        ZipcrackAction::Brute {
            file,
            charset,
            min_len,
            max_len,
        } => {
            let bytes = std::fs::read(&file)
                .with_context(|| format!("Failed to read zip file: {}", file.display()))?;

            match brute_attack(&bytes, &charset, min_len, max_len)? {
                Some(password) => println!("Found password: {}", password),
                None => println!("Not found"),
            }
        }
        ZipcrackAction::Info { file } => {
            let bytes = std::fs::read(&file)
                .with_context(|| format!("Failed to read zip file: {}", file.display()))?;

            for entry in list_entries(&bytes)? {
                println!(
                    "{}  size={}  encrypted={}  method={}",
                    entry.name, entry.size, entry.encrypted, entry.method
                );
            }
        }
    }
    Ok(())
}

pub struct EntryInfo {
    pub name: String,
    pub size: u64,
    pub encrypted: bool,
    pub method: String,
}

pub fn verify_password(zip_bytes: &[u8], password: &str) -> bool {
    let mut archive = match zip::ZipArchive::new(Cursor::new(zip_bytes)) {
        Ok(a) => a,
        Err(_) => return false,
    };

    let mut saw_encrypted = false;
    for i in 0..archive.len() {
        let encrypted = match archive.by_index_raw(i) {
            Ok(entry) => entry.encrypted(),
            Err(_) => return false,
        };
        if !encrypted {
            continue;
        }
        saw_encrypted = true;

        let mut entry = match archive.by_index_decrypt(i, password.as_bytes()) {
            Ok(entry) => entry,

            Err(_) => return false,
        };

        let mut sink = Vec::new();
        if entry.read_to_end(&mut sink).is_err() {
            return false;
        }
    }

    saw_encrypted
}

pub fn dict_attack(zip_bytes: &[u8], words: &[&str]) -> Option<String> {
    words
        .par_iter()
        .find_any(|word| verify_password(zip_bytes, word))
        .map(|word| word.to_string())
}

pub fn brute_attack(
    zip_bytes: &[u8],
    charset: &str,
    min_len: usize,
    max_len: usize,
) -> Result<Option<String>> {
    let chars: Vec<char> = charset.chars().collect();
    if chars.is_empty() {
        anyhow::bail!("Charset must not be empty");
    }
    if min_len == 0 {
        anyhow::bail!("min-len must be at least 1");
    }
    if max_len < min_len {
        anyhow::bail!("max-len ({}) must be >= min-len ({})", max_len, min_len);
    }

    let base = chars.len() as u128;
    let mut total: u128 = 0;
    for len in min_len..=max_len {
        let count = base.checked_pow(len as u32).unwrap_or(u128::MAX);
        total = total.saturating_add(count);
        if total > MAX_BRUTE_SPACE {
            anyhow::bail!(
                "Brute-force keyspace ({}+) exceeds the limit of {}",
                total,
                MAX_BRUTE_SPACE
            );
        }
    }

    for len in min_len..=max_len {
        let count = base.pow(len as u32);
        let found = (0..count).into_par_iter().find_map_any(|index| {
            let candidate = index_to_candidate(index, &chars, len);
            if verify_password(zip_bytes, &candidate) {
                Some(candidate)
            } else {
                None
            }
        });
        if found.is_some() {
            return Ok(found);
        }
    }

    Ok(None)
}

fn index_to_candidate(index: u128, chars: &[char], len: usize) -> String {
    let base = chars.len() as u128;
    let mut value = index;
    let mut out = vec![chars[0]; len];
    for slot in out.iter_mut().rev() {
        *slot = chars[(value % base) as usize];
        value /= base;
    }
    out.into_iter().collect()
}

pub fn list_entries(zip_bytes: &[u8]) -> Result<Vec<EntryInfo>> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(zip_bytes)).context("Failed to open zip archive")?;

    let mut entries = Vec::with_capacity(archive.len());
    for i in 0..archive.len() {
        let entry = archive
            .by_index_raw(i)
            .with_context(|| format!("Failed to read entry at index {}", i))?;
        entries.push(EntryInfo {
            name: entry.name().to_string(),
            size: entry.size(),
            encrypted: entry.encrypted(),
            method: format!("{:?}", entry.compression()),
        });
    }
    Ok(entries)
}

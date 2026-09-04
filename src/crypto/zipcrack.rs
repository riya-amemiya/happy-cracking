use anyhow::{Context, Result};
use clap::Subcommand;
use rayon::prelude::*;
use std::io::{self, Cursor, Read};
use std::path::PathBuf;

const MAX_BRUTE_SPACE: u128 = 1_000_000_000;

pub const MAX_BRUTE_LEN: usize = 32;

/// Maximum uncompressed bytes consumed while checking one zip member's password.
///
/// SECURITY: Fully buffering decompressed members (`read_to_end`) lets a zip bomb
/// advertise a tiny compressed size and expand to gigabytes of RAM. Streaming into
/// `io::sink` with this cap keeps verification memory-bounded.
const MAX_VERIFY_UNCOMPRESSED: u64 = 16 * 1024 * 1024;

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
                Some(password) => println!("Found password: {password}"),
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
                Some(password) => println!("Found password: {password}"),
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

#[must_use]
pub fn verify_password(zip_bytes: &[u8], password: &str) -> bool {
    verify_password_with_limit(zip_bytes, password, MAX_VERIFY_UNCOMPRESSED)
}

fn verify_password_with_limit(zip_bytes: &[u8], password: &str, max_uncompressed: u64) -> bool {
    let Ok(mut archive) = zip::ZipArchive::new(Cursor::new(zip_bytes)) else {
        return false;
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

        let Ok(entry) = archive.by_index_decrypt(i, password.as_bytes()) else {
            return false;
        };

        // Discard decompressed bytes; stop before a zip bomb can exhaust memory.
        // Reading one extra byte past the cap distinguishes "fully consumed"
        // (CRC checked on EOF) from "hit the limit mid-stream" (reject).
        let mut limited = entry.take(max_uncompressed.saturating_add(1));
        match io::copy(&mut limited, &mut io::sink()) {
            Ok(n) if n <= max_uncompressed => {}
            _ => return false,
        }
    }

    saw_encrypted
}

#[must_use]
pub fn dict_attack(zip_bytes: &[u8], words: &[&str]) -> Option<String> {
    words
        .par_iter()
        .find_any(|word| verify_password(zip_bytes, word))
        .map(std::string::ToString::to_string)
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
        anyhow::bail!("max-len ({max_len}) must be >= min-len ({min_len})");
    }
    if max_len > MAX_BRUTE_LEN {
        anyhow::bail!("Maximum password length {max_len} exceeds the limit of {MAX_BRUTE_LEN}");
    }

    let base = chars.len() as u128;
    let mut total: u128 = 0;
    for len in min_len..=max_len {
        let count = base.checked_pow(len as u32).unwrap_or(u128::MAX);
        total = total.saturating_add(count);
        if total > MAX_BRUTE_SPACE {
            anyhow::bail!("Brute-force keyspace ({total}+) exceeds the limit of {MAX_BRUTE_SPACE}");
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
            .with_context(|| format!("Failed to read entry at index {i}"))?;
        entries.push(EntryInfo {
            name: entry.name().to_string(),
            size: entry.size(),
            encrypted: entry.encrypted(),
            method: format!("{:?}", entry.compression()),
        });
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::AesMode;
    use zip::write::{FileOptions, ZipWriter};

    fn make_encrypted_zip(password: &str, content: &[u8]) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut writer = ZipWriter::new(&mut buf);
            let options: FileOptions<'_, ()> = FileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .with_aes_encryption(AesMode::Aes256, password);
            writer.start_file("flag.txt", options).unwrap();
            writer.write_all(content).unwrap();
            writer.finish().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn verify_accepts_correct_password_via_streaming_sink() {
        let bytes = make_encrypted_zip("hunter2", b"flag{zip_cracked}");
        assert!(verify_password(&bytes, "hunter2"));
        assert!(!verify_password(&bytes, "wrongpass"));
    }

    #[test]
    fn verify_rejects_when_decompressed_member_exceeds_byte_cap() {
        let content = b"flag{zip_cracked}";
        let bytes = make_encrypted_zip("hunter2", content);
        assert!(
            verify_password_with_limit(&bytes, "hunter2", content.len() as u64),
            "cap equal to uncompressed size must still accept"
        );
        assert!(
            !verify_password_with_limit(&bytes, "hunter2", 4),
            "cap below uncompressed size must reject to bound zip-bomb expansion"
        );
    }
}

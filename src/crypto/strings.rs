use anyhow::{Context, Result};
use clap::{Subcommand, ValueEnum};
use std::io::Read;
use std::path::{Path, PathBuf};

pub const MAX_STRINGS_BYTES: usize = 16 * 1024 * 1024;

pub const MAX_EXTRACTED_STRINGS: usize = 250_000;

#[derive(Clone, Copy, ValueEnum)]
pub enum StringEncoding {
    Ascii,
    Utf16le,
    Both,
}

#[derive(Subcommand)]
pub enum StringsAction {
    #[command(about = "Extract printable strings from binary data")]
    Extract {
        #[arg(help = "Input as hex string (or use --file)")]
        input: Option<String>,
        #[arg(long, help = "Read input from a file path")]
        file: Option<PathBuf>,
        #[arg(long, default_value = "4", help = "Minimum string length")]
        min_len: usize,
        #[arg(long, default_value = "ascii", help = "Encoding to scan for")]
        encoding: StringEncoding,
    },
}

pub fn run(action: StringsAction) -> Result<()> {
    match action {
        StringsAction::Extract {
            input,
            file,
            min_len,
            encoding,
        } => match (input, file) {
            (Some(_), Some(_)) => {
                anyhow::bail!("Provide exactly one of <input> or --file, not both")
            }
            (None, None) => {
                anyhow::bail!("Provide exactly one of <input> or --file")
            }
            (Some(hex_str), None) => match encoding {
                StringEncoding::Ascii => {
                    for s in extract_ascii_from_hex(&hex_str, min_len)? {
                        println!("{s}");
                    }
                }
                StringEncoding::Utf16le => {
                    for s in extract_utf16le_from_hex(&hex_str, min_len)? {
                        println!("{s}");
                    }
                }
                StringEncoding::Both => {
                    let data = decode_strings_hex_with_limit(&hex_str, MAX_STRINGS_BYTES)?;
                    print_extracted(&data, min_len, encoding)?;
                }
            },
            (None, Some(path)) => {
                let data = read_strings_bytes_with_limit(&path, MAX_STRINGS_BYTES)?;
                print_extracted(&data, min_len, encoding)?;
            }
        },
    }
    Ok(())
}

fn print_extracted(data: &[u8], min_len: usize, encoding: StringEncoding) -> Result<()> {
    match encoding {
        StringEncoding::Ascii => {
            for s in extract_ascii(data, min_len)? {
                println!("{s}");
            }
        }
        StringEncoding::Utf16le => {
            for s in extract_utf16le(data, min_len)? {
                println!("{s}");
            }
        }
        StringEncoding::Both => {
            for s in extract_ascii(data, min_len)? {
                println!("{s}");
            }
            for s in extract_utf16le(data, min_len)? {
                println!("{s}");
            }
        }
    }
    Ok(())
}

pub fn read_strings_bytes_with_limit(path: &Path, max_bytes: usize) -> Result<Vec<u8>> {
    let max_bytes = max_bytes.min(MAX_STRINGS_BYTES);
    let file = std::fs::File::open(path)
        .with_context(|| format!("Failed to open file: {}", path.display()))?;
    let mut buf = Vec::new();
    file.take((max_bytes as u64).saturating_add(1))
        .read_to_end(&mut buf)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;
    if buf.len() > max_bytes {
        anyhow::bail!(
            "Input exceeds maximum size of {max_bytes} bytes to prevent Denial of Service"
        );
    }
    Ok(buf)
}

pub fn check_extracted_count(count: usize, max_count: usize) -> Result<()> {
    let max_count = max_count.min(MAX_EXTRACTED_STRINGS);
    if count > max_count {
        anyhow::bail!(
            "Extracted string count exceeds maximum of {max_count} to prevent Denial of Service"
        );
    }
    Ok(())
}

fn push_extracted(results: &mut Vec<String>, s: String, max_count: usize) -> Result<()> {
    check_extracted_count(results.len().saturating_add(1), max_count)?;
    results.push(s);
    Ok(())
}

fn bounded_strings_hex(hex_str: &str, max_bytes: usize) -> Result<&str> {
    let hex_str = hex_str.trim();
    let max_bytes = max_bytes.min(MAX_STRINGS_BYTES);
    let max_chars = max_bytes.saturating_mul(2);
    if hex_str.len() > max_chars {
        anyhow::bail!(
            "Input exceeds maximum size of {max_bytes} bytes to prevent Denial of Service"
        );
    }
    Ok(hex_str)
}

pub fn decode_strings_hex_with_limit(hex_str: &str, max_bytes: usize) -> Result<Vec<u8>> {
    let hex_str = bounded_strings_hex(hex_str, max_bytes)?;
    hex::decode(hex_str).context("Failed to decode input as hex")
}

pub fn extract_ascii_from_hex(hex_str: &str, min_len: usize) -> Result<Vec<String>> {
    extract_ascii_from_hex_with_limit(hex_str, min_len, MAX_STRINGS_BYTES)
}

pub fn extract_ascii_from_hex_with_limit(
    hex_str: &str,
    min_len: usize,
    max_bytes: usize,
) -> Result<Vec<String>> {
    let data = decode_strings_hex_with_limit(hex_str, max_bytes)?;
    extract_ascii(&data, min_len)
}

pub fn extract_utf16le_from_hex(hex_str: &str, min_len: usize) -> Result<Vec<String>> {
    extract_utf16le_from_hex_with_limit(hex_str, min_len, MAX_STRINGS_BYTES)
}

pub fn extract_utf16le_from_hex_with_limit(
    hex_str: &str,
    min_len: usize,
    max_bytes: usize,
) -> Result<Vec<String>> {
    let data = decode_strings_hex_with_limit(hex_str, max_bytes)?;
    extract_utf16le(&data, min_len)
}

pub fn extract_ascii(data: &[u8], min_len: usize) -> Result<Vec<String>> {
    extract_ascii_with_count_limit(data, min_len, MAX_EXTRACTED_STRINGS)
}

pub fn extract_ascii_with_count_limit(
    data: &[u8],
    min_len: usize,
    max_count: usize,
) -> Result<Vec<String>> {
    if min_len == 0 {
        anyhow::bail!("min_len must be at least 1");
    }

    let mut results = Vec::new();
    let mut current = Vec::new();

    for &b in data {
        if (0x20..=0x7E).contains(&b) {
            current.push(b);
        } else if !current.is_empty() {
            if current.len() >= min_len {
                push_extracted(
                    &mut results,
                    String::from_utf8_lossy(&current).into_owned(),
                    max_count,
                )?;
            }
            current.clear();
        }
    }
    if current.len() >= min_len {
        push_extracted(
            &mut results,
            String::from_utf8_lossy(&current).into_owned(),
            max_count,
        )?;
    }

    Ok(results)
}

pub fn extract_utf16le(data: &[u8], min_len: usize) -> Result<Vec<String>> {
    extract_utf16le_with_count_limit(data, min_len, MAX_EXTRACTED_STRINGS)
}

pub fn extract_utf16le_with_count_limit(
    data: &[u8],
    min_len: usize,
    max_count: usize,
) -> Result<Vec<String>> {
    if min_len == 0 {
        anyhow::bail!("min_len must be at least 1");
    }

    let mut results = Vec::new();
    let mut current = Vec::new();
    let mut i = 0;

    while i + 1 < data.len() {
        let lo = data[i];
        let hi = data[i + 1];
        if (0x20..=0x7E).contains(&lo) && hi == 0x00 {
            current.push(lo);
            i += 2;
        } else {
            if current.len() >= min_len {
                push_extracted(
                    &mut results,
                    String::from_utf8_lossy(&current).into_owned(),
                    max_count,
                )?;
            }
            current.clear();
            i += 1;
        }
    }
    if current.len() >= min_len {
        push_extracted(
            &mut results,
            String::from_utf8_lossy(&current).into_owned(),
            max_count,
        )?;
    }

    Ok(results)
}

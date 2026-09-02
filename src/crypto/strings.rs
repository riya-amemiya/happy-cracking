use anyhow::{Context, Result};
use clap::{Subcommand, ValueEnum};
use std::path::PathBuf;

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
        } => {
            let data = match (input, file) {
                (Some(_), Some(_)) => {
                    anyhow::bail!("Provide exactly one of <input> or --file, not both")
                }
                (None, None) => {
                    anyhow::bail!("Provide exactly one of <input> or --file")
                }
                (Some(hex_str), None) => {
                    hex::decode(hex_str.trim()).context("Failed to decode input as hex")?
                }
                (None, Some(path)) => std::fs::read(&path)
                    .with_context(|| format!("Failed to read file: {}", path.display()))?,
            };

            match encoding {
                StringEncoding::Ascii => {
                    for s in extract_ascii(&data, min_len)? {
                        println!("{s}");
                    }
                }
                StringEncoding::Utf16le => {
                    for s in extract_utf16le(&data, min_len)? {
                        println!("{s}");
                    }
                }
                StringEncoding::Both => {
                    for s in extract_ascii(&data, min_len)? {
                        println!("{s}");
                    }
                    for s in extract_utf16le(&data, min_len)? {
                        println!("{s}");
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn extract_ascii(data: &[u8], min_len: usize) -> Result<Vec<String>> {
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
                results.push(String::from_utf8_lossy(&current).into_owned());
            }
            current.clear();
        }
    }
    if current.len() >= min_len {
        results.push(String::from_utf8_lossy(&current).into_owned());
    }

    Ok(results)
}

pub fn extract_utf16le(data: &[u8], min_len: usize) -> Result<Vec<String>> {
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
                results.push(String::from_utf8_lossy(&current).into_owned());
            }
            current.clear();
            i += 1;
        }
    }
    if current.len() >= min_len {
        results.push(String::from_utf8_lossy(&current).into_owned());
    }

    Ok(results)
}

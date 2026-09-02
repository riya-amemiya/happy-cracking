use anyhow::{Context, Result};
use clap::Subcommand;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const MAX_IDENTIFY_BYTES: usize = 512;

#[derive(Subcommand)]
pub enum FiletypeAction {
    #[command(about = "Identify file type from magic bytes")]
    Identify {
        #[arg(help = "Input as hex string (or use --file)")]
        input: Option<String>,
        #[arg(long, help = "Read input from a file path")]
        file: Option<PathBuf>,
    },
}

pub struct Signature {
    pub name: &'static str,
    pub description: &'static str,
    pub offset: usize,
}

pub fn read_identify_prefix(path: &Path) -> Result<Vec<u8>> {
    let mut file = std::fs::File::open(path)
        .with_context(|| format!("Failed to open file: {}", path.display()))?;
    let mut buf = vec![0u8; MAX_IDENTIFY_BYTES];
    let n = file
        .read(&mut buf)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;
    buf.truncate(n);
    Ok(buf)
}

pub fn decode_identify_hex(hex_str: &str) -> Result<Vec<u8>> {
    let hex_str = hex_str.trim();
    let max_chars = MAX_IDENTIFY_BYTES.saturating_mul(2);
    let prefix = if hex_str.len() > max_chars {
        &hex_str[..max_chars]
    } else {
        hex_str
    };
    hex::decode(prefix).context("Failed to decode input as hex")
}

pub fn run(action: FiletypeAction) -> Result<()> {
    match action {
        FiletypeAction::Identify { input, file } => {
            let data = match (input, file) {
                (Some(_), Some(_)) => {
                    anyhow::bail!("Provide exactly one of <input> or --file, not both")
                }
                (None, None) => {
                    anyhow::bail!("Provide exactly one of <input> or --file")
                }
                (Some(hex_str), None) => decode_identify_hex(&hex_str)?,
                (None, Some(path)) => read_identify_prefix(&path)?,
            };

            let matches = identify(&data);
            if matches.is_empty() {
                println!("No known file type signature matched");
            } else {
                for sig in matches {
                    println!("{} - {} (offset {})", sig.name, sig.description, sig.offset);
                }
            }
        }
    }
    Ok(())
}

fn matches_at(data: &[u8], offset: usize, magic: &[u8]) -> bool {
    data.len() >= offset + magic.len() && &data[offset..offset + magic.len()] == magic
}

fn starts_with(data: &[u8], magic: &[u8]) -> bool {
    matches_at(data, 0, magic)
}

#[must_use]
pub fn identify(data: &[u8]) -> Vec<Signature> {
    let mut out = Vec::new();

    let push = |out: &mut Vec<Signature>, name, description, offset| {
        out.push(Signature {
            name,
            description,
            offset,
        });
    };

    if starts_with(data, &[0x89, 0x50, 0x4E, 0x47]) {
        push(&mut out, "PNG", "Portable Network Graphics image", 0);
    }
    if starts_with(data, &[0xFF, 0xD8, 0xFF]) {
        push(&mut out, "JPEG", "JPEG image", 0);
    }
    if starts_with(data, b"GIF87a") {
        push(&mut out, "GIF", "GIF image (87a)", 0);
    }
    if starts_with(data, b"GIF89a") {
        push(&mut out, "GIF", "GIF image (89a)", 0);
    }
    if starts_with(data, b"BM") {
        push(&mut out, "BMP", "Bitmap image", 0);
    }
    if starts_with(data, &[0x00, 0x00, 0x01, 0x00]) {
        push(&mut out, "ICO", "Windows icon", 0);
    }

    if starts_with(data, b"%PDF") {
        push(&mut out, "PDF", "Portable Document Format", 0);
    }

    if starts_with(data, &[0x50, 0x4B, 0x03, 0x04]) {
        push(&mut out, "ZIP", "ZIP archive (also JAR/APK/DOCX/etc.)", 0);
    }
    if starts_with(data, &[0x50, 0x4B, 0x05, 0x06]) {
        push(&mut out, "ZIP", "ZIP archive (empty)", 0);
    }
    if starts_with(data, &[0x50, 0x4B, 0x07, 0x08]) {
        push(&mut out, "ZIP", "ZIP archive (spanned)", 0);
    }
    if starts_with(data, &[0x1F, 0x8B]) {
        push(&mut out, "GZIP", "gzip compressed data", 0);
    }
    if starts_with(data, b"BZh") {
        push(&mut out, "BZIP2", "bzip2 compressed data", 0);
    }
    if starts_with(data, &[0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00]) {
        push(&mut out, "XZ", "XZ compressed data", 0);
    }
    if starts_with(data, &[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]) {
        push(&mut out, "7Z", "7-Zip archive", 0);
    }
    if starts_with(data, &[0x52, 0x61, 0x72, 0x21, 0x1A, 0x07, 0x01, 0x00]) {
        push(&mut out, "RAR", "RAR archive (v5)", 0);
    } else if starts_with(data, &[0x52, 0x61, 0x72, 0x21, 0x1A, 0x07, 0x00]) {
        push(&mut out, "RAR", "RAR archive (v4)", 0);
    }

    if starts_with(data, &[0x7F, 0x45, 0x4C, 0x46]) {
        push(&mut out, "ELF", "ELF executable", 0);
    }
    if starts_with(data, b"MZ") {
        push(&mut out, "PE", "DOS/PE executable (MZ)", 0);
    }
    if starts_with(data, &[0xCA, 0xFE, 0xBA, 0xBE]) {
        push(&mut out, "Java class", "Java class file", 0);
        push(&mut out, "Mach-O", "Mach-O universal (fat) binary", 0);
    }
    if starts_with(data, &[0xFE, 0xED, 0xFA, 0xCE]) {
        push(&mut out, "Mach-O", "Mach-O executable (32-bit)", 0);
    }
    if starts_with(data, &[0xFE, 0xED, 0xFA, 0xCF]) {
        push(&mut out, "Mach-O", "Mach-O executable (64-bit)", 0);
    }

    if starts_with(data, &[0x49, 0x44, 0x33]) {
        push(&mut out, "MP3", "MP3 audio (ID3 tag)", 0);
    }
    if starts_with(data, &[0xFF, 0xFB]) {
        push(&mut out, "MP3", "MP3 audio (MPEG frame)", 0);
    }
    if starts_with(data, b"OggS") {
        push(&mut out, "OGG", "Ogg container", 0);
    }
    if starts_with(data, b"fLaC") {
        push(&mut out, "FLAC", "FLAC audio", 0);
    }
    if starts_with(data, b"RIFF") {
        if matches_at(data, 8, b"WAVE") {
            push(&mut out, "WAV", "WAVE audio (RIFF)", 0);
        } else if matches_at(data, 8, b"WEBP") {
            push(&mut out, "WEBP", "WebP image (RIFF)", 0);
        } else if matches_at(data, 8, b"AVI ") {
            push(&mut out, "AVI", "AVI video (RIFF)", 0);
        } else {
            push(&mut out, "RIFF", "RIFF container (unknown form)", 0);
        }
    }

    if starts_with(data, b"SQLite format 3\0") {
        push(&mut out, "SQLite", "SQLite 3 database", 0);
    }

    if matches_at(data, 257, b"ustar") {
        push(&mut out, "TAR", "tar archive (ustar)", 257);
    }

    out
}

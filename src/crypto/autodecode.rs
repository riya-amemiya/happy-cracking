use anyhow::Result;
use clap::Subcommand;

use super::{base32, base58, base64, binary, hex, morse, url};

#[derive(Subcommand)]
pub enum AutoDecodeAction {
    #[command(about = "Automatically detect and decode")]
    Decode {
        #[arg(help = "Input to decode")]
        input: String,
        #[arg(short, long, help = "Recursively decode", default_value = "false")]
        recursive: bool,
        #[arg(short, long, help = "Maximum recursion depth", default_value = "5")]
        max_depth: usize,
    },
}

pub fn run(action: AutoDecodeAction) -> Result<()> {
    match action {
        AutoDecodeAction::Decode {
            input,
            recursive,
            max_depth,
        } => {
            if recursive {
                decode_recursive(&input, 0, max_depth);
            } else {
                let results = detect_and_decode(&input);
                if results.is_empty() {
                    println!("No encoding detected");
                } else {
                    for (encoding, decoded) in results {
                        println!("[{}] {}", encoding, decoded);
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn detect_and_decode(input: &str) -> Vec<(&'static str, String)> {
    let input = input.trim();
    let mut results = Vec::new();

    // Try Base64
    if looks_like_base64(input)
        && let Ok(decoded) = base64::decode(input)
        && is_printable(&decoded)
    {
        results.push(("Base64", decoded));
    }

    // Try Base32
    if looks_like_base32(input)
        && let Ok(decoded) = base32::decode(input)
        && is_printable(&decoded)
    {
        results.push(("Base32", decoded));
    }

    // Try Base58
    if looks_like_base58(input)
        && let Ok(decoded) = base58::decode(input)
        && is_printable(&decoded)
    {
        results.push(("Base58", decoded));
    }

    // Try Hex
    if looks_like_hex(input)
        && let Ok(decoded) = hex::decode(input)
        && is_printable(&decoded)
    {
        results.push(("Hex", decoded));
    }

    // Try URL encoding
    if input.contains('%')
        && let Ok(decoded) = url::decode(input)
        && decoded != input
    {
        results.push(("URL", decoded));
    }

    // Try Binary
    if looks_like_binary(input)
        && let Ok(decoded) = binary::decode(input)
        && is_printable(&decoded)
    {
        results.push(("Binary", decoded));
    }

    // Try Morse
    if looks_like_morse(input)
        && let Ok(decoded) = morse::decode(input)
    {
        results.push(("Morse", decoded));
    }

    results
}

fn decode_recursive(input: &str, depth: usize, max_depth: usize) {
    let indent = "  ".repeat(depth);

    if depth >= max_depth {
        println!("{}[Max depth reached]", indent);
        return;
    }

    let results = detect_and_decode(input);

    if results.is_empty() {
        if depth == 0 {
            println!("No encoding detected");
        }
        return;
    }

    for (encoding, decoded) in results {
        println!("{}[{}] {}", indent, encoding, decoded);

        // Try to decode further
        if decoded != input && !decoded.is_empty() {
            decode_recursive(&decoded, depth + 1, max_depth);
        }
    }
}

fn is_printable(s: &str) -> bool {
    s.chars()
        .all(|c| !c.is_control() || c == '\n' || c == '\t' || c == '\r')
}

fn looks_like_base64(s: &str) -> bool {
    if s.len() < 2 {
        return false;
    }
    let valid_chars = s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=');
    valid_chars && (s.len().is_multiple_of(4) || s.ends_with('='))
}

fn looks_like_base32(s: &str) -> bool {
    if s.len() < 2 {
        return false;
    }
    let valid_chars = s
        .to_uppercase()
        .chars()
        .all(|c| matches!(c, 'A'..='Z' | '2'..='7' | '='));
    valid_chars && s.len().is_multiple_of(8)
}

fn looks_like_base58(s: &str) -> bool {
    if s.len() < 2 {
        return false;
    }
    // Base58 excludes 0, O, I, l
    s.chars()
        .all(|c| c.is_ascii_alphanumeric() && !matches!(c, '0' | 'O' | 'I' | 'l'))
}

fn looks_like_hex(s: &str) -> bool {
    s.len() >= 2 && s.len().is_multiple_of(2) && s.chars().all(|c| c.is_ascii_hexdigit())
}

fn looks_like_binary(s: &str) -> bool {
    let cleaned: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    cleaned.len() >= 8 && cleaned.chars().all(|c| c == '0' || c == '1')
}

fn looks_like_morse(s: &str) -> bool {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() < 2 {
        return false;
    }
    chars
        .iter()
        .all(|&c| c == '.' || c == '-' || c == ' ' || c == '/')
}

use anyhow::Result;
use clap::Subcommand;
use std::collections::{HashSet, VecDeque};

use super::{base32, base58, base64, binary, hex, morse, url};

const MAX_TREE_NODES: usize = 256;
const MAX_OUTPUT_CHARS: usize = 1_000_000;

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
        #[arg(
            long,
            help = "Aggressive BFS decode tree with scoring and flag detection"
        )]
        aggressive: bool,
        #[arg(
            long,
            default_value_t = 32,
            help = "Max nodes to expand in aggressive mode"
        )]
        max_nodes: usize,
    },
}

pub fn run(action: AutoDecodeAction) -> Result<()> {
    match action {
        AutoDecodeAction::Decode {
            input,
            recursive,
            max_depth,
            aggressive,
            max_nodes,
        } => {
            if aggressive {
                let results = decode_tree(&input, max_depth, max_nodes);
                if results.is_empty() {
                    println!("No encoding path found");
                } else {
                    for (path, decoded) in results {
                        let flag = if looks_like_flag(&decoded) {
                            " [FLAG?]"
                        } else {
                            ""
                        };
                        println!("[{}]{} {}", path, flag, decoded);
                    }
                }
            } else if recursive {
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

fn looks_like_flag(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    lower.contains("flag{")
        || lower.contains("ctf{")
        || lower.contains("picoctf{")
        || (s.contains('{') && s.contains('}') && s.len() >= 8 && s.len() <= 200)
}

/// Score a candidate decode result. Higher is better.
pub fn score_decode_candidate(s: &str) -> f64 {
    if s.is_empty() || s.len() > MAX_OUTPUT_CHARS {
        return f64::NEG_INFINITY;
    }
    let mut score = 0.0f64;
    if looks_like_flag(s) {
        score += 500.0;
    }
    let printable = s
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t' || *c == '\r')
        .count();
    score += (printable as f64 / s.len() as f64) * 40.0;
    // Prefer shorter expansions (decoding usually shrinks encoded text ratio-wise, but not always)
    if s.is_ascii() {
        score += 5.0;
    }
    // Mild preference for spaces (English)
    let spaces = s.chars().filter(|c| *c == ' ').count() as f64;
    score += (spaces / s.len() as f64 * 30.0).min(10.0);
    score
}

/// Breadth-first multi-path decode tree for aggressive mode.
/// Returns (path, decoded) pairs sorted by score descending.
pub fn decode_tree(input: &str, max_depth: usize, max_nodes: usize) -> Vec<(String, String)> {
    let max_depth = max_depth.min(12);
    let max_nodes = max_nodes.clamp(1, MAX_TREE_NODES);

    let mut queue: VecDeque<(String, String, usize)> = VecDeque::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut leaves: Vec<(String, String, f64)> = Vec::new();

    let root = input.trim().to_string();
    if root.is_empty() {
        return Vec::new();
    }
    queue.push_back((String::new(), root.clone(), 0));
    seen.insert(root);

    let mut expanded = 0usize;
    while let Some((path, current, depth)) = queue.pop_front() {
        if expanded >= max_nodes {
            break;
        }
        expanded += 1;

        let results = detect_and_decode(&current);
        if results.is_empty() || depth >= max_depth {
            let score = score_decode_candidate(&current);
            if score.is_finite() {
                let label = if path.is_empty() {
                    "identity".to_string()
                } else {
                    path.clone()
                };
                leaves.push((label, current, score));
            }
            continue;
        }

        let mut progressed = false;
        for (encoding, decoded) in results {
            if decoded == current || decoded.is_empty() || decoded.len() > MAX_OUTPUT_CHARS {
                continue;
            }
            if !seen.insert(decoded.clone()) {
                continue;
            }
            progressed = true;
            let next_path = if path.is_empty() {
                encoding.to_string()
            } else {
                format!("{}->{}", path, encoding)
            };
            let score = score_decode_candidate(&decoded);
            leaves.push((next_path.clone(), decoded.clone(), score));
            if depth + 1 < max_depth {
                queue.push_back((next_path, decoded, depth + 1));
            }
        }

        if !progressed {
            let score = score_decode_candidate(&current);
            let label = if path.is_empty() {
                "identity".to_string()
            } else {
                path
            };
            leaves.push((label, current, score));
        }
    }

    leaves.sort_by(|a, b| {
        b.2.partial_cmp(&a.2)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });

    // Deduplicate by decoded text, keep best path
    let mut best: Vec<(String, String)> = Vec::new();
    let mut seen_text: HashSet<String> = HashSet::new();
    for (path, text, _) in leaves {
        if seen_text.insert(text.clone()) {
            best.push((path, text));
        }
    }
    best
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
        .chars()
        .all(|c| c.is_ascii_alphabetic() || matches!(c, '2'..='7' | '='));
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
    let non_ws_count = s.chars().filter(|c| !c.is_whitespace()).count();
    non_ws_count >= 8 && s.chars().all(|c| c == '0' || c == '1' || c.is_whitespace())
}

fn looks_like_morse(s: &str) -> bool {
    s.len() >= 2
        && s.chars()
            .all(|c| c == '.' || c == '-' || c == ' ' || c == '/')
}

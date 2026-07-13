use anyhow::{Context, Result};
use clap::Subcommand;

#[derive(Subcommand)]
pub enum XorAction {
    #[command(about = "XOR with a key")]
    Cipher {
        #[arg(help = "Input (hex string)")]
        input: String,
        #[arg(short, long, help = "Key (hex string or ASCII with --ascii flag)")]
        key: String,
        #[arg(long, help = "Treat key as ASCII string")]
        ascii: bool,
    },
    #[command(about = "Brute force single-byte XOR")]
    Bruteforce {
        #[arg(help = "Input (hex string)")]
        input: String,
        #[arg(long, help = "Only show printable ASCII results")]
        printable: bool,
    },
    #[command(about = "Detect likely XOR key length via normalized Hamming distance")]
    Keylength {
        #[arg(help = "Input (hex string)")]
        input: String,
        #[arg(long, help = "Maximum key length to test", default_value = "40")]
        max_len: usize,
        #[arg(long, help = "Number of top results to show", default_value = "5")]
        top: usize,
    },
    #[command(about = "Recover a repeating multi-byte XOR key via frequency analysis")]
    Crack {
        #[arg(help = "Input (hex string)")]
        input: String,
        #[arg(long, help = "Maximum key length to test", default_value = "40")]
        max_len: usize,
        #[arg(
            long,
            help = "Number of key-length candidates to try",
            default_value = "3"
        )]
        top: usize,
        #[arg(long, help = "Known key length (skips length detection)")]
        key_length: Option<usize>,
    },
    #[command(about = "Crib-drag a known plaintext fragment across XOR ciphertext")]
    Crib {
        #[arg(help = "Input (hex string)")]
        input: String,
        #[arg(short, long, help = "Known plaintext crib (ASCII)")]
        crib: String,
        #[arg(long, help = "Maximum hits to print", default_value = "20")]
        limit: usize,
    },
}

pub fn run(action: XorAction) -> Result<()> {
    match action {
        XorAction::Cipher { input, key, ascii } => run_cipher(&input, &key, ascii),
        XorAction::Bruteforce { input, printable } => run_bruteforce(&input, printable),
        XorAction::Keylength {
            input,
            max_len,
            top,
        } => run_keylength(&input, max_len, top),
        XorAction::Crack {
            input,
            max_len,
            top,
            key_length,
        } => run_crack(&input, max_len, top, key_length),
        XorAction::Crib { input, crib, limit } => run_crib(&input, &crib, limit),
    }
}

fn run_cipher(input: &str, key: &str, ascii_key: bool) -> Result<()> {
    let input_bytes = hex::decode(input.trim()).context("Failed to decode input hex")?;

    let key_bytes = if ascii_key {
        key.as_bytes().to_vec()
    } else {
        hex::decode(key.trim()).context("Failed to decode key hex")?
    };

    let result = xor_bytes(&input_bytes, &key_bytes);
    println!("Hex: {}", hex::encode(&result));

    if let Ok(s) = std::str::from_utf8(&result) {
        println!("ASCII: {}", s);
    }

    Ok(())
}

fn run_bruteforce(input: &str, printable_only: bool) -> Result<()> {
    let input_bytes = hex::decode(input.trim()).context("Failed to decode input hex")?;

    for (key, result) in single_byte_xor_bruteforce(&input_bytes) {
        if printable_only {
            if result.iter().all(|&b| b.is_ascii_graphic() || b == b' ')
                && let Ok(s) = std::str::from_utf8(&result)
            {
                println!("Key 0x{:02x}: {}", key, s);
            }
        } else if let Ok(s) = std::str::from_utf8(&result) {
            println!("Key 0x{:02x}: {}", key, s);
        }
    }

    Ok(())
}

fn run_keylength(input: &str, max_len: usize, top: usize) -> Result<()> {
    let input_bytes = hex::decode(input.trim()).context("Failed to decode input hex")?;

    let results = detect_key_length(&input_bytes, max_len);
    if results.is_empty() {
        println!("Input too short to detect key length");
        return Ok(());
    }

    println!(
        "Top {} likely key lengths (lower distance = more likely):",
        top
    );
    for (i, &(key_len, distance)) in results.iter().take(top).enumerate() {
        println!(
            "  {}. length={:2}  normalized distance={:.4}",
            i + 1,
            key_len,
            distance
        );
    }

    Ok(())
}

pub fn xor_bytes(data: &[u8], key: &[u8]) -> Vec<u8> {
    if key.is_empty() {
        return data.to_vec();
    }
    // Optimization: Process data in full chunks of key length to avoid
    // expensive modulo operations per byte (i % key_len) and enable
    // better compiler vectorization/unrolling.
    let mut out = Vec::with_capacity(data.len());
    let key_len = key.len();

    let chunks = data.chunks_exact(key_len);
    let remainder = chunks.remainder();

    for chunk in chunks {
        out.extend(chunk.iter().zip(key).map(|(b, k)| b ^ k));
    }

    if !remainder.is_empty() {
        out.extend(remainder.iter().zip(key).map(|(b, k)| b ^ k));
    }
    out
}

pub fn single_byte_xor_bruteforce(data: &[u8]) -> Vec<(u8, Vec<u8>)> {
    (0..=255)
        .map(|key| (key, xor_bytes(data, &[key])))
        .collect()
}

fn hamming_distance(a: &[u8], b: &[u8]) -> u32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x ^ y).count_ones())
        .sum()
}

// Detect likely XOR key length using normalized Hamming distance.
// Returns a sorted list of (key_length, normalized_distance) pairs,
// with the most likely key lengths first (lowest distance).
pub fn detect_key_length(data: &[u8], max_len: usize) -> Vec<(usize, f64)> {
    let max_key_len = max_len.min(data.len() / 2);
    if max_key_len < 2 {
        return Vec::new();
    }

    let mut results: Vec<(usize, f64)> = (2..=max_key_len)
        .filter_map(|key_len| {
            // Use as many blocks as possible for better accuracy
            let num_blocks = data.len() / key_len;
            if num_blocks < 2 {
                return None;
            }

            let num_pairs = num_blocks - 1;
            let total_distance: u32 = (0..num_pairs)
                .map(|i| {
                    let block_a = &data[i * key_len..(i + 1) * key_len];
                    let block_b = &data[(i + 1) * key_len..(i + 2) * key_len];
                    hamming_distance(block_a, block_b)
                })
                .sum();

            let normalized = total_distance as f64 / (num_pairs as f64 * key_len as f64);
            Some((key_len, normalized))
        })
        .collect();

    // SECURITY: Use unwrap_or(Equal) instead of unwrap() to prevent panic on NaN values.
    // partial_cmp returns None for NaN comparisons; treating them as equal avoids a DoS vector.
    results.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    results
}

fn run_crack(input: &str, max_len: usize, top: usize, key_length: Option<usize>) -> Result<()> {
    let input_bytes = hex::decode(input.trim()).context("Failed to decode input hex")?;
    if input_bytes.is_empty() {
        anyhow::bail!("Input is empty");
    }

    let candidates = crack_repeating_key(&input_bytes, max_len, top, key_length);
    if candidates.is_empty() {
        println!("Could not recover a plausible key");
        return Ok(());
    }

    for (i, cand) in candidates.iter().enumerate() {
        println!(
            "=== Candidate {} (score={:.2}, key_len={}) ===",
            i + 1,
            cand.score,
            cand.key.len()
        );
        println!("Key (hex):   {}", hex::encode(&cand.key));
        if let Ok(s) = std::str::from_utf8(&cand.key)
            && s.chars().all(|c| c.is_ascii_graphic())
        {
            println!("Key (ascii): {}", s);
        }
        if let Ok(s) = std::str::from_utf8(&cand.plaintext) {
            println!("Plaintext:   {}", s);
        } else {
            println!("Plaintext (hex): {}", hex::encode(&cand.plaintext));
        }
        println!();
    }
    Ok(())
}

fn run_crib(input: &str, crib: &str, limit: usize) -> Result<()> {
    let input_bytes = hex::decode(input.trim()).context("Failed to decode input hex")?;
    let crib_bytes = crib.as_bytes();
    if crib_bytes.is_empty() {
        anyhow::bail!("Crib must not be empty");
    }
    if crib_bytes.len() > input_bytes.len() {
        anyhow::bail!("Crib longer than ciphertext");
    }

    let hits = crib_drag(&input_bytes, crib_bytes);
    if hits.is_empty() {
        println!("No crib positions produced printable key fragments");
        return Ok(());
    }

    for hit in hits.iter().take(limit) {
        println!(
            "offset={:<4} key_frag(hex)={}  key_frag(ascii)={:?}  window={:?}",
            hit.offset,
            hex::encode(&hit.key_fragment),
            String::from_utf8_lossy(&hit.key_fragment),
            hit.plaintext_window
        );
    }
    if hits.len() > limit {
        println!("... {} more (raise --limit)", hits.len() - limit);
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct XorCrackCandidate {
    pub key: Vec<u8>,
    pub plaintext: Vec<u8>,
    pub score: f64,
}

#[derive(Debug, Clone)]
pub struct CribHit {
    pub offset: usize,
    pub key_fragment: Vec<u8>,
    pub plaintext_window: String,
}

/// English-ish scoring for byte sequences (higher is better).
pub fn english_score(data: &[u8]) -> f64 {
    if data.is_empty() {
        return f64::NEG_INFINITY;
    }
    let mut score = 0.0f64;
    let mut letters = 0usize;
    let mut spaces = 0usize;
    for &b in data {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' => {
                letters += 1;
                score += 1.0;
                // ETAOIN SHRDLU bonus
                match b.to_ascii_lowercase() {
                    b'e' | b't' | b'a' | b'o' | b'i' | b'n' => score += 0.5,
                    b's' | b'h' | b'r' | b'd' | b'l' | b'u' => score += 0.25,
                    _ => {}
                }
            }
            b' ' => {
                spaces += 1;
                score += 1.2;
            }
            b'0'..=b'9'
            | b'.'
            | b','
            | b'\''
            | b'!'
            | b'?'
            | b'-'
            | b'_'
            | b'{'
            | b'}'
            | b':'
            | b'/'
            | b'\\'
            | b'@'
            | b'#' => score += 0.3,
            0x21..=0x7e => score += 0.1,
            b'\n' | b'\r' | b'\t' => score += 0.05,
            _ => score -= 2.0,
        }
    }
    score += (letters as f64 / data.len() as f64) * 10.0;
    score += (spaces as f64 / data.len() as f64) * 8.0;
    score
}

pub fn best_single_byte_key(data: &[u8]) -> (u8, f64, Vec<u8>) {
    let mut best_key = 0u8;
    let mut best_score = f64::NEG_INFINITY;
    let mut best_plain = Vec::new();
    for key in 0u8..=255 {
        let plain = xor_bytes(data, &[key]);
        let score = english_score(&plain);
        if score > best_score {
            best_score = score;
            best_key = key;
            best_plain = plain;
        }
    }
    (best_key, best_score, best_plain)
}

/// Recover repeating-key XOR. Tries the top Hamming-distance key lengths
/// (or a fixed length) and ranks recovered plaintexts by english_score.
pub fn crack_repeating_key(
    data: &[u8],
    max_len: usize,
    top_lengths: usize,
    fixed_length: Option<usize>,
) -> Vec<XorCrackCandidate> {
    if data.is_empty() {
        return Vec::new();
    }

    let lengths: Vec<usize> = if let Some(k) = fixed_length {
        if k == 0 {
            return Vec::new();
        }
        vec![k.min(data.len())]
    } else if data.len() < 4 {
        // Fall back to single-byte
        vec![1]
    } else {
        let mut lens: Vec<usize> = detect_key_length(data, max_len)
            .into_iter()
            .map(|(len, _)| len)
            .take(top_lengths.max(1))
            .collect();
        if !lens.contains(&1) {
            lens.push(1);
        }
        lens
    };

    let mut candidates = Vec::new();
    for key_len in lengths {
        if key_len == 0 || key_len > data.len() {
            continue;
        }
        let mut key = vec![0u8; key_len];
        for (pos, slot) in key.iter_mut().enumerate() {
            let column: Vec<u8> = data.iter().skip(pos).step_by(key_len).copied().collect();
            if column.is_empty() {
                continue;
            }
            let (k, _, _) = best_single_byte_key(&column);
            *slot = k;
        }
        let plaintext = xor_bytes(data, &key);
        let score = english_score(&plaintext);
        candidates.push(XorCrackCandidate {
            key,
            plaintext,
            score,
        });
    }

    candidates.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    candidates
}

/// Crib-drag: at each offset, derive the key fragment CT⊕crib and
/// re-apply it (repeating if possible) to a local window for inspection.
pub fn crib_drag(ciphertext: &[u8], crib: &[u8]) -> Vec<CribHit> {
    if crib.is_empty() || crib.len() > ciphertext.len() {
        return Vec::new();
    }

    let mut hits = Vec::new();
    let max_off = ciphertext.len() - crib.len();
    for offset in 0..=max_off {
        let key_fragment: Vec<u8> = ciphertext[offset..offset + crib.len()]
            .iter()
            .zip(crib.iter())
            .map(|(c, p)| c ^ p)
            .collect();

        // Expand key fragment cyclically across a local window for context
        let win_start = offset.saturating_sub(8);
        let win_end = (offset + crib.len() + 8).min(ciphertext.len());
        let window = &ciphertext[win_start..win_end];
        // Key is only known for crib.len() bytes; for crib drag display we
        // show CT⊕key_fragment cycling over the crib-length key at this offset.
        let mut plain_window = Vec::with_capacity(window.len());
        for (i, &b) in window.iter().enumerate() {
            let abs = win_start + i;
            if abs >= offset && abs < offset + crib.len() {
                plain_window.push(b ^ key_fragment[abs - offset]);
            } else if !key_fragment.is_empty() {
                // Speculative cyclic extension from the recovered fragment
                let k = key_fragment[(abs.wrapping_sub(offset)) % key_fragment.len()];
                plain_window.push(b ^ k);
            } else {
                plain_window.push(b);
            }
        }

        let printable_ratio = plain_window
            .iter()
            .filter(|&&b| (0x20..=0x7e).contains(&b) || b == b'\n' || b == b'\t')
            .count() as f64
            / plain_window.len() as f64;

        if printable_ratio < 0.6 {
            continue;
        }

        hits.push(CribHit {
            offset,
            key_fragment,
            plaintext_window: String::from_utf8_lossy(&plain_window).into_owned(),
        });
    }
    hits
}

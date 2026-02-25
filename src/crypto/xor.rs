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

    results.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    results
}

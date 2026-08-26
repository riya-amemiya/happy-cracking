use anyhow::{Context, Result};
use clap::Subcommand;
use std::collections::HashMap;

#[derive(Subcommand)]
pub enum VigenereAction {
    #[command(about = "Encrypt with Vigenère cipher")]
    Encrypt {
        #[arg(help = "Input text")]
        input: String,
        #[arg(short, long, help = "Encryption key (alphabetic)")]
        key: String,
    },
    #[command(about = "Decrypt Vigenère cipher")]
    Decrypt {
        #[arg(help = "Encrypted text")]
        input: String,
        #[arg(short, long, help = "Decryption key (alphabetic)")]
        key: String,
    },
    #[command(about = "Auto-crack Vigenère cipher using frequency analysis")]
    Crack {
        #[arg(help = "Ciphertext to crack")]
        input: String,
        #[arg(short, long, help = "Known key length (if omitted, auto-detected)")]
        key_length: Option<usize>,
    },
    #[command(about = "Estimate key length using Kasiski examination and Index of Coincidence")]
    KeyLength {
        #[arg(help = "Ciphertext to analyze")]
        input: String,
        #[arg(short, long, default_value_t = 20, help = "Maximum key length to test")]
        max_length: usize,
    },
}

pub fn run(action: VigenereAction) -> Result<()> {
    match action {
        VigenereAction::Encrypt { input, key } => {
            println!("{}", encrypt(&input, &key)?);
        }
        VigenereAction::Decrypt { input, key } => {
            println!("{}", decrypt(&input, &key)?);
        }
        VigenereAction::Crack { input, key_length } => {
            let key_len = match key_length {
                Some(len) => {
                    check_key_length(len)?;
                    len
                }
                None => {
                    let candidates = estimate_key_length(&input, 20);
                    if candidates.is_empty() {
                        anyhow::bail!(
                            "Could not determine key length. Try specifying --key-length"
                        );
                    }
                    candidates[0].0
                }
            };
            let key = recover_key(&input, key_len)?;
            let plaintext = decrypt(&input, &key)?;
            println!("Key: {}", key);
            println!("Plaintext: {}", plaintext);
        }
        VigenereAction::KeyLength { input, max_length } => {
            if max_length > MAX_KEY_LENGTH {
                check_key_length(max_length)?;
            }
            let candidates = estimate_key_length(&input, max_length);
            println!("Key length candidates (by Index of Coincidence):");
            println!("{:<8} {:<10}", "Length", "IoC");
            println!("{}", "-".repeat(20));
            for (len, ioc) in &candidates {
                println!("{:<8} {:<10.6}", len, ioc);
            }
            if let Some((best, _)) = candidates.first() {
                println!("\nMost likely key length: {}", best);
            }
        }
    }
    Ok(())
}

// Non-ASCII UTF-8 continuation bytes have the high bit set, so
// `is_ascii_alphabetic()` returns false for them and they pass through
// unchanged — preserving the original char-based semantics exactly.
pub fn encrypt(input: &str, key: &str) -> Result<String> {
    let key_bytes = key.as_bytes();
    if key_bytes.is_empty() || !key_bytes.iter().all(u8::is_ascii_alphabetic) {
        anyhow::bail!("Key must be non-empty and contain only alphabetic characters");
    }

    let mut bytes = input.as_bytes().to_vec();
    let key_len = key_bytes.len();
    let mut key_index = 0usize;

    for b in &mut bytes {
        if b.is_ascii_alphabetic() {
            let base = if b.is_ascii_uppercase() { b'A' } else { b'a' };
            let shift = (key_bytes[key_index % key_len] | 0x20) - b'a';
            *b = (*b - base + shift) % 26 + base;
            key_index += 1;
        }
    }

    String::from_utf8(bytes).context("vigenere encrypt: produced invalid UTF-8")
}

pub fn decrypt(input: &str, key: &str) -> Result<String> {
    let key_bytes = key.as_bytes();
    if key_bytes.is_empty() || !key_bytes.iter().all(u8::is_ascii_alphabetic) {
        anyhow::bail!("Key must be non-empty and contain only alphabetic characters");
    }

    let mut bytes = input.as_bytes().to_vec();
    let key_len = key_bytes.len();
    let mut key_index = 0usize;

    for b in &mut bytes {
        if b.is_ascii_alphabetic() {
            let base = if b.is_ascii_uppercase() { b'A' } else { b'a' };
            let shift = (key_bytes[key_index % key_len] | 0x20) - b'a';
            *b = (*b - base + 26 - shift) % 26 + base;
            key_index += 1;
        }
    }

    String::from_utf8(bytes).context("vigenere decrypt: produced invalid UTF-8")
}

// English letter frequencies (A-Z) as proportions
const ENGLISH_FREQ: [f64; 26] = [
    0.082, 0.015, 0.028, 0.043, 0.127, 0.022, 0.020, 0.061, 0.070, 0.0015, 0.008, 0.040, 0.024,
    0.067, 0.075, 0.019, 0.001, 0.060, 0.063, 0.091, 0.028, 0.010, 0.024, 0.0015, 0.020, 0.0007,
];

// Expected IoC for English text
const ENGLISH_IOC: f64 = 0.0667;

fn extract_alpha(input: &str) -> Vec<u8> {
    input
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_uppercase() as u8 - b'A')
        .collect()
}

fn kasiski_examination(input: &str, max_key_len: usize) -> HashMap<usize, usize> {
    let letters = extract_alpha(input);
    let mut distances: Vec<usize> = Vec::new();

    for trigram_len in 3..=5 {
        if letters.len() < trigram_len {
            break;
        }
        let mut positions: HashMap<u32, Vec<usize>> = HashMap::new();
        for i in 0..=letters.len() - trigram_len {
            let mut trigram: u32 = 0;
            for j in 0..trigram_len {
                trigram = trigram * 26 + (letters[i + j] as u32);
            }
            positions.entry(trigram).or_default().push(i);
        }
        for pos_list in positions.values() {
            if pos_list.len() >= 2 {
                for i in 0..pos_list.len() - 1 {
                    distances.push(pos_list[i + 1] - pos_list[i]);
                }
            }
        }
    }

    let mut factor_counts: HashMap<usize, usize> = HashMap::new();
    for d in &distances {
        let limit = (*d).min(max_key_len);
        for f in 2..=limit {
            if d % f == 0 {
                *factor_counts.entry(f).or_insert(0) += 1;
            }
        }
    }
    factor_counts
}

/// Maximum key length for Vigenère crack / key-length estimation.
///
/// SECURITY: `--key-length` and `--max-length` are user-controlled loop and
/// allocation bounds. `recover_key` does `String::with_capacity(key_length)`
/// and iterates `0..key_length`; `estimate_key_length` scans O(max_length * n).
/// Unbounded values allow CPU/memory exhaustion.
pub const MAX_KEY_LENGTH: usize = 256;

pub fn check_key_length(len: usize) -> Result<()> {
    if len == 0 {
        anyhow::bail!("Key length must be at least 1");
    }
    if len > MAX_KEY_LENGTH {
        anyhow::bail!(
            "Key length {} exceeds the maximum allowed of {} to prevent Denial of Service",
            len,
            MAX_KEY_LENGTH
        );
    }
    Ok(())
}

pub fn estimate_key_length(input: &str, max_length: usize) -> Vec<(usize, f64)> {
    let max_length = max_length.min(MAX_KEY_LENGTH);
    let letters = extract_alpha(input);
    if letters.len() < 2 {
        return Vec::new();
    }

    let mut results: Vec<(usize, f64)> = Vec::new();

    for key_len in 1..=max_length.min(letters.len() / 2) {
        let mut total_ioc = 0.0;
        let mut count = 0;
        for col in 0..key_len {
            let mut counts = [0usize; 26];
            let mut n: usize = 0;
            let mut idx = col;
            while idx < letters.len() {
                counts[letters[idx] as usize] += 1;
                n += 1;
                idx += key_len;
            }
            if n >= 2 {
                let n_f = n as f64;
                let sum: f64 = counts.iter().map(|&c| c as f64 * (c as f64 - 1.0)).sum();
                total_ioc += sum / (n_f * (n_f - 1.0));
                count += 1;
            }
        }
        if count > 0 {
            let avg_ioc = total_ioc / count as f64;
            results.push((key_len, avg_ioc));
        }
    }

    let kasiski = kasiski_examination(input, max_length);

    results.sort_by(|a, b| {
        let a_diff = (a.1 - ENGLISH_IOC).abs();
        let b_diff = (b.1 - ENGLISH_IOC).abs();
        let a_kasiski = *kasiski.get(&a.0).unwrap_or(&0) as f64;
        let b_kasiski = *kasiski.get(&b.0).unwrap_or(&0) as f64;
        a_diff
            .partial_cmp(&b_diff)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                b_kasiski
                    .partial_cmp(&a_kasiski)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });

    results.truncate(10);
    results
}

pub fn recover_key(input: &str, key_length: usize) -> Result<String> {
    check_key_length(key_length)?;
    let letters = extract_alpha(input);
    let mut key = String::with_capacity(key_length);

    for col in 0..key_length {
        let mut counts = [0usize; 26];
        let mut n: usize = 0;
        let mut idx = col;
        while idx < letters.len() {
            counts[letters[idx] as usize] += 1;
            n += 1;
            idx += key_length;
        }
        let best_shift = find_best_shift(&counts, n);
        key.push((b'A' + best_shift) as char);
    }

    Ok(key)
}

fn find_best_shift(counts: &[usize; 26], n: usize) -> u8 {
    if n == 0 {
        return 0;
    }

    let n_f = n as f64;

    let mut best_shift = 0u8;
    let mut best_chi2 = f64::MAX;

    for shift in 0..26u8 {
        let mut chi2 = 0.0;
        for i in 0..26 {
            let observed = counts[(i + shift as usize) % 26] as f64;
            let expected = ENGLISH_FREQ[i] * n_f;
            if expected > 0.0 {
                chi2 += (observed - expected).powi(2) / expected;
            }
        }
        if chi2 < best_chi2 {
            best_chi2 = chi2;
            best_shift = shift;
        }
    }

    best_shift
}

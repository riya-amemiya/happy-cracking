use anyhow::Result;
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
                    if len == 0 {
                        anyhow::bail!("Key length must be at least 1");
                    }
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
            let key = recover_key(&input, key_len);
            let plaintext = decrypt(&input, &key)?;
            println!("Key: {}", key);
            println!("Plaintext: {}", plaintext);
        }
        VigenereAction::KeyLength { input, max_length } => {
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

pub fn encrypt(input: &str, key: &str) -> Result<String> {
    if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphabetic()) {
        anyhow::bail!("Key must be non-empty and contain only alphabetic characters");
    }

    let key_upper: Vec<u8> = key.to_uppercase().bytes().collect();
    let mut key_index = 0;

    Ok(input
        .chars()
        .map(|c| {
            if c.is_ascii_alphabetic() {
                let base = if c.is_ascii_uppercase() { b'A' } else { b'a' };
                let shift = key_upper[key_index % key_upper.len()] - b'A';
                key_index += 1;
                (((c as u8 - base) + shift) % 26 + base) as char
            } else {
                c
            }
        })
        .collect())
}

pub fn decrypt(input: &str, key: &str) -> Result<String> {
    if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphabetic()) {
        anyhow::bail!("Key must be non-empty and contain only alphabetic characters");
    }

    let key_upper: Vec<u8> = key.to_uppercase().bytes().collect();
    let mut key_index = 0;

    Ok(input
        .chars()
        .map(|c| {
            if c.is_ascii_alphabetic() {
                let base = if c.is_ascii_uppercase() { b'A' } else { b'a' };
                let shift = key_upper[key_index % key_upper.len()] - b'A';
                key_index += 1;
                (((c as u8 - base) + 26 - shift) % 26 + base) as char
            } else {
                c
            }
        })
        .collect())
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

fn index_of_coincidence(data: &[u8]) -> f64 {
    if data.len() < 2 {
        return 0.0;
    }
    let mut counts = [0usize; 26];
    for &b in data {
        counts[b as usize] += 1;
    }
    let n = data.len() as f64;
    let sum: f64 = counts.iter().map(|&c| c as f64 * (c as f64 - 1.0)).sum();
    sum / (n * (n - 1.0))
}

fn kasiski_examination(input: &str, max_key_len: usize) -> HashMap<usize, usize> {
    let letters = extract_alpha(input);
    let mut distances: Vec<usize> = Vec::new();

    // Find repeated trigrams and record distances
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
                // Optimization: only consider adjacent occurrences.
                // This reduces complexity from O(N^2) to O(N).
                for i in 0..pos_list.len() - 1 {
                    distances.push(pos_list[i + 1] - pos_list[i]);
                }
            }
        }
    }

    // Count factor occurrences
    let mut factor_counts: HashMap<usize, usize> = HashMap::new();
    for d in &distances {
        // Optimization: only check factors up to max_key_len.
        // We only care about factors that are plausible key lengths.
        let limit = (*d).min(max_key_len);
        for f in 2..=limit {
            if d % f == 0 {
                *factor_counts.entry(f).or_insert(0) += 1;
            }
        }
    }
    factor_counts
}

pub fn estimate_key_length(input: &str, max_length: usize) -> Vec<(usize, f64)> {
    let letters = extract_alpha(input);
    if letters.len() < 2 {
        return Vec::new();
    }

    let mut results: Vec<(usize, f64)> = Vec::new();

    for key_len in 1..=max_length.min(letters.len() / 2) {
        // Split into columns and compute average IoC
        let mut total_ioc = 0.0;
        let mut count = 0;
        for col in 0..key_len {
            let column: Vec<u8> = letters.iter().skip(col).step_by(key_len).copied().collect();
            if column.len() >= 2 {
                total_ioc += index_of_coincidence(&column);
                count += 1;
            }
        }
        if count > 0 {
            let avg_ioc = total_ioc / count as f64;
            results.push((key_len, avg_ioc));
        }
    }

    // Use Kasiski to boost scores for lengths supported by repeated ngrams
    let kasiski = kasiski_examination(input, max_length);

    // Sort by how close IoC is to English IoC, with Kasiski as tiebreaker
    results.sort_by(|a, b| {
        let a_diff = (a.1 - ENGLISH_IOC).abs();
        let b_diff = (b.1 - ENGLISH_IOC).abs();
        let a_kasiski = *kasiski.get(&a.0).unwrap_or(&0) as f64;
        let b_kasiski = *kasiski.get(&b.0).unwrap_or(&0) as f64;
        // Primary sort by IoC closeness to English, secondary by Kasiski support
        a_diff
            .partial_cmp(&b_diff)
            .unwrap()
            .then(b_kasiski.partial_cmp(&a_kasiski).unwrap())
    });

    results.truncate(10);
    results
}

pub fn recover_key(input: &str, key_length: usize) -> String {
    let letters = extract_alpha(input);
    let mut key = String::with_capacity(key_length);

    for col in 0..key_length {
        let column: Vec<u8> = letters
            .iter()
            .skip(col)
            .step_by(key_length)
            .copied()
            .collect();
        let best_shift = find_best_shift(&column);
        key.push((b'A' + best_shift) as char);
    }

    key
}

fn find_best_shift(column: &[u8]) -> u8 {
    if column.is_empty() {
        return 0;
    }

    let mut counts = [0usize; 26];
    for &b in column {
        counts[b as usize] += 1;
    }
    let n = column.len() as f64;

    // Use chi-squared statistic to find the best shift
    let mut best_shift = 0u8;
    let mut best_chi2 = f64::MAX;

    for shift in 0..26u8 {
        let mut chi2 = 0.0;
        for i in 0..26 {
            let observed = counts[(i + shift as usize) % 26] as f64;
            let expected = ENGLISH_FREQ[i] * n;
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

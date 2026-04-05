use anyhow::Result;
use clap::Subcommand;
use std::collections::HashMap;

#[derive(Subcommand)]
pub enum FrequencyAction {
    #[command(about = "Analyze character frequency")]
    Analyze {
        #[arg(help = "Input text")]
        input: String,
        #[arg(short, long, help = "Show only alphabetic characters")]
        alpha_only: bool,
    },
    #[command(about = "Chi-squared test against English letter frequencies")]
    ChiSquared {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Calculate Index of Coincidence (IoC)")]
    Ioc {
        #[arg(help = "Input text")]
        input: String,
    },
}

pub fn run(action: FrequencyAction) -> Result<()> {
    match action {
        FrequencyAction::Analyze { input, alpha_only } => {
            let result = analyze(&input, alpha_only);
            print_analysis(&result);
        }
        FrequencyAction::ChiSquared { input } => {
            let score = chi_squared(&input);
            println!("Chi-squared score: {:.4}", score);
            println!("(Lower score = closer to English letter distribution)");
            if score < 50.0 {
                println!("Likely English text");
            } else if score < 100.0 {
                println!("Possibly English text");
            } else {
                println!("Unlikely to be English text");
            }
        }
        FrequencyAction::Ioc { input } => {
            let ioc = index_of_coincidence(&input);
            println!("Index of Coincidence: {:.6}", ioc);
            println!("English text ~0.0667, random text ~0.0385");
            if (ioc - 0.0667).abs() < 0.01 {
                println!("Consistent with monoalphabetic cipher or English text");
            } else if (ioc - 0.0385).abs() < 0.01 {
                println!("Consistent with polyalphabetic cipher or random text");
            }
        }
    }
    Ok(())
}

// English letter frequencies (percentage)
const ENGLISH_FREQ: &[(char, f64)] = &[
    ('E', 12.7),
    ('T', 9.1),
    ('A', 8.2),
    ('O', 7.5),
    ('I', 7.0),
    ('N', 6.7),
    ('S', 6.3),
    ('H', 6.1),
    ('R', 6.0),
    ('D', 4.3),
    ('L', 4.0),
    ('C', 2.8),
    ('U', 2.8),
    ('M', 2.4),
    ('W', 2.4),
    ('F', 2.2),
    ('G', 2.0),
    ('Y', 2.0),
    ('P', 1.9),
    ('B', 1.5),
    ('V', 1.0),
    ('K', 0.8),
    ('J', 0.15),
    ('X', 0.15),
    ('Q', 0.10),
    ('Z', 0.07),
];

#[derive(Debug)]
pub struct FrequencyResult {
    pub frequencies: Vec<(char, usize, f64)>, // (char, count, percentage)
    pub total_chars: usize,
}

pub fn analyze(input: &str, alpha_only: bool) -> FrequencyResult {
    let mut frequencies: Vec<(char, usize, f64)> = Vec::new();
    let mut total = 0usize;

    if alpha_only {
        // Optimization: Use array instead of HashMap for pure alphabetic analysis
        let mut counts = [0usize; 26];

        for c in input.chars() {
            if c.is_ascii_alphabetic() {
                counts[(c.to_ascii_uppercase() as u8 - b'A') as usize] += 1;
                total += 1;
            }
        }

        for (i, &count) in counts.iter().enumerate() {
            if count > 0 {
                let percentage = if total > 0 {
                    (count as f64 / total as f64) * 100.0
                } else {
                    0.0
                };
                frequencies.push(((b'A' + i as u8) as char, count, percentage));
            }
        }
    } else {
        let mut counts: HashMap<char, usize> = HashMap::new();
        for c in input.chars() {
            *counts.entry(c).or_insert(0) += 1;
            total += 1;
        }

        frequencies = counts
            .into_iter()
            .map(|(c, count)| {
                let percentage = if total > 0 {
                    (count as f64 / total as f64) * 100.0
                } else {
                    0.0
                };
                (c, count, percentage)
            })
            .collect();
    }

    // Sort by count descending
    frequencies.sort_by_key(|b| std::cmp::Reverse(b.1));

    FrequencyResult {
        frequencies,
        total_chars: total,
    }
}

// Chi-squared test comparing input letter frequencies against English.
// Lower score means closer to English distribution.
pub fn chi_squared(input: &str) -> f64 {
    let mut counts = [0u32; 26];
    let mut total = 0u32;

    for c in input.chars() {
        if c.is_ascii_alphabetic() {
            counts[(c.to_ascii_uppercase() as u8 - b'A') as usize] += 1;
            total += 1;
        }
    }

    if total == 0 {
        return f64::INFINITY;
    }

    let total_f = total as f64;
    ENGLISH_FREQ
        .iter()
        .map(|&(ch, expected_pct)| {
            let observed = counts[(ch as u8 - b'A') as usize] as f64;
            let expected = expected_pct / 100.0 * total_f;
            if expected > 0.0 {
                (observed - expected).powi(2) / expected
            } else {
                0.0
            }
        })
        .sum()
}

// Index of Coincidence (IoC) for the alphabetic characters in input.
// English text has IoC ~0.0667, random text ~0.0385 (1/26).
pub fn index_of_coincidence(input: &str) -> f64 {
    let mut counts = [0u64; 26];
    let mut total = 0u64;

    for c in input.chars() {
        if c.is_ascii_alphabetic() {
            counts[(c.to_ascii_uppercase() as u8 - b'A') as usize] += 1;
            total += 1;
        }
    }

    if total <= 1 {
        return 0.0;
    }

    let numerator: u64 = counts.iter().map(|&n| n * n.saturating_sub(1)).sum();
    numerator as f64 / (total * (total - 1)) as f64
}

fn print_analysis(result: &FrequencyResult) {
    println!("Character Frequency Analysis");
    println!("============================");
    println!("Total characters: {}", result.total_chars);
    println!();

    println!("{:<6} {:>6} {:>8}   English %", "Char", "Count", "Freq %");
    println!("{}", "-".repeat(40));

    for (c, count, percentage) in &result.frequencies {
        let english_freq = ENGLISH_FREQ
            .iter()
            .find(|(ch, _)| *ch == c.to_ascii_uppercase())
            .map(|(_, f)| format!("{:.1}%", f))
            .unwrap_or_default();

        let display_char = if *c == ' ' {
            "SPACE".to_string()
        } else if *c == '\n' {
            "\\n".to_string()
        } else if *c == '\t' {
            "\\t".to_string()
        } else {
            c.to_string()
        };

        println!(
            "{:<6} {:>6} {:>7.1}%   {}",
            display_char, count, percentage, english_freq
        );
    }
}

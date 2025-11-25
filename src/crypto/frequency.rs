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
}

pub fn run(action: FrequencyAction) -> Result<()> {
    match action {
        FrequencyAction::Analyze { input, alpha_only } => {
            let result = analyze(&input, alpha_only);
            print_analysis(&result);
        }
    }
    Ok(())
}

#[derive(Debug)]
pub struct FrequencyResult {
    pub frequencies: Vec<(char, usize, f64)>, // (char, count, percentage)
    pub total_chars: usize,
}

pub fn analyze(input: &str, alpha_only: bool) -> FrequencyResult {
    let mut counts: HashMap<char, usize> = HashMap::new();

    let chars: Vec<char> = if alpha_only {
        input
            .chars()
            .filter(|c| c.is_ascii_alphabetic())
            .map(|c| c.to_ascii_uppercase())
            .collect()
    } else {
        input.chars().collect()
    };

    for c in &chars {
        *counts.entry(*c).or_insert(0) += 1;
    }

    let total = chars.len();
    let mut frequencies: Vec<(char, usize, f64)> = counts
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

    // Sort by count descending
    frequencies.sort_by(|a, b| b.1.cmp(&a.1));

    FrequencyResult {
        frequencies,
        total_chars: total,
    }
}

fn print_analysis(result: &FrequencyResult) {
    println!("Character Frequency Analysis");
    println!("============================");
    println!("Total characters: {}", result.total_chars);
    println!();

    // English letter frequency for comparison
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

use anyhow::Result;
use clap::Subcommand;

use super::frequency;

#[derive(Subcommand)]
pub enum CipheridAction {
    #[command(about = "Heuristically identify the encoding or cipher of a text")]
    Analyze {
        #[arg(help = "Ciphertext / encoded text to analyze")]
        input: String,
    },
}

pub fn run(action: CipheridAction) -> Result<()> {
    match action {
        CipheridAction::Analyze { input } => {
            let candidates = analyze(&input);
            print_candidates(&candidates);
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct Candidate {
    pub name: String,
    pub confidence: f64,
    pub reason: String,
}

const STRONG_THRESHOLD: f64 = 0.3;

const COMMON_WORDS: &[&str] = &[
    "the", "be", "to", "of", "and", "a", "in", "that", "have", "it", "for", "not", "on", "with",
    "he", "as", "you", "do", "at", "this", "but", "his", "by", "from", "they", "we", "say", "her",
    "she", "or", "an", "will", "my", "one", "all", "would", "there", "their", "flag", "is", "are",
    "was", "were", "what", "when", "where", "who", "how",
];

const NATO_WORDS: &[&str] = &[
    "alfa", "alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel", "india",
    "juliet", "juliett", "kilo", "lima", "mike", "november", "oscar", "papa", "quebec", "romeo",
    "sierra", "tango", "uniform", "victor", "whiskey", "xray", "yankee", "zulu", "zero", "one",
    "two", "three", "four", "five", "six", "seven", "eight", "nine",
];

pub fn analyze(input: &str) -> Vec<Candidate> {
    let mut candidates: Vec<Candidate> = Vec::new();
    let trimmed = input.trim();

    if trimmed.is_empty() {
        return candidates;
    }

    detect_formats(trimmed, &mut candidates);
    detect_statistics(trimmed, &mut candidates);

    candidates.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.name.cmp(&b.name))
    });

    candidates
}

fn detect_formats(s: &str, out: &mut Vec<Candidate>) {
    let len = s.chars().count();

    if let Some(reason) = looks_like_flag(s) {
        out.push(Candidate {
            name: "flag / plaintext marker".to_string(),
            confidence: 0.97,
            reason,
        });
    }

    if len >= 2 && len.is_multiple_of(2) && s.chars().all(|c| c.is_ascii_hexdigit()) {
        let has_hex_letter = s.chars().any(|c| c.is_ascii_alphabetic());
        let confidence = if has_hex_letter { 0.9 } else { 0.55 };
        out.push(Candidate {
            name: "Hex".to_string(),
            confidence,
            reason: format!("{} hex digits, even length", len),
        });
    }

    let non_ws: Vec<char> = s.chars().filter(|c| !c.is_whitespace()).collect();
    if non_ws.len() >= 8 && non_ws.iter().all(|&c| c == '0' || c == '1') {
        let confidence = if non_ws.len().is_multiple_of(8) {
            0.92
        } else {
            0.7
        };
        out.push(Candidate {
            name: "Binary".to_string(),
            confidence,
            reason: format!("{} bits of 0/1 only", non_ws.len()),
        });
    }

    if s.chars()
        .all(|c| c == '.' || c == '-' || c == '/' || c == ' ')
        && s.chars().any(|c| c == '.' || c == '-')
    {
        out.push(Candidate {
            name: "Morse".to_string(),
            confidence: 0.93,
            reason: "only dots, dashes, slashes and spaces".to_string(),
        });
    }

    if let Some(c) = detect_decimal(s) {
        out.push(c);
    }

    let braille = s
        .chars()
        .filter(|&c| ('\u{2800}'..='\u{28FF}').contains(&c))
        .count();
    if braille > 0
        && s.chars()
            .all(|c| ('\u{2800}'..='\u{28FF}').contains(&c) || c.is_whitespace())
    {
        out.push(Candidate {
            name: "Braille".to_string(),
            confidence: 0.95,
            reason: format!("{} characters in the Braille unicode block", braille),
        });
    }

    if let Some(c) = detect_nato(s) {
        out.push(c);
    }

    if len >= 4
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() && !matches!(c, '0' | 'O' | 'I' | 'l'))
    {
        let confidence = if s.chars().any(|c| !c.is_ascii_hexdigit()) {
            0.45
        } else {
            0.2
        };
        out.push(Candidate {
            name: "Base58".to_string(),
            confidence,
            reason: "alphanumeric without 0/O/I/l".to_string(),
        });
    }

    let b32_body = s.trim_end_matches('=');
    if len >= 8
        && len.is_multiple_of(8)
        && !b32_body.is_empty()
        && b32_body
            .chars()
            .all(|c| c.is_ascii_uppercase() || matches!(c, '2'..='7'))
    {
        out.push(Candidate {
            name: "Base32".to_string(),
            confidence: 0.82,
            reason: "A-Z and 2-7 alphabet, padded length multiple of 8".to_string(),
        });
    }

    let b64_body = s.trim_end_matches('=');
    if len >= 4
        && len.is_multiple_of(4)
        && !b64_body.is_empty()
        && b64_body
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/')
    {
        let distinctive = s.contains('=')
            || s.chars()
                .any(|c| c.is_ascii_lowercase() || c == '+' || c == '/');
        let confidence = if distinctive { 0.9 } else { 0.6 };
        out.push(Candidate {
            name: "Base64".to_string(),
            confidence,
            reason: "Base64 alphabet, length multiple of 4".to_string(),
        });
    }

    if len >= 5
        && s.chars().all(|c| ('\u{21}'..='\u{75}').contains(&c))
        && s.chars().any(|c| !c.is_ascii_alphanumeric())
    {
        out.push(Candidate {
            name: "Base85".to_string(),
            confidence: 0.4,
            reason: "printable ASCII in the Base85 ('!'..'u') range".to_string(),
        });
    }
}

fn looks_like_flag(s: &str) -> Option<String> {
    let lower = s.to_lowercase();

    if let Some(open) = lower.find('{')
        && lower[open..].contains('}')
    {
        let prefix = &lower[..open];
        if !prefix.is_empty()
            && prefix
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Some(format!("contains '{}{{...}}' CTF flag marker", prefix));
        }
    }
    None
}

fn detect_decimal(s: &str) -> Option<Candidate> {
    let parts: Vec<&str> = s
        .split(['-', ' ', ',', '.', '\t', '\n'])
        .filter(|p| !p.is_empty())
        .collect();

    if parts.len() < 2 {
        return None;
    }
    if !parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit())) {
        return None;
    }

    let values: Vec<u32> = parts.iter().filter_map(|p| p.parse::<u32>().ok()).collect();
    if values.len() == parts.len() && values.iter().all(|&v| (1..=26).contains(&v)) {
        Some(Candidate {
            name: "A1Z26 / decimal".to_string(),
            confidence: 0.9,
            reason: format!(
                "{} groups, all within 1..=26 (letter positions)",
                values.len()
            ),
        })
    } else {
        Some(Candidate {
            name: "Decimal / numeric".to_string(),
            confidence: 0.6,
            reason: format!("{} separator-delimited decimal groups", parts.len()),
        })
    }
}

fn detect_nato(s: &str) -> Option<Candidate> {
    let tokens: Vec<&str> = s.split_whitespace().collect();
    if tokens.len() < 2 {
        return None;
    }
    let lower_tokens: Vec<String> = tokens.iter().map(|t| t.to_lowercase()).collect();
    let matched = lower_tokens
        .iter()
        .filter(|t| NATO_WORDS.contains(&t.as_str()))
        .count();
    let ratio = matched as f64 / tokens.len() as f64;
    if ratio >= 0.6 {
        Some(Candidate {
            name: "NATO phonetic".to_string(),
            confidence: 0.5 + 0.45 * ratio,
            reason: format!("{}/{} tokens are NATO words", matched, tokens.len()),
        })
    } else {
        None
    }
}

fn detect_statistics(s: &str, out: &mut Vec<Candidate>) {
    let alpha_count = s.chars().filter(|c| c.is_ascii_alphabetic()).count();

    if alpha_count < 20 {
        if let Some(c) = detect_english(s) {
            out.push(c);
        }
        return;
    }

    let ioc = frequency::index_of_coincidence(s);

    if ioc >= 0.058 {
        out.push(Candidate {
            name: "Monoalphabetic (substitution / Caesar / Atbash) or plaintext".to_string(),
            confidence: 0.75,
            reason: format!(
                "IoC = {:.4} (~0.067 expected for English/monoalphabetic)",
                ioc
            ),
        });
    } else if ioc <= 0.05 {
        out.push(Candidate {
            name: "Polyalphabetic (Vigenere / Beaufort)".to_string(),
            confidence: 0.75,
            reason: format!(
                "IoC = {:.4} (~0.038 expected for polyalphabetic/random)",
                ioc
            ),
        });
    } else {
        out.push(Candidate {
            name: "Polyalphabetic or short monoalphabetic".to_string(),
            confidence: 0.45,
            reason: format!(
                "IoC = {:.4} (ambiguous, between mono- and polyalphabetic)",
                ioc
            ),
        });
    }

    if let Some(c) = detect_english(s) {
        out.push(c);
    }
}

fn detect_english(s: &str) -> Option<Candidate> {
    let lower = s.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_ascii_alphabetic())
        .filter(|w| !w.is_empty())
        .collect();

    if words.is_empty() {
        return None;
    }

    let common_hits = words.iter().filter(|w| COMMON_WORDS.contains(w)).count();
    let chi = frequency::chi_squared(s);

    if common_hits >= 2 {
        let confidence = (0.6 + 0.1 * common_hits as f64).min(0.95);
        return Some(Candidate {
            name: "English plaintext".to_string(),
            confidence,
            reason: format!(
                "{} common English words found, chi-squared = {:.1}",
                common_hits, chi
            ),
        });
    }

    if chi.is_finite() && chi < 50.0 {
        return Some(Candidate {
            name: "English plaintext".to_string(),
            confidence: 0.5,
            reason: format!(
                "letter distribution close to English (chi-squared = {:.1})",
                chi
            ),
        });
    }

    None
}

fn print_candidates(candidates: &[Candidate]) {
    if candidates.is_empty() {
        println!("No candidates: input is empty.");
        return;
    }

    let strong = candidates.iter().any(|c| c.confidence >= STRONG_THRESHOLD);
    if !strong {
        println!("No strong match. Showing the top guesses:");
    }

    for c in candidates {
        println!(
            "{}  (confidence {:.2})  - {}",
            c.name, c.confidence, c.reason
        );
    }
}

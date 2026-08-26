use anyhow::Result;
use clap::Subcommand;
use std::collections::HashSet;

use super::{
    affine, atbash, autodecode, cipherid, frequency, railfence, rot, substitution, vigenere,
};

const FLAG_PATTERNS: &[&str] = &["flag{", "FLAG{", "ctf{", "CTF{", "picoctf{", "HK{", "hk{"];

const COMMON_WORDS: &[&str] = &[
    "the", "be", "to", "of", "and", "a", "in", "that", "have", "it", "for", "not", "on", "with",
    "he", "as", "you", "do", "at", "this", "but", "his", "by", "from", "they", "we", "flag", "is",
    "are", "was", "were", "hello", "world", "secret", "password", "key", "crypto",
];

#[derive(Subcommand)]
pub enum SolveAction {
    #[command(about = "Aggressively try encodings and classic ciphers against input")]
    Run {
        #[arg(help = "Ciphertext or encoded text")]
        input: String,
        #[arg(
            short,
            long,
            default_value_t = 15,
            help = "Maximum candidates to print"
        )]
        top: usize,
        #[arg(long, default_value_t = 8, help = "Max rail-fence rails to try")]
        max_rails: usize,
        #[arg(
            long,
            default_value_t = 800,
            help = "Hill-climbing iterations for substitution solve"
        )]
        substitution_iters: usize,
        #[arg(long, help = "Also run deep recursive decoding")]
        aggressive: bool,
        #[arg(
            long,
            default_value_t = 6,
            help = "Max recursive decode depth when --aggressive"
        )]
        max_depth: usize,
    },
}

#[derive(Debug, Clone)]
pub struct SolveCandidate {
    pub method: String,
    pub plaintext: String,
    pub score: f64,
    pub is_flag: bool,
}

pub fn run(action: SolveAction) -> Result<()> {
    match action {
        SolveAction::Run {
            input,
            top,
            max_rails,
            substitution_iters,
            aggressive,
            max_depth,
        } => {
            substitution::check_solve_iterations(substitution_iters)?;
            let results = solve(
                &input,
                SolveOptions {
                    max_rails,
                    substitution_iters,
                    aggressive,
                    max_depth,
                },
            );
            print_results(&results, top);
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct SolveOptions {
    pub max_rails: usize,
    pub substitution_iters: usize,
    pub aggressive: bool,
    pub max_depth: usize,
}

impl Default for SolveOptions {
    fn default() -> Self {
        Self {
            max_rails: 8,
            substitution_iters: 800,
            aggressive: false,
            max_depth: 6,
        }
    }
}

pub fn solve(input: &str, options: SolveOptions) -> Vec<SolveCandidate> {
    let mut out: Vec<SolveCandidate> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    let trimmed = input.trim();
    if trimmed.is_empty() {
        return out;
    }

    push_candidate(
        &mut out,
        &mut seen,
        "identity".to_string(),
        trimmed.to_string(),
    );

    for c in cipherid::analyze(trimmed) {
        if c.confidence >= 0.3 {
            push_candidate(
                &mut out,
                &mut seen,
                format!("cipherid:{}", c.name),
                format!("{} ({})", c.reason, c.confidence),
            );
        }
    }

    for (enc, decoded) in autodecode::detect_and_decode(trimmed) {
        push_candidate(&mut out, &mut seen, format!("decode:{}", enc), decoded);
    }

    if options.aggressive {
        for (path, decoded) in autodecode::decode_tree(trimmed, options.max_depth, 64) {
            push_candidate(&mut out, &mut seen, format!("aggressive:{}", path), decoded);
        }
    }

    try_classic_ciphers(trimmed, &options, &mut out, &mut seen);

    let decoded_layers: Vec<String> = out
        .iter()
        .filter(|c| c.method.starts_with("decode:") || c.method.starts_with("aggressive:"))
        .map(|c| c.plaintext.clone())
        .take(12)
        .collect();
    for layer in decoded_layers {
        try_classic_ciphers(&layer, &options, &mut out, &mut seen);
    }

    out.sort_by(|a, b| {
        b.is_flag
            .cmp(&a.is_flag)
            .then_with(|| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.method.cmp(&b.method))
    });
    out
}

fn push_candidate(
    out: &mut Vec<SolveCandidate>,
    seen: &mut HashSet<String>,
    method: String,
    plaintext: String,
) {
    if plaintext.is_empty() || !seen.insert(format!("{}|{}", method, plaintext)) {
        return;
    }
    let score = score_plaintext(&plaintext);
    let is_flag = looks_like_flag(&plaintext);
    out.push(SolveCandidate {
        method,
        plaintext,
        score,
        is_flag,
    });
}

fn try_classic_ciphers(
    text: &str,
    options: &SolveOptions,
    out: &mut Vec<SolveCandidate>,
    seen: &mut HashSet<String>,
) {
    let letters: String = text.chars().filter(|c| c.is_ascii_alphabetic()).collect();
    if letters.len() < 3 {
        return;
    }

    push_candidate(out, seen, "rot13".to_string(), rot::rot13(text));
    push_candidate(out, seen, "rot47".to_string(), rot::rot47(text));
    push_candidate(out, seen, "atbash".to_string(), atbash::transform(text));

    for shift in 0..26u8 {
        let plain = rot::rotate(text, (26 - (shift % 26)) % 26);
        push_candidate(out, seen, format!("caesar:shift={}", shift), plain);
    }

    let max_rails = options.max_rails.clamp(2, 20);
    for rails in 2..=max_rails {
        if let Ok(plain) = railfence::decrypt(text, rails) {
            push_candidate(out, seen, format!("railfence:rails={}", rails), plain);
        }
    }

    for &a in &[1i32, 3, 5, 7, 9, 11, 15, 17, 19, 21, 23, 25] {
        for b in 0..26i32 {
            if let Ok(plain) = affine::decrypt(text, a, b) {
                push_candidate(out, seen, format!("affine:a={},b={}", a, b), plain);
            }
        }
    }

    if letters.len() >= 20 {
        let key_lens = vigenere::estimate_key_length(text, 16);
        for &(key_len, _) in key_lens.iter().take(3) {
            if key_len == 0 {
                continue;
            }
            let Ok(key) = vigenere::recover_key(text, key_len) else {
                continue;
            };
            if let Ok(plain) = vigenere::decrypt(text, &key) {
                push_candidate(out, seen, format!("vigenere:key={}", key), plain);
            }
        }
    }

    if letters.len() >= 40
        && options.substitution_iters > 0
        && let Ok((key, plain, _score)) = substitution::solve(text, options.substitution_iters)
    {
        push_candidate(out, seen, format!("substitution:key={}", key), plain);
    }
}

pub fn looks_like_flag(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if FLAG_PATTERNS
        .iter()
        .any(|p| lower.contains(&p.to_ascii_lowercase()))
    {
        return true;
    }
    // Generic brace flag form like xxx{...}
    text.chars().any(|c| c == '{')
        && text.chars().any(|c| c == '}')
        && text.len() >= 8
        && text.len() <= 200
}

/// Higher score = more likely English / CTF plaintext.
pub fn score_plaintext(text: &str) -> f64 {
    if text.is_empty() {
        return f64::NEG_INFINITY;
    }

    let mut score = 0.0f64;

    if looks_like_flag(text) {
        score += 1000.0;
    }

    let printable = text
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t' || *c == '\r')
        .count();
    score += (printable as f64 / text.len() as f64) * 50.0;

    let lower = text.to_ascii_lowercase();
    for word in COMMON_WORDS {
        if lower
            .split(|c: char| !c.is_ascii_alphabetic())
            .any(|w| w == *word)
        {
            score += 8.0;
        }
    }

    let chi = frequency::chi_squared(text);
    if chi.is_finite() {
        // Lower chi-squared is better; map into a positive contribution
        score += (200.0 / (1.0 + chi)).min(80.0);
    }

    let spaces = text.chars().filter(|c| *c == ' ').count() as f64;
    let letters = text.chars().filter(|c| c.is_ascii_alphabetic()).count() as f64;
    if letters > 0.0 {
        score += (spaces / letters * 20.0).min(15.0);
    }

    let non_print = text.len() - printable;
    score -= non_print as f64 * 5.0;

    score
}

fn print_results(results: &[SolveCandidate], top: usize) {
    if results.is_empty() {
        println!("No candidates produced");
        return;
    }

    let flags: Vec<&SolveCandidate> = results.iter().filter(|c| c.is_flag).collect();
    if !flags.is_empty() {
        println!("=== Flag-like hits ===");
        for c in flags.iter().take(top) {
            println!("[{:.1}] {} => {}", c.score, c.method, c.plaintext);
        }
        println!();
    }

    println!("=== Top candidates ===");
    for c in results.iter().take(top) {
        let mark = if c.is_flag { " FLAG" } else { "" };
        println!("[{:.1}{}] {} => {}", c.score, mark, c.method, c.plaintext);
    }
}

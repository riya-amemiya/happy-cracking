use anyhow::Result;
use clap::Subcommand;
use std::collections::HashMap;
use std::sync::LazyLock;

#[derive(Subcommand)]
pub enum SubstitutionAction {
    #[command(about = "Encode with a substitution alphabet")]
    Encode {
        #[arg(help = "Input text")]
        input: String,
        #[arg(
            short,
            long,
            help = "Substitution alphabet (26 letters, e.g. ZYXWVUTSRQPONMLKJIHGFEDCBA)"
        )]
        alphabet: String,
    },
    #[command(about = "Decode with a substitution alphabet")]
    Decode {
        #[arg(help = "Encrypted text")]
        input: String,
        #[arg(short, long, help = "Substitution alphabet (26 letters)")]
        alphabet: String,
    },
    #[command(about = "Auto-solve using frequency analysis (English text)")]
    Solve {
        #[arg(help = "Encrypted text")]
        input: String,
        #[arg(
            short,
            long,
            default_value_t = 5000,
            help = "Number of hill-climbing iterations"
        )]
        iterations: usize,
    },
}

pub fn run(action: SubstitutionAction) -> Result<()> {
    match action {
        SubstitutionAction::Encode { input, alphabet } => {
            println!("{}", encode(&input, &alphabet)?);
        }
        SubstitutionAction::Decode { input, alphabet } => {
            println!("{}", decode(&input, &alphabet)?);
        }
        SubstitutionAction::Solve { input, iterations } => {
            let (key, plaintext, score) = solve(&input, iterations);
            println!("Best key: {}", key);
            println!("Score: {:.2}", score);
            println!("Plaintext: {}", plaintext);
        }
    }
    Ok(())
}

fn validate_alphabet(alphabet: &str) -> Result<[u8; 26]> {
    let upper: String = alphabet.to_uppercase();
    let chars: Vec<char> = upper.chars().collect();
    if chars.len() != 26 {
        anyhow::bail!(
            "Alphabet must be exactly 26 characters, got {}",
            chars.len()
        );
    }
    let mut seen = [false; 26];
    let mut mapping = [0u8; 26];
    for (i, c) in chars.iter().enumerate() {
        if !c.is_ascii_uppercase() {
            anyhow::bail!("Alphabet must contain only letters");
        }
        let idx = (*c as u8 - b'A') as usize;
        if seen[idx] {
            anyhow::bail!("Alphabet must not contain duplicate letters");
        }
        seen[idx] = true;
        mapping[i] = *c as u8;
    }
    Ok(mapping)
}

pub fn encode(input: &str, alphabet: &str) -> Result<String> {
    let mapping = validate_alphabet(alphabet)?;
    Ok(input
        .chars()
        .map(|c| {
            if c.is_ascii_uppercase() {
                mapping[(c as u8 - b'A') as usize] as char
            } else if c.is_ascii_lowercase() {
                (mapping[(c as u8 - b'a') as usize] - b'A' + b'a') as char
            } else {
                c
            }
        })
        .collect())
}

pub fn decode(input: &str, alphabet: &str) -> Result<String> {
    let mapping = validate_alphabet(alphabet)?;
    let mut reverse = [0u8; 26];
    for (i, &m) in mapping.iter().enumerate() {
        reverse[(m - b'A') as usize] = b'A' + i as u8;
    }
    Ok(input
        .chars()
        .map(|c| {
            if c.is_ascii_uppercase() {
                reverse[(c as u8 - b'A') as usize] as char
            } else if c.is_ascii_lowercase() {
                (reverse[(c as u8 - b'a') as usize] - b'A' + b'a') as char
            } else {
                c
            }
        })
        .collect())
}

// English letter frequencies (A-Z) used for scoring
const ENGLISH_FREQ: [f64; 26] = [
    8.2, 1.5, 2.8, 4.3, 12.7, 2.2, 2.0, 6.1, 7.0, 0.15, 0.8, 4.0, 2.4, 6.7, 7.5, 1.9, 0.10, 6.0,
    6.3, 9.1, 2.8, 1.0, 2.4, 0.15, 2.0, 0.07,
];

// English bigram log-frequencies for scoring
static BIGRAM_SCORES: LazyLock<HashMap<(u8, u8), f64>> = LazyLock::new(|| {
    let common_bigrams = [
        ("TH", 3.56),
        ("HE", 3.07),
        ("IN", 2.43),
        ("ER", 2.05),
        ("AN", 1.99),
        ("RE", 1.85),
        ("ON", 1.76),
        ("AT", 1.49),
        ("EN", 1.45),
        ("ND", 1.35),
        ("TI", 1.34),
        ("ES", 1.34),
        ("OR", 1.28),
        ("TE", 1.27),
        ("OF", 1.17),
        ("ED", 1.17),
        ("IS", 1.13),
        ("IT", 1.12),
        ("AL", 1.09),
        ("AR", 1.07),
        ("ST", 1.05),
        ("TO", 1.04),
        ("NT", 1.04),
        ("NG", 0.95),
        ("SE", 0.93),
        ("HA", 0.93),
        ("AS", 0.87),
        ("OU", 0.87),
        ("IO", 0.83),
        ("LE", 0.83),
        ("VE", 0.83),
        ("CO", 0.79),
        ("ME", 0.79),
        ("DE", 0.76),
        ("HI", 0.76),
        ("RI", 0.73),
        ("RO", 0.73),
        ("IC", 0.70),
        ("NE", 0.69),
        ("EA", 0.69),
        ("RA", 0.62),
        ("CE", 0.65),
    ];
    let mut map = HashMap::new();
    for (bg, score) in common_bigrams {
        let bytes = bg.as_bytes();
        map.insert((bytes[0], bytes[1]), score);
    }
    map
});

fn score_text(text: &str) -> f64 {
    let upper: Vec<u8> = text
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_uppercase() as u8)
        .collect();
    if upper.len() < 2 {
        return 0.0;
    }
    let mut score = 0.0;
    for window in upper.windows(2) {
        if let Some(&s) = BIGRAM_SCORES.get(&(window[0], window[1])) {
            score += s;
        }
    }
    score / (upper.len() - 1) as f64
}

fn apply_key(input: &str, key: &[u8; 26]) -> String {
    input
        .chars()
        .map(|c| {
            if c.is_ascii_uppercase() {
                key[(c as u8 - b'A') as usize] as char
            } else if c.is_ascii_lowercase() {
                (key[(c as u8 - b'a') as usize] - b'A' + b'a') as char
            } else {
                c
            }
        })
        .collect()
}

fn frequency_based_initial_key(input: &str) -> [u8; 26] {
    let mut counts = [0usize; 26];
    for c in input.chars() {
        if c.is_ascii_alphabetic() {
            counts[(c.to_ascii_uppercase() as u8 - b'A') as usize] += 1;
        }
    }

    let mut cipher_order: Vec<usize> = (0..26).collect();
    cipher_order.sort_by(|&a, &b| counts[b].cmp(&counts[a]));

    let mut english_order: Vec<usize> = (0..26).collect();
    english_order.sort_by(|a, b| ENGLISH_FREQ[*b].partial_cmp(&ENGLISH_FREQ[*a]).unwrap());

    let mut key = [0u8; 26];
    for (i, &cipher_idx) in cipher_order.iter().enumerate() {
        key[cipher_idx] = b'A' + english_order[i] as u8;
    }
    key
}

// Simple deterministic pseudo-random for reproducibility without external crates
struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }

    fn next_usize(&mut self, bound: usize) -> usize {
        (self.next_u64() % bound as u64) as usize
    }
}

pub fn solve(input: &str, iterations: usize) -> (String, String, f64) {
    let mut key = frequency_based_initial_key(input);
    let mut best_text = apply_key(input, &key);
    let mut best_score = score_text(&best_text);
    let mut best_key = key;

    let mut rng = SimpleRng::new(42);

    for _ in 0..iterations {
        let a = rng.next_usize(26);
        let b = rng.next_usize(26);
        if a == b {
            continue;
        }

        key.swap(a, b);
        let candidate = apply_key(input, &key);
        let candidate_score = score_text(&candidate);

        if candidate_score > best_score {
            best_score = candidate_score;
            best_text = candidate;
            best_key = key;
        } else {
            key.swap(a, b);
        }
    }

    let key_str: String = best_key.iter().map(|&b| b as char).collect();
    (key_str, best_text, best_score)
}

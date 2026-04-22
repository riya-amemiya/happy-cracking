use anyhow::Result;
use clap::Subcommand;
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
    let mut bytes = input.as_bytes().to_vec();
    for b in &mut bytes {
        if b.is_ascii_uppercase() {
            *b = mapping[(*b - b'A') as usize];
        } else if b.is_ascii_lowercase() {
            *b = mapping[(*b - b'a') as usize] - b'A' + b'a';
        }
    }
    // Security: use safe String::from_utf8 instead of unsafe from_utf8_unchecked to prevent
    // undefined behavior if a future refactor introduces non-ASCII byte mutations.
    Ok(String::from_utf8(bytes).expect("substitution of ASCII alphabetic bytes preserves UTF-8"))
}

pub fn decode(input: &str, alphabet: &str) -> Result<String> {
    let mapping = validate_alphabet(alphabet)?;
    let mut reverse = [0u8; 26];
    for (i, &m) in mapping.iter().enumerate() {
        reverse[(m - b'A') as usize] = b'A' + i as u8;
    }
    let mut bytes = input.as_bytes().to_vec();
    for b in &mut bytes {
        if b.is_ascii_uppercase() {
            *b = reverse[(*b - b'A') as usize];
        } else if b.is_ascii_lowercase() {
            *b = reverse[(*b - b'a') as usize] - b'A' + b'a';
        }
    }
    // Security: use safe String::from_utf8 instead of unsafe from_utf8_unchecked to prevent
    // undefined behavior if a future refactor introduces non-ASCII byte mutations.
    Ok(String::from_utf8(bytes).expect("substitution of ASCII alphabetic bytes preserves UTF-8"))
}

// English letter frequencies (A-Z) used for scoring
const ENGLISH_FREQ: [f64; 26] = [
    8.2, 1.5, 2.8, 4.3, 12.7, 2.2, 2.0, 6.1, 7.0, 0.15, 0.8, 4.0, 2.4, 6.7, 7.5, 1.9, 0.10, 6.0,
    6.3, 9.1, 2.8, 1.0, 2.4, 0.15, 2.0, 0.07,
];

// English bigram log-frequencies for scoring. Stored as a dense [[f64; 26]; 26]
// table keyed by (first_letter - b'A', second_letter - b'A') so the hot path in
// `solve` can look up scores with two integer indexes instead of hashing a
// (u8, u8) tuple. Missing bigrams stay 0.0 which matches the old HashMap
// "no entry = no score contribution" behavior.
static BIGRAM_TABLE: LazyLock<[[f64; 26]; 26]> = LazyLock::new(|| {
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
    let mut table = [[0.0f64; 26]; 26];
    for (bg, score) in common_bigrams {
        let bytes = bg.as_bytes();
        let a = (bytes[0] - b'A') as usize;
        let b = (bytes[1] - b'A') as usize;
        table[a][b] = score;
    }
    table
});

// Compact representation of `input`: one u8 per alphabetic character in the
// range 0..=25 (uppercased). Non-alphabetic characters are dropped. This is
// computed once per `solve` call and reused across every iteration of the
// hill climb.
fn extract_alpha_indices(input: &str) -> Vec<u8> {
    input
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_uppercase() as u8 - b'A')
        .collect()
}

// Average bigram score of `input` after the `key` permutation is applied,
// computed directly from the precomputed `indices` without materializing the
// decrypted plaintext. `key[i]` is an uppercase ASCII byte in b'A'..=b'Z',
// so `key[i] - b'A'` maps cleanly to a BIGRAM_TABLE row/column index.
fn score_with_key(indices: &[u8], key: &[u8; 26], table: &[[f64; 26]; 26]) -> f64 {
    if indices.len() < 2 {
        return 0.0;
    }
    let mut score = 0.0;
    for window in indices.windows(2) {
        let a = (key[window[0] as usize] - b'A') as usize;
        let b = (key[window[1] as usize] - b'A') as usize;
        score += table[a][b];
    }
    score / (indices.len() - 1) as f64
}

fn apply_key(input: &str, key: &[u8; 26]) -> String {
    let mut bytes = input.as_bytes().to_vec();
    for b in &mut bytes {
        if b.is_ascii_uppercase() {
            *b = key[(*b - b'A') as usize];
        } else if b.is_ascii_lowercase() {
            *b = key[(*b - b'a') as usize] - b'A' + b'a';
        }
    }
    // Security: use safe String::from_utf8 instead of unsafe from_utf8_unchecked to prevent
    // undefined behavior if a future refactor introduces non-ASCII byte mutations.
    String::from_utf8(bytes).expect("substitution of ASCII alphabetic bytes preserves UTF-8")
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
    // Performance: preprocess the input once into compact 0..=25 indices and
    // cache a reference to the bigram table so the hot loop below is a pure
    // numeric scan with no allocations or hashing.
    let indices = extract_alpha_indices(input);
    let table = &*BIGRAM_TABLE;

    let mut key = frequency_based_initial_key(input);
    let mut best_score = score_with_key(&indices, &key, table);
    let mut best_key = key;

    let mut rng = SimpleRng::new(42);

    for _ in 0..iterations {
        let a = rng.next_usize(26);
        let b = rng.next_usize(26);
        if a == b {
            continue;
        }

        key.swap(a, b);
        let candidate_score = score_with_key(&indices, &key, table);

        if candidate_score > best_score {
            best_score = candidate_score;
            best_key = key;
        } else {
            key.swap(a, b);
        }
    }

    // Rebuild the decrypted plaintext once, outside the hot loop. The old
    // implementation did this on every improving iteration.
    let best_text = apply_key(input, &best_key);
    let key_str: String = best_key.iter().map(|&b| b as char).collect();
    (key_str, best_text, best_score)
}

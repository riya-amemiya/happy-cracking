use anyhow::Result;
use clap::Subcommand;

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
            let (key, plaintext, score) = solve(&input, iterations)?;
            println!("Best key: {key}");
            println!("Score: {score:.2}");
            println!("Plaintext: {plaintext}");
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
    Ok(String::from_utf8(bytes).expect("substitution of ASCII alphabetic bytes preserves UTF-8"))
}

// English letter frequencies (A-Z) used for scoring
const ENGLISH_FREQ: [f64; 26] = [
    8.2, 1.5, 2.8, 4.3, 12.7, 2.2, 2.0, 6.1, 7.0, 0.15, 0.8, 4.0, 2.4, 6.7, 7.5, 1.9, 0.10, 6.0,
    6.3, 9.1, 2.8, 1.0, 2.4, 0.15, 2.0, 0.07,
];

const BIGRAM_HUNDREDTHS: [([u8; 2], i32); 42] = [
    (*b"TH", 356),
    (*b"HE", 307),
    (*b"IN", 243),
    (*b"ER", 205),
    (*b"AN", 199),
    (*b"RE", 185),
    (*b"ON", 176),
    (*b"AT", 149),
    (*b"EN", 145),
    (*b"ND", 135),
    (*b"TI", 134),
    (*b"ES", 134),
    (*b"OR", 128),
    (*b"TE", 127),
    (*b"OF", 117),
    (*b"ED", 117),
    (*b"IS", 113),
    (*b"IT", 112),
    (*b"AL", 109),
    (*b"AR", 107),
    (*b"ST", 105),
    (*b"TO", 104),
    (*b"NT", 104),
    (*b"NG", 95),
    (*b"SE", 93),
    (*b"HA", 93),
    (*b"AS", 87),
    (*b"OU", 87),
    (*b"IO", 83),
    (*b"LE", 83),
    (*b"VE", 83),
    (*b"CO", 79),
    (*b"ME", 79),
    (*b"DE", 76),
    (*b"HI", 76),
    (*b"RI", 73),
    (*b"RO", 73),
    (*b"IC", 70),
    (*b"NE", 69),
    (*b"EA", 69),
    (*b"RA", 62),
    (*b"CE", 65),
];

const BIGRAM_TABLE: [[i32; 26]; 26] = {
    let mut table = [[0i32; 26]; 26];
    let mut i = 0;
    while i < BIGRAM_HUNDREDTHS.len() {
        let (bg, score) = BIGRAM_HUNDREDTHS[i];
        table[(bg[0] - b'A') as usize][(bg[1] - b'A') as usize] = score;
        i += 1;
    }
    table
};

fn extract_alpha_indices(input: &str) -> Vec<u8> {
    input
        .bytes()
        .filter(u8::is_ascii_alphabetic)
        .map(|c| c.to_ascii_uppercase() - b'A')
        .collect()
}

fn mapped_plain(key: &[u8; 26], cipher: u8, sa: usize, sb: usize) -> usize {
    let idx = cipher as usize;
    if idx == sa {
        key[sb] as usize
    } else if idx == sb {
        key[sa] as usize
    } else {
        key[idx] as usize
    }
}

fn window_delta(indices: &[u8], key: &[u8; 26], sa: usize, sb: usize, w: usize) -> i32 {
    let c1 = indices[w];
    let c2 = indices[w + 1];
    let old = BIGRAM_TABLE[key[c1 as usize] as usize][key[c2 as usize] as usize];
    let new = BIGRAM_TABLE[mapped_plain(key, c1, sa, sb)][mapped_plain(key, c2, sa, sb)];
    new - old
}

fn apply_key(input: &str, key: &[u8; 26]) -> String {
    let mut bytes = input.as_bytes().to_vec();
    for b in &mut bytes {
        if b.is_ascii_uppercase() {
            *b = b'A' + key[(*b - b'A') as usize];
        } else if b.is_ascii_lowercase() {
            *b = b'a' + key[(*b - b'a') as usize];
        }
    }
    String::from_utf8(bytes).expect("substitution of ASCII alphabetic bytes preserves UTF-8")
}

fn frequency_based_initial_key(input: &str) -> [u8; 26] {
    let mut counts = [0usize; 26];
    for c in input.bytes() {
        if c.is_ascii_alphabetic() {
            counts[(c.to_ascii_uppercase() - b'A') as usize] += 1;
        }
    }

    let mut cipher_order: Vec<usize> = (0..26).collect();
    cipher_order.sort_by(|&a, &b| counts[b].cmp(&counts[a]));

    let mut english_order: Vec<usize> = (0..26).collect();
    english_order.sort_by(|a, b| ENGLISH_FREQ[*b].partial_cmp(&ENGLISH_FREQ[*a]).unwrap());

    let mut key = [0u8; 26];
    for (i, &cipher_idx) in cipher_order.iter().enumerate() {
        key[cipher_idx] = english_order[i] as u8;
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

/// Maximum hill-climbing iterations for substitution auto-solve.
///
/// SECURITY: `--iterations` / `--substitution-iters` are user-controlled loop
/// bounds. Unbounded values let a caller hang the process (CPU exhaustion `DoS`).
pub const MAX_SOLVE_ITERATIONS: usize = 1_000_000;

pub fn check_solve_iterations(iterations: usize) -> Result<()> {
    if iterations > MAX_SOLVE_ITERATIONS {
        anyhow::bail!(
            "Hill-climbing iterations {iterations} exceeds the maximum allowed limit of {MAX_SOLVE_ITERATIONS} to prevent Denial of Service"
        );
    }
    Ok(())
}

pub fn solve(input: &str, iterations: usize) -> Result<(String, String, f64)> {
    check_solve_iterations(iterations)?;

    let indices = extract_alpha_indices(input);
    let mut pos: [Vec<usize>; 26] = std::array::from_fn(|_| Vec::new());
    for (i, &idx) in indices.iter().enumerate() {
        pos[idx as usize].push(i);
    }

    let mut key = frequency_based_initial_key(input);
    let mut score = 0i64;
    for window in indices.windows(2) {
        score += i64::from(
            BIGRAM_TABLE[key[window[0] as usize] as usize][key[window[1] as usize] as usize],
        );
    }
    let mut rng = SimpleRng::new(42);
    let n = indices.len();
    let mut stamp = vec![0u32; n.saturating_sub(1)];
    let mut epoch = 1u32;

    for _ in 0..iterations {
        let sa = rng.next_usize(26);
        let sb = rng.next_usize(26);
        if sa == sb {
            continue;
        }
        epoch = epoch.wrapping_add(1);
        if epoch == 0 {
            stamp.fill(0);
            epoch = 1;
        }
        let mut delta = 0i64;
        for &i in pos[sa].iter().chain(pos[sb].iter()) {
            if i > 0 {
                let w = i - 1;
                if stamp[w] != epoch {
                    stamp[w] = epoch;
                    delta += i64::from(window_delta(&indices, &key, sa, sb, w));
                }
            }
            if i + 1 < n && stamp[i] != epoch {
                stamp[i] = epoch;
                delta += i64::from(window_delta(&indices, &key, sa, sb, i));
            }
        }
        if delta > 0 {
            key.swap(sa, sb);
            score += delta;
        }
    }

    let display = if n < 2 {
        0.0
    } else {
        score as f64 / 100.0 / (n - 1) as f64
    };
    let best_text = apply_key(input, &key);
    let key_str: String = key.iter().map(|&p| (b'A' + p) as char).collect();
    Ok((key_str, best_text, display))
}

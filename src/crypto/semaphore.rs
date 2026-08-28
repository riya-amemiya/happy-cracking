use anyhow::{Context, Result};
use clap::Subcommand;

#[derive(Subcommand)]
pub enum SemaphoreAction {
    #[command(about = "Encode text to flag semaphore positions")]
    Encode {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Decode flag semaphore positions to text")]
    Decode {
        #[arg(help = "Semaphore encoded string (e.g. 1-2 1-3 1-4)")]
        input: String,
    },
}

pub fn run(action: SemaphoreAction) -> Result<()> {
    match action {
        SemaphoreAction::Encode { input } => {
            println!("{}", encode(&input)?);
        }
        SemaphoreAction::Decode { input } => {
            println!("{}", decode(&input)?);
        }
    }
    Ok(())
}

// Positions: 1=S, 2=SW, 3=W, 4=NW, 5=N, 6=NE, 7=E, 8=SE
const SEMAPHORE_MAP: &[(char, &str)] = &[
    ('A', "1-2"),
    ('B', "1-3"),
    ('C', "1-4"),
    ('D', "1-5"),
    ('E', "1-6"),
    ('F', "1-7"),
    ('G', "1-8"),
    ('H', "2-3"),
    ('I', "2-4"),
    ('J', "5-6"),
    ('K', "3-1"),
    ('L', "3-2"),
    ('M', "3-4"),
    ('N', "3-5"),
    ('O', "3-6"),
    ('P', "3-7"),
    ('Q', "3-8"),
    ('R', "4-3"),
    ('S', "4-5"),
    ('T', "4-6"),
    ('U', "4-7"),
    ('V', "5-1"),
    ('W', "6-2"),
    ('X', "6-3"),
    ('Y', "6-4"),
    ('Z', "6-5"),
];

const ENCODE_LUT: [Option<&str>; 26] = {
    let mut table: [Option<&str>; 26] = [None; 26];
    let mut i = 0;
    while i < SEMAPHORE_MAP.len() {
        let (ch, code) = SEMAPHORE_MAP[i];
        table[ch as usize - 'A' as usize] = Some(code);
        i += 1;
    }
    table
};

/// Digit-pair lookup for semaphore decode.
///
/// Every flag pair is `"X-Y"` with digits 1–8 (order matters: `1-3` is B, `3-1`
/// is K). Indexing `[first][second]` replaces HashMap hashing of the 3-byte
/// token with a pair of array loads. Slot 0 is unused (digits are 1-based).
const DECODE_LUT: [[Option<char>; 9]; 9] = {
    let mut table = [[None; 9]; 9];
    let mut i = 0;
    while i < SEMAPHORE_MAP.len() {
        let (ch, code) = SEMAPHORE_MAP[i];
        let b = code.as_bytes();
        table[(b[0] - b'0') as usize][(b[2] - b'0') as usize] = Some(ch);
        i += 1;
    }
    table
};

const fn decode_lut_len(table: &[[Option<char>; 9]; 9]) -> usize {
    let mut n = 0;
    let mut r = 0;
    while r < 9 {
        let mut c = 0;
        while c < 9 {
            if table[r][c].is_some() {
                n += 1;
            }
            c += 1;
        }
        r += 1;
    }
    n
}

const _: () = assert!(decode_lut_len(&DECODE_LUT) == SEMAPHORE_MAP.len());

fn lookup_semaphore(code: &str) -> Option<char> {
    let b = code.as_bytes();
    if b.len() != 3 || b[1] != b'-' {
        return None;
    }
    let r = b[0].wrapping_sub(b'0') as usize;
    let c = b[2].wrapping_sub(b'0') as usize;
    if r < 9 && c < 9 {
        DECODE_LUT[r][c]
    } else {
        None
    }
}

pub fn encode(input: &str) -> Result<String> {
    if input.is_empty() {
        return Ok(String::new());
    }

    // Each semaphore code is 3 chars ("X-Y") + 1 space separator
    let mut result = String::with_capacity(input.len() * 4);
    let mut has_content = false;

    for b in input.bytes() {
        let upper = match b {
            b'a'..=b'z' => b - b'a',
            b'A'..=b'Z' => b - b'A',
            _ => continue,
        };
        if has_content {
            result.push(' ');
        }
        result.push_str(ENCODE_LUT[upper as usize].ok_or_else(|| {
            anyhow::anyhow!("No semaphore code for character: {}", upper as char)
        })?);
        has_content = true;
    }

    Ok(result)
}

pub fn decode(input: &str) -> Result<String> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(String::new());
    }

    // Each code is 3 chars ("X-Y") plus a space; letters are 1-byte ASCII.
    let mut result = String::with_capacity((input.len() + 1) / 4);
    for code in input.split_whitespace() {
        let ch =
            lookup_semaphore(code).with_context(|| format!("Unknown semaphore code: {}", code))?;
        result.push(ch);
    }
    Ok(result)
}

use anyhow::Result;
use clap::Subcommand;

use super::polybius_utils::{build_polybius_square, build_reverse_lookup, find_in_square_fast};

#[derive(Subcommand)]
pub enum PlayfairAction {
    #[command(about = "Encrypt with Playfair cipher")]
    Encrypt {
        #[arg(help = "Input text")]
        input: String,
        #[arg(short, long, help = "Keyword for generating the 5x5 matrix")]
        key: String,
    },
    #[command(about = "Decrypt Playfair cipher")]
    Decrypt {
        #[arg(help = "Encrypted text")]
        input: String,
        #[arg(short, long, help = "Keyword for generating the 5x5 matrix")]
        key: String,
    },
}

pub fn run(action: PlayfairAction) -> Result<()> {
    match action {
        PlayfairAction::Encrypt { input, key } => {
            println!("{}", encrypt(&input, &key)?);
        }
        PlayfairAction::Decrypt { input, key } => {
            println!("{}", decrypt(&input, &key)?);
        }
    }
    Ok(())
}

fn validate_key(key: &str) -> Result<()> {
    if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphabetic()) {
        anyhow::bail!("Key must be non-empty and contain only alphabetic characters");
    }
    Ok(())
}

fn normalize_letter(b: u8) -> u8 {
    let mut c = b.to_ascii_uppercase();
    if c == b'J' {
        c = b'I';
    }
    c
}

fn pair_letters(
    matrix: &[char],
    reverse: &[u8; 128],
    a: char,
    b: char,
    shift: usize,
) -> Result<(char, char)> {
    let (ra, ca) = find_in_square_fast(reverse, a)
        .ok_or_else(|| anyhow::anyhow!("Character {a} not in square"))?;
    let (rb, cb) = find_in_square_fast(reverse, b)
        .ok_or_else(|| anyhow::anyhow!("Character {b} not in square"))?;

    if ra == rb {
        Ok((
            matrix[ra * 5 + (ca + shift) % 5],
            matrix[rb * 5 + (cb + shift) % 5],
        ))
    } else if ca == cb {
        Ok((
            matrix[((ra + shift) % 5) * 5 + ca],
            matrix[((rb + shift) % 5) * 5 + cb],
        ))
    } else {
        Ok((matrix[ra * 5 + cb], matrix[rb * 5 + ca]))
    }
}

fn push_pair(
    matrix: &[char],
    reverse: &[u8; 128],
    out: &mut Vec<u8>,
    a: u8,
    b: u8,
    shift: usize,
) -> Result<()> {
    let (oa, ob) = pair_letters(matrix, reverse, a as char, b as char, shift)?;
    out.push(oa as u8);
    out.push(ob as u8);
    Ok(())
}

fn encrypt_ascii(input: &[u8], key: &str) -> Result<String> {
    let matrix = build_polybius_square(key);
    let reverse = build_reverse_lookup(&matrix);
    let mut result = Vec::with_capacity(input.len().saturating_mul(2));
    let mut pending = None;

    for &b in input {
        if !b.is_ascii_alphabetic() {
            continue;
        }
        let c = normalize_letter(b);
        match pending {
            None => pending = Some(c),
            Some(a) if a == c => {
                push_pair(&matrix, &reverse, &mut result, a, b'X', 1)?;
                pending = Some(c);
            }
            Some(a) => {
                push_pair(&matrix, &reverse, &mut result, a, c, 1)?;
                pending = None;
            }
        }
    }
    if let Some(a) = pending {
        push_pair(&matrix, &reverse, &mut result, a, b'X', 1)?;
    }

    Ok(String::from_utf8(result).expect("playfair encrypt of ASCII letters stays ASCII"))
}

fn decrypt_ascii(input: &[u8], key: &str) -> Result<String> {
    let matrix = build_polybius_square(key);
    let reverse = build_reverse_lookup(&matrix);
    let mut result = Vec::with_capacity(input.len());
    let mut pending = None;

    for &b in input {
        if !b.is_ascii_alphabetic() {
            continue;
        }
        let c = normalize_letter(b);
        match pending {
            None => pending = Some(c),
            Some(a) => {
                push_pair(&matrix, &reverse, &mut result, a, c, 4)?;
                pending = None;
            }
        }
    }

    if pending.is_some() {
        anyhow::bail!("Encrypted text must have even length");
    }

    Ok(String::from_utf8(result).expect("playfair decrypt of ASCII letters stays ASCII"))
}

fn prepare_digraphs(input: &str) -> Vec<(char, char)> {
    let letters: Vec<char> = input
        .to_uppercase()
        .chars()
        .filter(char::is_ascii_uppercase)
        .map(|c| if c == 'J' { 'I' } else { c })
        .collect();

    let mut digraphs = Vec::new();
    let mut i = 0;
    while i < letters.len() {
        let a = letters[i];
        if i + 1 < letters.len() {
            let b = letters[i + 1];
            if a == b {
                digraphs.push((a, 'X'));
                i += 1;
            } else {
                digraphs.push((a, b));
                i += 2;
            }
        } else {
            digraphs.push((a, 'X'));
            i += 1;
        }
    }

    digraphs
}

fn encrypt_chars(input: &str, key: &str) -> Result<String> {
    let filtered: String = input.chars().filter(char::is_ascii_alphabetic).collect();
    if filtered.is_empty() {
        return Ok(String::new());
    }

    let matrix = build_polybius_square(key);
    let reverse = build_reverse_lookup(&matrix);
    let digraphs = prepare_digraphs(input);

    let mut result = String::with_capacity(digraphs.len().saturating_mul(2));
    for (a, b) in digraphs {
        let (oa, ob) = pair_letters(&matrix, &reverse, a, b, 1)?;
        result.push(oa);
        result.push(ob);
    }

    Ok(result)
}

fn decrypt_chars(input: &str, key: &str) -> Result<String> {
    let letters: Vec<char> = input
        .to_uppercase()
        .chars()
        .filter(char::is_ascii_uppercase)
        .map(|c| if c == 'J' { 'I' } else { c })
        .collect();

    if letters.is_empty() {
        return Ok(String::new());
    }

    if !letters.len().is_multiple_of(2) {
        anyhow::bail!("Encrypted text must have even length");
    }

    let matrix = build_polybius_square(key);
    let reverse = build_reverse_lookup(&matrix);

    let mut result = String::with_capacity(letters.len());
    for pair in letters.chunks(2) {
        let (oa, ob) = pair_letters(&matrix, &reverse, pair[0], pair[1], 4)?;
        result.push(oa);
        result.push(ob);
    }

    Ok(result)
}

pub fn encrypt(input: &str, key: &str) -> Result<String> {
    validate_key(key)?;
    if input.is_ascii() {
        encrypt_ascii(input.as_bytes(), key)
    } else {
        encrypt_chars(input, key)
    }
}

pub fn decrypt(input: &str, key: &str) -> Result<String> {
    validate_key(key)?;
    if input.is_ascii() {
        decrypt_ascii(input.as_bytes(), key)
    } else {
        decrypt_chars(input, key)
    }
}

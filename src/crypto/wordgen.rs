use anyhow::{Context, Result};
use clap::Subcommand;
use rand::RngExt;
use std::collections::HashSet;
use std::io::{self, BufWriter, Write};

pub const MAX_CANDIDATES: u128 = 1_000_000_000;
pub const MAX_CHARSET_LEN: usize = 256;
pub const MAX_OUTPUT_LEN: usize = 4_096;
pub const DEFAULT_CHARSET: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

#[derive(Subcommand)]
pub enum WordgenAction {
    #[command(about = "Generate random strings")]
    Random {
        #[arg(long, help = "Length of each generated string")]
        length: usize,
        #[arg(short = 'n', long, default_value_t = 1, help = "Number of strings")]
        count: u64,
        #[arg(
            short,
            long,
            default_value = DEFAULT_CHARSET,
            help = "Characters to sample"
        )]
        charset: String,
        #[arg(long, help = "Allow more than 1,000,000,000 outputs")]
        force: bool,
    },
    #[command(about = "Generate every combination from a character set")]
    Enumerate {
        #[arg(short, long, help = "Characters to combine")]
        charset: String,
        #[arg(long, default_value_t = 1, help = "Minimum output length")]
        min_len: usize,
        #[arg(long, default_value_t = 4, help = "Maximum output length")]
        max_len: usize,
        #[arg(long, help = "Allow more than 1,000,000,000 outputs")]
        force: bool,
    },
    #[command(about = "Generate strings from fixed text and ?c positions")]
    Mask {
        #[arg(help = "Mask containing fixed text, ?c variables, and ?? escapes")]
        mask: String,
        #[arg(short, long, help = "Characters for each ?c position")]
        charset: String,
        #[arg(long, help = "Allow more than 1,000,000,000 outputs")]
        force: bool,
    },
}

pub fn run(action: WordgenAction) -> Result<()> {
    let stdout = io::stdout();
    let mut writer = BufWriter::with_capacity(64 * 1024, stdout.lock());
    let result = match action {
        WordgenAction::Random {
            length,
            count,
            charset,
            force,
        } => write_random(&mut writer, &charset, length, count, force),
        WordgenAction::Enumerate {
            charset,
            min_len,
            max_len,
            force,
        } => write_enumerated(&mut writer, &charset, min_len, max_len, force),
        WordgenAction::Mask {
            mask,
            charset,
            force,
        } => write_masked(&mut writer, &mask, &charset, force),
    }
    .and_then(|()| {
        writer.flush()?;
        Ok(())
    });

    match result {
        Err(error)
            if error
                .downcast_ref::<io::Error>()
                .is_some_and(|source| source.kind() == io::ErrorKind::BrokenPipe) =>
        {
            Ok(())
        }
        other => other,
    }
}

fn normalize_charset(charset: &str) -> Result<Vec<char>> {
    let mut seen = HashSet::new();
    let chars: Vec<char> = charset
        .chars()
        .filter(|value| seen.insert(*value))
        .collect();

    if chars.is_empty() {
        anyhow::bail!("--charset must not be empty");
    }
    if chars.len() > MAX_CHARSET_LEN {
        anyhow::bail!("--charset contains more than {MAX_CHARSET_LEN} unique characters");
    }
    Ok(chars)
}

fn validate_length(length: usize) -> Result<()> {
    if length == 0 {
        anyhow::bail!("Length must be at least 1");
    }
    if length > MAX_OUTPUT_LEN {
        anyhow::bail!("Length {length} exceeds the limit of {MAX_OUTPUT_LEN}");
    }
    Ok(())
}

fn validate_length_range(min_len: usize, max_len: usize) -> Result<()> {
    validate_length(min_len)?;
    validate_length(max_len)?;
    if max_len < min_len {
        anyhow::bail!("--max-len ({max_len}) must be >= --min-len ({min_len})");
    }
    Ok(())
}

fn validate_candidate_count(total: u128, force: bool) -> Result<()> {
    if !force && total > MAX_CANDIDATES {
        anyhow::bail!(
            "Generation requires {total} candidates, exceeding the limit of 1,000,000,000; pass --force to continue"
        );
    }
    Ok(())
}

fn increment_digits(digits: &mut [usize], base: usize) {
    for digit in digits.iter_mut().rev() {
        *digit += 1;
        if *digit < base {
            return;
        }
        *digit = 0;
    }
}

enum MaskPosition {
    Literal(char),
    Variable,
}

fn parse_mask(mask: &str) -> Result<Vec<MaskPosition>> {
    if mask.is_empty() {
        anyhow::bail!("Mask must not be empty");
    }

    let mut chars = mask.chars();
    let mut positions = Vec::new();
    while let Some(value) = chars.next() {
        if value != '?' {
            positions.push(MaskPosition::Literal(value));
            continue;
        }

        match chars.next() {
            Some('c') => positions.push(MaskPosition::Variable),
            Some('?') => positions.push(MaskPosition::Literal('?')),
            Some(token) => anyhow::bail!("Unknown mask token '?{token}'"),
            None => anyhow::bail!("Mask ends with a dangling '?'"),
        }
    }

    if positions.len() > MAX_OUTPUT_LEN {
        anyhow::bail!(
            "Mask output length {} exceeds the limit of {MAX_OUTPUT_LEN}",
            positions.len()
        );
    }
    Ok(positions)
}

pub fn write_enumerated<W: Write>(
    writer: &mut W,
    charset: &str,
    min_len: usize,
    max_len: usize,
    force: bool,
) -> Result<()> {
    let chars = normalize_charset(charset)?;
    validate_length_range(min_len, max_len)?;

    let base = chars.len() as u128;
    let total = (min_len..=max_len).try_fold(0u128, |sum, len| {
        let count = base
            .checked_pow(len as u32)
            .context("Candidate count overflowed")?;
        sum.checked_add(count).context("Candidate count overflowed")
    })?;
    validate_candidate_count(total, force)?;

    let mut candidate = String::with_capacity(max_len.saturating_mul(4));
    for len in min_len..=max_len {
        let count = base.pow(len as u32);
        let mut digits = vec![0usize; len];
        for _ in 0..count {
            candidate.clear();
            candidate.extend(digits.iter().map(|&digit| chars[digit]));
            writer.write_all(candidate.as_bytes())?;
            writer.write_all(b"\n")?;
            increment_digits(&mut digits, chars.len());
        }
    }
    Ok(())
}

pub fn write_masked<W: Write>(
    writer: &mut W,
    mask: &str,
    charset: &str,
    force: bool,
) -> Result<()> {
    let chars = normalize_charset(charset)?;
    let positions = parse_mask(mask)?;
    let variable_count = positions
        .iter()
        .filter(|position| matches!(position, MaskPosition::Variable))
        .count();
    let total = (chars.len() as u128)
        .checked_pow(variable_count as u32)
        .context("Candidate count overflowed")?;
    validate_candidate_count(total, force)?;

    let mut digits = vec![0usize; variable_count];
    let mut candidate = String::with_capacity(positions.len().saturating_mul(4));
    for _ in 0..total {
        candidate.clear();
        let mut variable = 0;
        for position in &positions {
            match position {
                MaskPosition::Literal(value) => candidate.push(*value),
                MaskPosition::Variable => {
                    candidate.push(chars[digits[variable]]);
                    variable += 1;
                }
            }
        }
        writer.write_all(candidate.as_bytes())?;
        writer.write_all(b"\n")?;
        increment_digits(&mut digits, chars.len());
    }
    Ok(())
}

pub fn write_random<W: Write>(
    writer: &mut W,
    charset: &str,
    length: usize,
    count: u64,
    force: bool,
) -> Result<()> {
    let chars = normalize_charset(charset)?;
    validate_length(length)?;
    if count == 0 {
        anyhow::bail!("--count must be at least 1");
    }
    validate_candidate_count(u128::from(count), force)?;

    let mut rng = rand::rng();
    let mut candidate = String::with_capacity(length.saturating_mul(4));
    for _ in 0..count {
        candidate.clear();
        for _ in 0..length {
            candidate.push(chars[rng.random_range(0..chars.len())]);
        }
        writer.write_all(candidate.as_bytes())?;
        writer.write_all(b"\n")?;
    }
    Ok(())
}

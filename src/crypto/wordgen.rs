use anyhow::{Context, Result};
use std::collections::HashSet;
use std::io::Write;

pub const MAX_CANDIDATES: u128 = 1_000_000_000;
pub const MAX_CHARSET_LEN: usize = 256;
pub const MAX_OUTPUT_LEN: usize = 4_096;

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
        sum.checked_add(count)
            .context("Candidate count overflowed")
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

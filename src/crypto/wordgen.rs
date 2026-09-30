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
    #[command(about = "Generate strings from a mask with ?d/?l/?u/?w/?a classes and ?c positions")]
    Mask {
        #[arg(
            help = "Mask with literals, ?d/?l/?u/?w/?a classes, ?c custom positions, and ?? or \\? for a literal ?"
        )]
        mask: String,
        #[arg(
            short,
            long,
            default_value = "",
            help = "Characters for ?c positions (required only when the mask uses ?c)"
        )]
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

pub(crate) fn charset_class(token: char) -> Option<Vec<char>> {
    let literal = match token {
        'd' => "0123456789",
        'l' => "abcdefghijklmnopqrstuvwxyz",
        'u' => "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
        'w' => "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_",
        'a' => return Some((0x20u8..=0x7eu8).map(|byte| byte as char).collect()),
        _ => return None,
    };
    Some(literal.chars().collect())
}

fn expand_charset_tokens(charset: &str) -> Result<Vec<char>> {
    let mut expanded = Vec::new();
    let mut chars = charset.chars();
    while let Some(value) = chars.next() {
        match value {
            '\\' => {
                let Some(escaped) = chars.next() else {
                    anyhow::bail!("--charset ends with a dangling '\\'");
                };
                if matches!(escaped, '\n' | '\r') {
                    anyhow::bail!("--charset must not contain a line break");
                }
                expanded.push(escaped);
            }
            '?' => match chars.next() {
                Some('?') => expanded.push('?'),
                Some(token) => {
                    let Some(class) = charset_class(token) else {
                        anyhow::bail!("Unknown charset token '?{token}'");
                    };
                    expanded.extend(class);
                }
                None => anyhow::bail!("--charset ends with a dangling '?'"),
            },
            '\n' | '\r' => anyhow::bail!("--charset must not contain a line break"),
            other => expanded.push(other),
        }
    }
    Ok(expanded)
}

pub(crate) fn normalize_charset(charset: &str) -> Result<Vec<char>> {
    let mut seen = HashSet::new();
    let mut chars = Vec::new();
    for value in expand_charset_tokens(charset)? {
        if !seen.insert(value) {
            continue;
        }
        if chars.len() == MAX_CHARSET_LEN {
            anyhow::bail!("--charset contains more than {MAX_CHARSET_LEN} unique characters");
        }
        chars.push(value);
    }

    if chars.is_empty() {
        anyhow::bail!("--charset must not be empty");
    }
    Ok(chars)
}

fn validate_length(option: &str, length: usize) -> Result<()> {
    if length == 0 {
        anyhow::bail!("{option} must be at least 1");
    }
    if length > MAX_OUTPUT_LEN {
        anyhow::bail!("{option} ({length}) exceeds the limit of {MAX_OUTPUT_LEN}");
    }
    Ok(())
}

fn validate_length_range(min_len: usize, max_len: usize) -> Result<()> {
    validate_length("--min-len", min_len)?;
    validate_length("--max-len", max_len)?;
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

pub(crate) fn combination_count(base: usize, len: usize) -> Result<u128> {
    let exponent = u32::try_from(len).context("Candidate count overflowed")?;
    (base as u128)
        .checked_pow(exponent)
        .context("Candidate count overflowed")
}

pub(crate) fn enumerated_total(chars: &[char], min_len: usize, max_len: usize) -> Result<u128> {
    (min_len..=max_len).try_fold(0u128, |sum, len| {
        let count = combination_count(chars.len(), len)?;
        sum.checked_add(count).context("Candidate count overflowed")
    })
}

pub(crate) fn candidate_from_index(chars: &[char], len: usize, index: u128) -> String {
    let base = chars.len() as u128;
    let mut value = index;
    let mut out = vec![chars[0]; len];
    for slot in out.iter_mut().rev() {
        *slot = chars[(value % base) as usize];
        value /= base;
    }
    out.into_iter().collect()
}

pub(crate) fn append_random(
    out: &mut String,
    chars: &[char],
    length: usize,
    rng: &mut impl RngExt,
) {
    out.clear();
    for _ in 0..length {
        out.push(chars[rng.random_range(0..chars.len())]);
    }
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

fn increment_mixed(digits: &mut [usize], bases: &[usize]) {
    for (digit, base) in digits.iter_mut().zip(bases).rev() {
        *digit += 1;
        if *digit < *base {
            return;
        }
        *digit = 0;
    }
}

enum MaskPosition {
    Literal(char),
    Variable(Vec<char>),
}

pub(crate) struct MaskPlan {
    positions: Vec<MaskPosition>,
}

impl MaskPlan {
    pub(crate) fn output_len(&self) -> usize {
        self.positions.len()
    }

    pub(crate) fn total(&self) -> Result<u128> {
        self.positions
            .iter()
            .try_fold(1u128, |total, position| match position {
                MaskPosition::Literal(_) => Ok(total),
                MaskPosition::Variable(chars) => total
                    .checked_mul(chars.len() as u128)
                    .context("Candidate count overflowed"),
            })
    }

    fn variable_bases(&self) -> Vec<usize> {
        self.positions
            .iter()
            .filter_map(|position| match position {
                MaskPosition::Variable(chars) => Some(chars.len()),
                MaskPosition::Literal(_) => None,
            })
            .collect()
    }

    pub(crate) fn candidate(&self, index: u128) -> String {
        let mut value = index;
        let mut digits = Vec::new();
        for position in self.positions.iter().rev() {
            if let MaskPosition::Variable(chars) = position {
                let base = chars.len() as u128;
                digits.push((value % base) as usize);
                value /= base;
            }
        }
        digits.reverse();

        let mut candidate = String::with_capacity(self.positions.len());
        let mut variable = 0;
        for position in &self.positions {
            match position {
                MaskPosition::Literal(value) => candidate.push(*value),
                MaskPosition::Variable(chars) => {
                    candidate.push(chars[digits[variable]]);
                    variable += 1;
                }
            }
        }
        candidate
    }
}

pub(crate) fn mask_plan(mask: &str, charset: &str) -> Result<MaskPlan> {
    let custom = if charset.is_empty() {
        Vec::new()
    } else {
        normalize_charset(charset)?
    };
    let positions = parse_mask(mask, &custom)?;
    Ok(MaskPlan { positions })
}

fn parse_mask(mask: &str, custom: &[char]) -> Result<Vec<MaskPosition>> {
    if mask.is_empty() {
        anyhow::bail!("Mask must not be empty");
    }

    let mut chars = mask.chars();
    let mut positions = Vec::new();
    while let Some(value) = chars.next() {
        let position = match value {
            '\\' => {
                let Some(escaped) = chars.next() else {
                    anyhow::bail!("Mask ends with a dangling '\\'");
                };
                if matches!(escaped, '\n' | '\r') {
                    anyhow::bail!("Mask literals must not contain a line break");
                }
                MaskPosition::Literal(escaped)
            }
            '?' => match chars.next() {
                Some('c') => {
                    if custom.is_empty() {
                        anyhow::bail!("Mask uses '?c' but --charset is empty");
                    }
                    MaskPosition::Variable(custom.to_vec())
                }
                Some('?') => MaskPosition::Literal('?'),
                Some(token) => {
                    let Some(class) = charset_class(token) else {
                        anyhow::bail!("Unknown mask token '?{token}'");
                    };
                    MaskPosition::Variable(class)
                }
                None => anyhow::bail!("Mask ends with a dangling '?'"),
            },
            '\n' | '\r' => anyhow::bail!("Mask literals must not contain a line break"),
            other => MaskPosition::Literal(other),
        };
        if positions.len() == MAX_OUTPUT_LEN {
            anyhow::bail!("Mask output length exceeds the limit of {MAX_OUTPUT_LEN} characters");
        }
        positions.push(position);
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
    let total = enumerated_total(&chars, min_len, max_len)?;
    validate_candidate_count(total, force)?;

    let mut candidate = String::with_capacity(max_len.saturating_mul(4));
    for len in min_len..=max_len {
        let count = combination_count(chars.len(), len)?;
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
    let plan = mask_plan(mask, charset)?;
    let total = plan.total()?;
    validate_candidate_count(total, force)?;

    let bases = plan.variable_bases();
    let mut digits = vec![0usize; bases.len()];
    let mut candidate = String::with_capacity(plan.output_len().saturating_mul(4));
    for _ in 0..total {
        candidate.clear();
        let mut variable = 0;
        for position in &plan.positions {
            match position {
                MaskPosition::Literal(value) => candidate.push(*value),
                MaskPosition::Variable(chars) => {
                    candidate.push(chars[digits[variable]]);
                    variable += 1;
                }
            }
        }
        writer.write_all(candidate.as_bytes())?;
        writer.write_all(b"\n")?;
        increment_mixed(&mut digits, &bases);
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
    validate_length("--length", length)?;
    if count == 0 {
        anyhow::bail!("--count must be at least 1");
    }
    validate_candidate_count(u128::from(count), force)?;

    let mut rng = rand::rng();
    let mut candidate = String::with_capacity(length.saturating_mul(4));
    for _ in 0..count {
        append_random(&mut candidate, &chars, length, &mut rng);
        writer.write_all(candidate.as_bytes())?;
        writer.write_all(b"\n")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_from_index_matches_enumerated_order() {
        let chars = normalize_charset("ab").unwrap();
        let mut output = Vec::new();
        write_enumerated(&mut output, "ab", 1, 2, false).unwrap();
        let text = String::from_utf8(output).unwrap();
        let mut index_by_len = [0u128; 3];
        for line in text.lines() {
            let len = line.chars().count();
            assert_eq!(line, candidate_from_index(&chars, len, index_by_len[len]));
            index_by_len[len] += 1;
        }
    }

    #[test]
    fn mask_candidate_matches_masked_order() {
        let plan = mask_plan("A??B?c?c", "01").unwrap();
        let mut output = Vec::new();
        write_masked(&mut output, "A??B?c?c", "01", false).unwrap();
        let text = String::from_utf8(output).unwrap();
        for (index, line) in text.lines().enumerate() {
            assert_eq!(line, plan.candidate(u128::try_from(index).unwrap()));
        }
    }
}

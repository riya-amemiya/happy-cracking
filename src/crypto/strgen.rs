use anyhow::{Context, Result};
use clap::{Subcommand, ValueEnum};
use std::fs::File;
use std::io::{self, BufWriter, Read, StdoutLock, Write};
use std::path::{Path, PathBuf};

/// Inclusive length cap for exhaustive brute-force and masks.
pub const MAX_EXHAUSTIVE_LEN: usize = 64;

/// Inclusive length cap for random generation.
pub const MAX_RANDOM_LEN: usize = 4096;

/// Default candidate cap for exhaustive generation.
pub const DEFAULT_CANDIDATE_LIMIT: u64 = 100_000_000;

/// Hard candidate cap. `--limit` and random `--count` cannot exceed this.
pub const HARD_CANDIDATE_LIMIT: u64 = 1_000_000_000;

pub const MAX_CHARSET_LEN: usize = 256;

const MAX_CHARSET_BYTES: usize = 65_536;

const MAX_MASK_BYTES: usize = 65_536;

const MAX_AFFIX_CHARS: usize = 1024;

const OUT_BUF_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, ValueEnum)]
pub enum CharsetPreset {
    Lower,
    Upper,
    Digits,
    Hex,
    Alnum,
    All,
}

#[derive(Clone, Copy)]
pub struct BruteParams<'a> {
    pub chars: &'a [char],
    pub min_len: usize,
    pub max_len: usize,
    pub prefix: &'a str,
    pub suffix: &'a str,
    pub limit: u64,
}

#[derive(Clone, Copy)]
pub struct MaskParams<'a> {
    pub mask: &'a str,
    pub custom: [Option<&'a str>; 4],
    pub limit: u64,
}

#[derive(Clone, Copy)]
pub struct RandomParams<'a> {
    pub chars: &'a [char],
    pub min_len: usize,
    pub max_len: usize,
    pub count: u64,
    pub seed: u64,
    pub prefix: &'a str,
    pub suffix: &'a str,
}

#[derive(Clone, Copy)]
pub struct Xoshiro256StarStar {
    s: [u64; 4],
}

impl Xoshiro256StarStar {
    #[must_use]
    pub fn from_state(state: [u64; 4]) -> Self {
        Self { s: state }
    }

    #[must_use]
    pub fn from_seed(seed: u64) -> Self {
        let mut sm = seed;
        let mut s = [0u64; 4];
        for slot in &mut s {
            sm = sm.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = sm;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            *slot = z ^ (z >> 31);
        }
        if s == [0, 0, 0, 0] {
            s[0] = 0x9E37_79B9_7F4A_7C15;
        }
        Self { s }
    }

    #[must_use]
    pub fn next_u64(&mut self) -> u64 {
        let result = self.s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }

    fn next_index(&mut self, n: usize) -> usize {
        let n = n as u64;
        if n.is_power_of_two() {
            return (self.next_u64() & (n - 1)) as usize;
        }
        let mut x = self.next_u64();
        let mut mixed = u128::from(x) * u128::from(n);
        let mut low = mixed as u64;
        if low < n {
            let threshold = n.wrapping_neg() % n;
            while low < threshold {
                x = self.next_u64();
                mixed = u128::from(x) * u128::from(n);
                low = mixed as u64;
            }
        }
        (mixed >> 64) as usize
    }
}

#[derive(Subcommand)]
pub enum StrgenAction {
    #[command(about = "Print random strings drawn from a charset")]
    Random {
        #[arg(
            short,
            long,
            default_value = "",
            help = "Characters to draw from (ignored when --preset is set)"
        )]
        charset: String,
        #[arg(short, long, value_enum, help = "Predefined charset")]
        preset: Option<CharsetPreset>,
        #[arg(long, help = "Fixed length; overrides --min-len and --max-len")]
        len: Option<usize>,
        #[arg(
            long,
            default_value = "1",
            help = "Minimum length when --len is omitted"
        )]
        min_len: usize,
        #[arg(
            long,
            help = "Maximum length when --len is omitted (defaults to --min-len)"
        )]
        max_len: Option<usize>,
        #[arg(long, help = "Number of strings to print")]
        count: u64,
        #[arg(
            long,
            help = "PRNG seed. OS entropy is used when omitted, and that seed is printed to stderr"
        )]
        seed: Option<u64>,
        #[arg(long, default_value = "", help = "Literal prefix")]
        prefix: String,
        #[arg(long, default_value = "", help = "Literal suffix")]
        suffix: String,
        #[arg(short, long, help = "Write to this file instead of stdout")]
        output: Option<PathBuf>,
    },
    #[command(about = "Print every string from --min-len through --max-len over a charset")]
    Brute {
        #[arg(
            short,
            long,
            default_value = "abcdefghijklmnopqrstuvwxyz0123456789",
            help = "Characters to use (ignored when --preset is set)"
        )]
        charset: String,
        #[arg(
            short,
            long,
            value_enum,
            help = "Predefined charset overriding --charset"
        )]
        preset: Option<CharsetPreset>,
        #[arg(long, help = "Minimum length (inclusive)")]
        min_len: usize,
        #[arg(long, help = "Maximum length (inclusive)")]
        max_len: usize,
        #[arg(long, default_value = "", help = "Literal prefix")]
        prefix: String,
        #[arg(long, default_value = "", help = "Literal suffix")]
        suffix: String,
        #[arg(
            long,
            default_value_t = DEFAULT_CANDIDATE_LIMIT,
            help = "Maximum number of candidates to emit"
        )]
        limit: u64,
        #[arg(long, help = "Print the keyspace size instead of the candidates")]
        count_only: bool,
        #[arg(short, long, help = "Write to this file instead of stdout")]
        output: Option<PathBuf>,
    },
    #[command(
        about = "Print every string matching a mask; literal characters stay fixed",
        long_about = "Mask classes: ?l lower, ?u upper, ?d digits, ?h hex, ?s symbols, ?a printable ASCII, ?1-?4 custom charsets (--custom1 .. --custom4), ?? a literal question mark. Other characters are fixed."
    )]
    Mask {
        #[arg(help = "Mask, e.g. 'flag{?d?d?d?d}' or 'id?1?1'")]
        mask: String,
        #[arg(short = '1', long = "custom1", help = "Custom charset for ?1")]
        custom1: Option<String>,
        #[arg(short = '2', long = "custom2", help = "Custom charset for ?2")]
        custom2: Option<String>,
        #[arg(short = '3', long = "custom3", help = "Custom charset for ?3")]
        custom3: Option<String>,
        #[arg(short = '4', long = "custom4", help = "Custom charset for ?4")]
        custom4: Option<String>,
        #[arg(
            long,
            default_value_t = DEFAULT_CANDIDATE_LIMIT,
            help = "Maximum number of candidates to emit"
        )]
        limit: u64,
        #[arg(long, help = "Print the keyspace size instead of the candidates")]
        count_only: bool,
        #[arg(short, long, help = "Write to this file instead of stdout")]
        output: Option<PathBuf>,
    },
}

pub fn run(action: StrgenAction) -> Result<()> {
    match action {
        StrgenAction::Random {
            charset,
            preset,
            len,
            min_len,
            max_len,
            count,
            seed,
            prefix,
            suffix,
            output,
        } => {
            let chars = resolve_charset(&charset, preset)?;
            let (min_len, max_len) = match len {
                Some(fixed) => (fixed, fixed),
                None => (min_len, max_len.unwrap_or(min_len)),
            };
            let generated = seed.is_none();
            let seed = seed.map_or_else(os_seed, Ok)?;
            if generated {
                eprintln!("seed {seed}");
            }
            let params = RandomParams {
                chars: &chars,
                min_len,
                max_len,
                count,
                seed,
                prefix: &prefix,
                suffix: &suffix,
            };
            write_to(output.as_deref(), |out| {
                write_random(out, &params)?;
                Ok(())
            })?;
        }
        StrgenAction::Brute {
            charset,
            preset,
            min_len,
            max_len,
            prefix,
            suffix,
            limit,
            count_only,
            output,
        } => {
            let chars = resolve_charset(&charset, preset)?;
            let keyspace = brute_keyspace(chars.len(), min_len, max_len)?;
            if count_only {
                println!("{keyspace}");
                return Ok(());
            }
            let limit = checked_limit(limit)?;
            let params = BruteParams {
                chars: &chars,
                min_len,
                max_len,
                prefix: &prefix,
                suffix: &suffix,
                limit,
            };
            write_to(output.as_deref(), |out| {
                write_brute(out, &params)?;
                Ok(())
            })?;
        }
        StrgenAction::Mask {
            mask,
            custom1,
            custom2,
            custom3,
            custom4,
            limit,
            count_only,
            output,
        } => {
            let custom = [
                custom1.as_deref(),
                custom2.as_deref(),
                custom3.as_deref(),
                custom4.as_deref(),
            ];
            let positions = expand_mask(&mask, &custom)?;
            let keyspace = mask_keyspace(&positions)?;
            if count_only {
                println!("{keyspace}");
                return Ok(());
            }
            let limit = checked_limit(limit)?;
            let params = MaskParams {
                mask: &mask,
                custom,
                limit,
            };
            write_to(output.as_deref(), |out| {
                write_mask(out, &params)?;
                Ok(())
            })?;
        }
    }
    Ok(())
}

#[must_use]
pub fn preset_charset(preset: CharsetPreset) -> &'static str {
    match preset {
        CharsetPreset::Lower => "abcdefghijklmnopqrstuvwxyz",
        CharsetPreset::Upper => "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
        CharsetPreset::Digits => "0123456789",
        CharsetPreset::Hex => "0123456789abcdef",
        CharsetPreset::Alnum => "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789",
        CharsetPreset::All => {
            "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 !\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~"
        }
    }
}

#[must_use]
pub fn dedup_chars(input: &str) -> Vec<char> {
    let mut out = Vec::new();
    for c in input.chars() {
        if !out.contains(&c) {
            out.push(c);
            if out.len() > MAX_CHARSET_LEN {
                break;
            }
        }
    }
    out
}

pub fn brute_keyspace(base: usize, min_len: usize, max_len: usize) -> Result<u128> {
    if base == 0 {
        anyhow::bail!("Charset must not be empty");
    }
    if max_len < min_len {
        anyhow::bail!("--max-len ({max_len}) must be >= --min-len ({min_len})");
    }
    if max_len > MAX_EXHAUSTIVE_LEN {
        anyhow::bail!("Maximum length {max_len} exceeds the limit of {MAX_EXHAUSTIVE_LEN}");
    }
    if base > MAX_CHARSET_LEN {
        anyhow::bail!("Charset too large ({base} characters); maximum is {MAX_CHARSET_LEN}");
    }
    let mut total = 0u128;
    for len in min_len..=max_len {
        let count = if len == 0 {
            1
        } else {
            (base as u128)
                .checked_pow(u32::try_from(len).context("length does not fit in u32")?)
                .context("Keyspace overflowed")?
        };
        total = total.checked_add(count).context("Keyspace overflowed")?;
    }
    Ok(total)
}

pub fn mask_keyspace(positions: &[Vec<char>]) -> Result<u128> {
    if positions.is_empty() {
        anyhow::bail!("Mask is empty");
    }
    let mut total = 1u128;
    for pos in positions {
        if pos.is_empty() {
            anyhow::bail!("Mask position has an empty charset");
        }
        total = total
            .checked_mul(pos.len() as u128)
            .context("Keyspace overflowed")?;
    }
    Ok(total)
}

pub fn expand_mask(mask: &str, custom: &[Option<&str>; 4]) -> Result<Vec<Vec<char>>> {
    if mask.is_empty() {
        anyhow::bail!("Mask is empty");
    }
    if mask.len() > MAX_MASK_BYTES {
        anyhow::bail!(
            "Mask exceeds maximum size of {MAX_MASK_BYTES} bytes to prevent Denial of Service"
        );
    }
    let chars: Vec<char> = mask.chars().collect();
    let mut positions = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '?' {
            i += 1;
            if i >= chars.len() {
                anyhow::bail!("Mask ends with dangling '?'");
            }
            positions.push(mask_class(chars[i], custom)?);
        } else {
            positions.push(vec![chars[i]]);
        }
        i += 1;
        if positions.len() > MAX_EXHAUSTIVE_LEN {
            anyhow::bail!(
                "Mask length {} exceeds the limit of {MAX_EXHAUSTIVE_LEN}",
                positions.len()
            );
        }
    }
    Ok(positions)
}

pub fn write_brute<W: Write>(out: &mut W, params: &BruteParams<'_>) -> Result<u64> {
    if params.chars.is_empty() {
        anyhow::bail!("Charset must not be empty");
    }
    check_affix("prefix", params.prefix)?;
    check_affix("suffix", params.suffix)?;
    let keyspace = brute_keyspace(params.chars.len(), params.min_len, params.max_len)?;
    ensure_limit(keyspace, params.limit)?;

    let mut written = 0u64;
    if params.chars.iter().all(char::is_ascii) {
        let charset: Vec<u8> = params.chars.iter().map(|&c| c as u8).collect();
        let prefix = params.prefix.as_bytes();
        let suffix = params.suffix.as_bytes();
        for len in params.min_len..=params.max_len {
            written += write_repeated_bytes(out, &charset, len, prefix, suffix)?;
        }
    } else {
        for len in params.min_len..=params.max_len {
            written += write_repeated_chars(out, params.chars, len, params.prefix, params.suffix)?;
        }
    }
    Ok(written)
}

pub fn write_mask<W: Write>(out: &mut W, params: &MaskParams<'_>) -> Result<u64> {
    let positions = expand_mask(params.mask, &params.custom)?;
    let keyspace = mask_keyspace(&positions)?;
    ensure_limit(keyspace, params.limit)?;
    if positions.iter().flatten().all(char::is_ascii) {
        let classes: Vec<Vec<u8>> = positions
            .iter()
            .map(|class| class.iter().map(|&c| c as u8).collect())
            .collect();
        write_product_bytes(out, &classes)
    } else {
        write_product_chars(out, &positions)
    }
}

pub fn write_random<W: Write>(out: &mut W, params: &RandomParams<'_>) -> Result<u64> {
    if params.chars.is_empty() {
        anyhow::bail!("Charset must not be empty");
    }
    if params.chars.len() > MAX_CHARSET_LEN {
        anyhow::bail!(
            "Charset too large ({} characters); maximum is {MAX_CHARSET_LEN}",
            params.chars.len()
        );
    }
    if params.count == 0 {
        anyhow::bail!("--count must be at least 1");
    }
    if params.count > HARD_CANDIDATE_LIMIT {
        anyhow::bail!(
            "--count {} exceeds the limit of {HARD_CANDIDATE_LIMIT} candidates to prevent Denial of Service",
            params.count
        );
    }
    if params.max_len < params.min_len {
        anyhow::bail!(
            "--max-len ({}) must be >= --min-len ({})",
            params.max_len,
            params.min_len
        );
    }
    if params.max_len > MAX_RANDOM_LEN {
        anyhow::bail!(
            "Maximum length {} exceeds the limit of {MAX_RANDOM_LEN}",
            params.max_len
        );
    }
    check_affix("prefix", params.prefix)?;
    check_affix("suffix", params.suffix)?;

    let mut rng = Xoshiro256StarStar::from_seed(params.seed);
    if params.chars.iter().all(char::is_ascii) {
        let charset: Vec<u8> = params.chars.iter().map(|&c| c as u8).collect();
        write_random_bytes(out, params, &charset, &mut rng)
    } else {
        write_random_chars(out, params, &mut rng)
    }
}

fn mask_class(class: char, custom: &[Option<&str>; 4]) -> Result<Vec<char>> {
    let builtin: Option<&str> = match class {
        'l' => Some("abcdefghijklmnopqrstuvwxyz"),
        'u' => Some("ABCDEFGHIJKLMNOPQRSTUVWXYZ"),
        'd' => Some("0123456789"),
        'h' => Some("0123456789abcdef"),
        's' => Some(" !\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~"),
        'a' => None,
        '?' => return Ok(vec!['?']),
        '1' | '2' | '3' | '4' => {
            let idx = usize::from(class as u8 - b'1');
            let Some(spec) = custom[idx] else {
                anyhow::bail!("Custom charset ?{class} is not set");
            };
            return custom_charset(spec, class);
        }
        other => anyhow::bail!("Unknown mask class '?{other}'"),
    };
    if class == 'a' {
        return Ok((0x20u8..=0x7e).map(char::from).collect());
    }
    Ok(builtin
        .context("internal mask class was not mapped")?
        .chars()
        .collect())
}

fn custom_charset(spec: &str, class: char) -> Result<Vec<char>> {
    if spec.len() > MAX_CHARSET_BYTES {
        anyhow::bail!(
            "Custom charset ?{class} exceeds maximum size of {MAX_CHARSET_BYTES} bytes to prevent Denial of Service"
        );
    }
    let chars = dedup_chars(spec);
    if chars.is_empty() {
        anyhow::bail!("Custom charset ?{class} must not be empty");
    }
    if chars.len() > MAX_CHARSET_LEN {
        anyhow::bail!(
            "Custom charset ?{class} is too large ({} characters); maximum is {MAX_CHARSET_LEN}",
            chars.len()
        );
    }
    Ok(chars)
}

fn resolve_charset(charset: &str, preset: Option<CharsetPreset>) -> Result<Vec<char>> {
    let raw = match preset {
        Some(preset) => preset_charset(preset),
        None => charset,
    };
    if raw.len() > MAX_CHARSET_BYTES {
        anyhow::bail!(
            "Charset exceeds maximum size of {MAX_CHARSET_BYTES} bytes to prevent Denial of Service"
        );
    }
    let chars = dedup_chars(raw);
    if chars.is_empty() {
        anyhow::bail!("Charset must not be empty");
    }
    if chars.len() > MAX_CHARSET_LEN {
        anyhow::bail!(
            "Charset too large ({} characters); maximum is {MAX_CHARSET_LEN}",
            chars.len()
        );
    }
    Ok(chars)
}

fn checked_limit(limit: u64) -> Result<u64> {
    if limit == 0 {
        anyhow::bail!("--limit must be at least 1");
    }
    if limit > HARD_CANDIDATE_LIMIT {
        anyhow::bail!(
            "--limit {limit} exceeds the maximum of {HARD_CANDIDATE_LIMIT} to prevent Denial of Service"
        );
    }
    Ok(limit)
}

fn ensure_limit(keyspace: u128, limit: u64) -> Result<()> {
    if keyspace > u128::from(limit) {
        anyhow::bail!(
            "Keyspace {keyspace} exceeds the limit of {limit} candidates; narrow the pattern or raise --limit (maximum {HARD_CANDIDATE_LIMIT}) to prevent Denial of Service"
        );
    }
    Ok(())
}

fn check_affix(label: &str, value: &str) -> Result<()> {
    if value.len() > MAX_AFFIX_CHARS.saturating_mul(4) {
        anyhow::bail!("{label} exceeds the limit of {MAX_AFFIX_CHARS} characters");
    }
    let n = value.chars().count();
    if n > MAX_AFFIX_CHARS {
        anyhow::bail!("{label} exceeds the limit of {MAX_AFFIX_CHARS} characters");
    }
    Ok(())
}

fn write_repeated_bytes<W: Write>(
    out: &mut W,
    charset: &[u8],
    len: usize,
    prefix: &[u8],
    suffix: &[u8],
) -> Result<u64> {
    if len == 0 {
        out.write_all(prefix)?;
        out.write_all(suffix)?;
        out.write_all(b"\n")?;
        return Ok(1);
    }
    let mut idx = [0usize; MAX_EXHAUSTIVE_LEN];
    let mut buf = Vec::with_capacity(prefix.len() + len + suffix.len() + 1);
    buf.extend_from_slice(prefix);
    let body = buf.len();
    buf.extend(std::iter::repeat_n(charset[0], len));
    buf.extend_from_slice(suffix);
    buf.push(b'\n');
    let mut written = 0u64;
    loop {
        out.write_all(&buf)?;
        written += 1;
        if !bump_bytes(&mut idx[..len], &mut buf, body, charset) {
            break;
        }
    }
    Ok(written)
}

fn bump_bytes(idx: &mut [usize], buf: &mut [u8], body: usize, charset: &[u8]) -> bool {
    for i in (0..idx.len()).rev() {
        let next = idx[i] + 1;
        if next < charset.len() {
            idx[i] = next;
            buf[body + i] = charset[next];
            return true;
        }
        idx[i] = 0;
        buf[body + i] = charset[0];
    }
    false
}

fn write_repeated_chars<W: Write>(
    out: &mut W,
    chars: &[char],
    len: usize,
    prefix: &str,
    suffix: &str,
) -> Result<u64> {
    if len == 0 {
        writeln_affix(out, prefix, &[], suffix)?;
        return Ok(1);
    }
    let mut idx = vec![0usize; len];
    let mut body = vec![chars[0]; len];
    let mut written = 0u64;
    loop {
        writeln_affix(out, prefix, &body, suffix)?;
        written += 1;
        if !bump_chars(&mut idx, &mut body, chars) {
            break;
        }
    }
    Ok(written)
}

fn bump_chars(idx: &mut [usize], body: &mut [char], chars: &[char]) -> bool {
    for i in (0..idx.len()).rev() {
        let next = idx[i] + 1;
        if next < chars.len() {
            idx[i] = next;
            body[i] = chars[next];
            return true;
        }
        idx[i] = 0;
        body[i] = chars[0];
    }
    false
}

fn writeln_affix<W: Write>(out: &mut W, prefix: &str, body: &[char], suffix: &str) -> Result<()> {
    let mut line = String::with_capacity(prefix.len() + body.len() * 4 + suffix.len() + 1);
    line.push_str(prefix);
    for &c in body {
        line.push(c);
    }
    line.push_str(suffix);
    line.push('\n');
    out.write_all(line.as_bytes())?;
    Ok(())
}

fn write_product_bytes<W: Write>(out: &mut W, classes: &[Vec<u8>]) -> Result<u64> {
    let n = classes.len();
    let mut idx = vec![0usize; n];
    let mut buf = vec![0u8; n + 1];
    buf[n] = b'\n';
    for i in 0..n {
        buf[i] = classes[i][0];
    }
    let mut written = 0u64;
    loop {
        out.write_all(&buf)?;
        written += 1;
        if !bump_product(&mut idx, &mut buf, classes) {
            break;
        }
    }
    Ok(written)
}

fn bump_product(idx: &mut [usize], buf: &mut [u8], classes: &[Vec<u8>]) -> bool {
    for i in (0..idx.len()).rev() {
        let next = idx[i] + 1;
        if next < classes[i].len() {
            idx[i] = next;
            buf[i] = classes[i][next];
            return true;
        }
        idx[i] = 0;
        buf[i] = classes[i][0];
    }
    false
}

fn write_product_chars<W: Write>(out: &mut W, classes: &[Vec<char>]) -> Result<u64> {
    let n = classes.len();
    let mut idx = vec![0usize; n];
    let mut body: Vec<char> = classes.iter().map(|class| class[0]).collect();
    let mut written = 0u64;
    loop {
        writeln_affix(out, "", &body, "")?;
        written += 1;
        if !bump_chars_classes(&mut idx, &mut body, classes) {
            break;
        }
    }
    Ok(written)
}

fn bump_chars_classes(idx: &mut [usize], body: &mut [char], classes: &[Vec<char>]) -> bool {
    for i in (0..idx.len()).rev() {
        let next = idx[i] + 1;
        if next < classes[i].len() {
            idx[i] = next;
            body[i] = classes[i][next];
            return true;
        }
        idx[i] = 0;
        body[i] = classes[i][0];
    }
    false
}

fn write_random_bytes<W: Write>(
    out: &mut W,
    params: &RandomParams<'_>,
    charset: &[u8],
    rng: &mut Xoshiro256StarStar,
) -> Result<u64> {
    let prefix = params.prefix.as_bytes();
    let suffix = params.suffix.as_bytes();
    let span = params.max_len - params.min_len + 1;
    let mut buf = Vec::with_capacity(prefix.len() + params.max_len + suffix.len() + 1);
    for _ in 0..params.count {
        let len = params.min_len + rng.next_index(span);
        buf.clear();
        buf.extend_from_slice(prefix);
        for _ in 0..len {
            buf.push(charset[rng.next_index(charset.len())]);
        }
        buf.extend_from_slice(suffix);
        buf.push(b'\n');
        out.write_all(&buf)?;
    }
    Ok(params.count)
}

fn write_random_chars<W: Write>(
    out: &mut W,
    params: &RandomParams<'_>,
    rng: &mut Xoshiro256StarStar,
) -> Result<u64> {
    let span = params.max_len - params.min_len + 1;
    let mut body = Vec::with_capacity(params.max_len);
    for _ in 0..params.count {
        let len = params.min_len + rng.next_index(span);
        body.clear();
        for _ in 0..len {
            body.push(params.chars[rng.next_index(params.chars.len())]);
        }
        writeln_affix(out, params.prefix, &body, params.suffix)?;
    }
    Ok(params.count)
}

enum Sink<'a> {
    Stdout(BufWriter<StdoutLock<'a>>),
    File(BufWriter<File>),
}

impl Write for Sink<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Stdout(out) => out.write(buf),
            Self::File(out) => out.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Stdout(out) => out.flush(),
            Self::File(out) => out.flush(),
        }
    }
}

fn write_to<F>(path: Option<&Path>, write: F) -> Result<()>
where
    F: FnOnce(&mut Sink<'_>) -> Result<()>,
{
    let stdout;
    let mut out = if let Some(path) = path {
        let file = File::create(path)
            .with_context(|| format!("Failed to create output file: {}", path.display()))?;
        Sink::File(BufWriter::with_capacity(OUT_BUF_BYTES, file))
    } else {
        stdout = io::stdout();
        Sink::Stdout(BufWriter::with_capacity(OUT_BUF_BYTES, stdout.lock()))
    };
    write(&mut out)?;
    out.flush().with_context(|| match path {
        Some(path) => format!("Failed to flush generated strings to {}", path.display()),
        None => "Failed to flush generated strings".to_string(),
    })?;
    Ok(())
}

fn os_seed() -> Result<u64> {
    let mut bytes = [0u8; 8];
    let mut file = File::open("/dev/urandom").context("Failed to read OS randomness")?;
    file.read_exact(&mut bytes)
        .context("Failed to read OS randomness")?;
    Ok(u64::from_le_bytes(bytes))
}

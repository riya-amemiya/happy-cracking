use anyhow::{Context, Result};
use clap::Subcommand;

#[derive(Subcommand)]
pub enum BinaryAction {
    #[command(about = "Encode text to binary")]
    Encode {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Decode binary to text")]
    Decode {
        #[arg(help = "Binary string (space or no space separated)")]
        input: String,
    },
}

pub fn run(action: BinaryAction) -> Result<()> {
    match action {
        BinaryAction::Encode { input } => {
            println!("{}", encode(&input));
        }
        BinaryAction::Decode { input } => {
            println!("{}", decode(&input)?);
        }
    }
    Ok(())
}

#[must_use]
pub fn encode(input: &str) -> String {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return String::new();
    }

    let mut out = vec![0u8; bytes.len() * 9 - 1];
    let mut pos = 0;
    for &b in bytes {
        if pos != 0 {
            out[pos] = b' ';
            pos += 1;
        }
        for j in 0..8 {
            out[pos + j] = b'0' + ((b >> (7 - j)) & 1);
        }
        pos += 8;
    }
    String::from_utf8(out).expect("binary encode produces ASCII digits and spaces")
}

fn pack8(ascii: u64) -> Option<u8> {
    let x = ascii ^ 0x3030_3030_3030_3030;
    if x & 0xFEFE_FEFE_FEFE_FEFE != 0 {
        return None;
    }
    Some((x.wrapping_mul(0x8040_2010_0804_0201) >> 56) as u8)
}

fn try_pack(src: &[u8], stride: usize) -> Option<Vec<u8>> {
    let n = match stride {
        8 if !src.is_empty() && src.len().is_multiple_of(8) => src.len() / 8,
        9 if src.len() >= 8 && (src.len() + 1).is_multiple_of(9) => (src.len() + 1) / 9,
        _ => return None,
    };
    let mut out = vec![0u8; n];
    for (i, slot) in out.iter_mut().enumerate() {
        let off = i * stride;
        if stride == 9 && i + 1 < n && src[off + 8] != b' ' {
            return None;
        }
        let chunk: [u8; 8] = src[off..off + 8].try_into().ok()?;
        *slot = pack8(u64::from_le_bytes(chunk))?;
    }
    Some(out)
}

fn decode_bits(src: &[u8]) -> Result<Vec<u8>> {
    if src.is_empty() {
        return Ok(Vec::new());
    }
    if let Some(out) = try_pack(src, 9) {
        return Ok(out);
    }
    if let Some(out) = try_pack(src, 8) {
        return Ok(out);
    }
    let mut bytes = Vec::with_capacity(src.len() / 8);
    let mut acc = 0u8;
    let mut nbits = 0u8;
    for &b in src {
        if b == b'0' || b == b'1' {
            acc = (acc << 1) | (b - b'0');
            nbits += 1;
            if nbits == 8 {
                bytes.push(acc);
                acc = 0;
                nbits = 0;
            }
        }
    }
    if nbits != 0 {
        anyhow::bail!("Binary string length must be a multiple of 8");
    }
    Ok(bytes)
}

pub fn decode(input: &str) -> Result<String> {
    String::from_utf8(decode_bits(input.as_bytes())?).context("Decoded data is not valid UTF-8")
}

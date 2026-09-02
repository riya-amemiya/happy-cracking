use anyhow::{Context, Result};
use clap::Subcommand;

#[derive(Subcommand)]
pub enum HashExtAction {
    #[command(about = "SHA-256 hash length extension attack")]
    Sha256Extend {
        #[arg(help = "Original hash (hex)")]
        original_hash: String,
        #[arg(long, help = "Length of secret + original message in bytes")]
        original_len: u64,
        #[arg(long, help = "Data to append")]
        append: String,
    },
}

pub fn run(action: HashExtAction) -> Result<()> {
    match action {
        HashExtAction::Sha256Extend {
            original_hash,
            original_len,
            append,
        } => {
            let result = sha256_extend(&original_hash, original_len, append.as_bytes())?;
            println!("New hash:       {}", result.new_hash);
            println!("Forged suffix:  {}", hex::encode(&result.forged_suffix));
        }
    }
    Ok(())
}

pub struct ExtensionResult {
    pub new_hash: String,
    pub forged_suffix: Vec<u8>,
}

// Given H(secret || message) and the total length of (secret || message),
// computes H(secret || message || padding || append) without knowing the secret.
pub fn sha256_extend(
    original_hash_hex: &str,
    original_len: u64,
    append: &[u8],
) -> Result<ExtensionResult> {
    let hash_bytes =
        hex::decode(original_hash_hex.trim()).context("Invalid hex in original hash")?;
    if hash_bytes.len() != 32 {
        anyhow::bail!(
            "SHA-256 hash must be 32 bytes (64 hex chars), got {}",
            hash_bytes.len()
        );
    }

    let mut state = [0u32; 8];
    for i in 0..8 {
        state[i] = u32::from_be_bytes([
            hash_bytes[i * 4],
            hash_bytes[i * 4 + 1],
            hash_bytes[i * 4 + 2],
            hash_bytes[i * 4 + 3],
        ]);
    }

    let glue_padding = sha256_padding(original_len)?;

    let total_processed = original_len
        .checked_add(glue_padding.len() as u64)
        .context("Padded original length overflow")?;

    let mut buffer = append.to_vec();
    let total_bytes = total_processed
        .checked_add(append.len() as u64)
        .context("Extended message length overflow")?;
    let final_bit_len = sha256_bit_len(total_bytes)?;
    let append_padding = sha256_finish_padding(append.len() as u64, final_bit_len);
    buffer.extend_from_slice(&append_padding);

    for chunk in buffer.chunks(64) {
        let mut block = [0u8; 64];
        block[..chunk.len()].copy_from_slice(chunk);
        sha256_compress(&mut state, &block);
    }

    let mut new_hash_bytes = Vec::with_capacity(32);
    for &word in &state {
        new_hash_bytes.extend_from_slice(&word.to_be_bytes());
    }

    let mut forged_suffix = glue_padding;
    forged_suffix.extend_from_slice(append);

    Ok(ExtensionResult {
        new_hash: hex::encode(&new_hash_bytes),
        forged_suffix,
    })
}

/// Convert a byte length to SHA-256's 64-bit bit-length field.
///
/// Security: `byte_len * 8` panics in debug (CLI `DoS` via `--original-len`) and
/// wraps in release, producing incorrect glue padding and a wrong forged hash.
fn sha256_bit_len(byte_len: u64) -> Result<u64> {
    byte_len
        .checked_mul(8)
        .context("Message length exceeds SHA-256's 64-bit bit-length field")
}

// Padding is: 0x80, then zeros, then 8-byte big-endian bit length,
// such that the total padded length is a multiple of 64 bytes.
fn sha256_padding(message_len: u64) -> Result<Vec<u8>> {
    let bit_len = sha256_bit_len(message_len)?;
    let remainder = (message_len % 64) as usize;
    // We need at least 1 + 8 bytes (0x80 + length), padded to 64
    let padding_len = if remainder < 56 {
        56 - remainder
    } else {
        120 - remainder
    };

    let mut padding = Vec::with_capacity(padding_len + 8);
    padding.push(0x80);
    padding.resize(padding_len, 0x00);
    padding.extend_from_slice(&bit_len.to_be_bytes());
    Ok(padding)
}

fn sha256_finish_padding(append_len: u64, total_bit_len: u64) -> Vec<u8> {
    let remainder = (append_len % 64) as usize;
    let padding_len = if remainder < 56 {
        56 - remainder
    } else {
        120 - remainder
    };

    let mut padding = Vec::with_capacity(padding_len + 8);
    padding.push(0x80);
    padding.resize(padding_len, 0x00);
    padding.extend_from_slice(&total_bit_len.to_be_bytes());
    padding
}

const K: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

fn sha256_compress(state: &mut [u32; 8], block: &[u8; 64]) {
    let mut w = [0u32; 64];
    for i in 0..16 {
        w[i] = u32::from_be_bytes([
            block[i * 4],
            block[i * 4 + 1],
            block[i * 4 + 2],
            block[i * 4 + 3],
        ]);
    }
    for i in 16..64 {
        let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
        let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
    }

    let mut a = state[0];
    let mut b = state[1];
    let mut c = state[2];
    let mut d = state[3];
    let mut e = state[4];
    let mut f = state[5];
    let mut g = state[6];
    let mut h = state[7];

    for i in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ ((!e) & g);
        let temp1 = h
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(K[i])
            .wrapping_add(w[i]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let temp2 = s0.wrapping_add(maj);

        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(temp1);
        d = c;
        c = b;
        b = a;
        a = temp1.wrapping_add(temp2);
    }

    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
    state[4] = state[4].wrapping_add(e);
    state[5] = state[5].wrapping_add(f);
    state[6] = state[6].wrapping_add(g);
    state[7] = state[7].wrapping_add(h);
}

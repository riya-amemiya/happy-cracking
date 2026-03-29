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

// Performs a SHA-256 hash length extension attack.
//
// Given H(secret || message) and the total length of (secret || message),
// computes H(secret || message || padding || append) without knowing the secret.
pub fn sha256_extend(
    original_hash_hex: &str,
    original_len: u64,
    append: &[u8],
) -> Result<ExtensionResult> {
    // Security: Reject original_len values that would cause integer overflow
    // in bit-length calculations (original_len * 8 must fit in u64).
    // SHA-256 itself is defined for messages up to 2^64 - 1 bits, so the
    // byte-length limit is (2^64 - 1) / 8 = 2305843009213693951.
    const MAX_MESSAGE_BYTES: u64 = u64::MAX / 8;
    if original_len > MAX_MESSAGE_BYTES {
        anyhow::bail!(
            "original_len ({}) is too large and would cause integer overflow in bit-length calculation (max {})",
            original_len,
            MAX_MESSAGE_BYTES
        );
    }

    let hash_bytes =
        hex::decode(original_hash_hex.trim()).context("Invalid hex in original hash")?;
    if hash_bytes.len() != 32 {
        anyhow::bail!(
            "SHA-256 hash must be 32 bytes (64 hex chars), got {}",
            hash_bytes.len()
        );
    }

    // Recover the SHA-256 internal state from the hash output
    let mut state = [0u32; 8];
    for i in 0..8 {
        state[i] = u32::from_be_bytes([
            hash_bytes[i * 4],
            hash_bytes[i * 4 + 1],
            hash_bytes[i * 4 + 2],
            hash_bytes[i * 4 + 3],
        ]);
    }

    // Compute the padding that would have been applied to the original message
    let glue_padding = sha256_padding(original_len);

    // Security: Check for overflow before addition.
    let total_processed = original_len
        .checked_add(glue_padding.len() as u64)
        .ok_or_else(|| anyhow::anyhow!("Integer overflow computing total processed length"))?;

    // Build the message blocks for the appended data and process them
    let mut buffer = append.to_vec();
    // Security: Use checked arithmetic to prevent silent overflow in bit-length.
    let total_with_append = total_processed
        .checked_add(append.len() as u64)
        .ok_or_else(|| {
            anyhow::anyhow!("Integer overflow computing total length with appended data")
        })?;
    let final_bit_len = total_with_append
        .checked_mul(8)
        .ok_or_else(|| anyhow::anyhow!("Integer overflow computing final bit length"))?;
    let append_padding = sha256_finish_padding(append.len() as u64, final_bit_len);
    buffer.extend_from_slice(&append_padding);

    // Process each 64-byte block through SHA-256 compression
    for chunk in buffer.chunks(64) {
        let mut block = [0u8; 64];
        block[..chunk.len()].copy_from_slice(chunk);
        sha256_compress(&mut state, &block);
    }

    // Produce the new hash
    let mut new_hash_bytes = Vec::with_capacity(32);
    for &word in &state {
        new_hash_bytes.extend_from_slice(&word.to_be_bytes());
    }

    // The forged suffix is the original padding + appended data
    let mut forged_suffix = glue_padding;
    forged_suffix.extend_from_slice(append);

    Ok(ExtensionResult {
        new_hash: hex::encode(&new_hash_bytes),
        forged_suffix,
    })
}

// Computes the SHA-256 padding for a message of the given byte length.
// Padding is: 0x80, then zeros, then 8-byte big-endian bit length,
// such that the total padded length is a multiple of 64 bytes.
fn sha256_padding(message_len: u64) -> Vec<u8> {
    let bit_len = message_len * 8;
    // Number of bytes in the last incomplete block
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
    padding
}

// Computes padding for the appended data block, using the total accumulated
// bit length (including the original message + glue padding + append data).
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

// SHA-256 round constants
const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

// SHA-256 compression function.
// Processes one 64-byte block and updates the state in-place.
fn sha256_compress(state: &mut [u32; 8], block: &[u8; 64]) {
    // Prepare message schedule
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

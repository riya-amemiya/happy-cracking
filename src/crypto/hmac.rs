use anyhow::Result;
use clap::Subcommand;
use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};

#[derive(Subcommand)]
pub enum HmacAction {
    #[command(about = "HMAC-MD5")]
    Md5 {
        #[arg(help = "Input message")]
        input: String,
        #[arg(short, long, help = "HMAC key")]
        key: String,
    },
    #[command(about = "HMAC-SHA1")]
    Sha1 {
        #[arg(help = "Input message")]
        input: String,
        #[arg(short, long, help = "HMAC key")]
        key: String,
    },
    #[command(about = "HMAC-SHA256")]
    Sha256 {
        #[arg(help = "Input message")]
        input: String,
        #[arg(short, long, help = "HMAC key")]
        key: String,
    },
    #[command(about = "HMAC-SHA512")]
    Sha512 {
        #[arg(help = "Input message")]
        input: String,
        #[arg(short, long, help = "HMAC key")]
        key: String,
    },
    #[command(about = "Verify HMAC-MD5 tag in constant time")]
    VerifyMd5 {
        #[arg(help = "Input message")]
        input: String,
        #[arg(short, long, help = "HMAC key")]
        key: String,
        #[arg(short, long, help = "Expected hex-encoded tag")]
        tag: String,
    },
    #[command(about = "Verify HMAC-SHA1 tag in constant time")]
    VerifySha1 {
        #[arg(help = "Input message")]
        input: String,
        #[arg(short, long, help = "HMAC key")]
        key: String,
        #[arg(short, long, help = "Expected hex-encoded tag")]
        tag: String,
    },
    #[command(about = "Verify HMAC-SHA256 tag in constant time")]
    VerifySha256 {
        #[arg(help = "Input message")]
        input: String,
        #[arg(short, long, help = "HMAC key")]
        key: String,
        #[arg(short, long, help = "Expected hex-encoded tag")]
        tag: String,
    },
    #[command(about = "Verify HMAC-SHA512 tag in constant time")]
    VerifySha512 {
        #[arg(help = "Input message")]
        input: String,
        #[arg(short, long, help = "HMAC key")]
        key: String,
        #[arg(short, long, help = "Expected hex-encoded tag")]
        tag: String,
    },
}

pub fn run(action: HmacAction) -> Result<()> {
    match action {
        HmacAction::Md5 { input, key } => {
            println!("{}", hmac_md5(key.as_bytes(), input.as_bytes()));
        }
        HmacAction::Sha1 { input, key } => {
            println!("{}", hmac_sha1(key.as_bytes(), input.as_bytes()));
        }
        HmacAction::Sha256 { input, key } => {
            println!("{}", hmac_sha256(key.as_bytes(), input.as_bytes()));
        }
        HmacAction::Sha512 { input, key } => {
            println!("{}", hmac_sha512(key.as_bytes(), input.as_bytes()));
        }
        HmacAction::VerifyMd5 { input, key, tag } => {
            print_verify(verify_md5(key.as_bytes(), input.as_bytes(), &tag));
        }
        HmacAction::VerifySha1 { input, key, tag } => {
            print_verify(verify_sha1(key.as_bytes(), input.as_bytes(), &tag));
        }
        HmacAction::VerifySha256 { input, key, tag } => {
            print_verify(verify_sha256(key.as_bytes(), input.as_bytes(), &tag));
        }
        HmacAction::VerifySha512 { input, key, tag } => {
            print_verify(verify_sha512(key.as_bytes(), input.as_bytes(), &tag));
        }
    }
    Ok(())
}

fn print_verify(is_match: bool) {
    // Distinct exit words keep shell scripts robust while avoiding length
    // differences that would leak through user-visible timing.
    if is_match {
        println!("ok");
    } else {
        println!("mismatch");
    }
}

fn hmac<D: Digest>(key: &[u8], message: &[u8], block_size: usize) -> Vec<u8> {
    let actual_key = if key.len() > block_size {
        let mut hasher = D::new();
        hasher.update(key);
        hasher.finalize().to_vec()
    } else {
        key.to_vec()
    };

    let mut padded_key = vec![0u8; block_size];
    padded_key[..actual_key.len()].copy_from_slice(&actual_key);

    let ipad: Vec<u8> = padded_key.iter().map(|&k| k ^ 0x36).collect();
    let opad: Vec<u8> = padded_key.iter().map(|&k| k ^ 0x5c).collect();

    let mut inner_hasher = D::new();
    inner_hasher.update(&ipad);
    inner_hasher.update(message);
    let inner_hash = inner_hasher.finalize();

    let mut outer_hasher = D::new();
    outer_hasher.update(&opad);
    outer_hasher.update(&inner_hash);
    outer_hasher.finalize().to_vec()
}

pub fn hmac_md5(key: &[u8], message: &[u8]) -> String {
    hex::encode(hmac::<Md5>(key, message, 64))
}

pub fn hmac_sha1(key: &[u8], message: &[u8]) -> String {
    hex::encode(hmac::<Sha1>(key, message, 64))
}

pub fn hmac_sha256(key: &[u8], message: &[u8]) -> String {
    hex::encode(hmac::<Sha256>(key, message, 64))
}

pub fn hmac_sha512(key: &[u8], message: &[u8]) -> String {
    hex::encode(hmac::<Sha512>(key, message, 128))
}

/// Constant-time byte-slice equality check.
///
/// Returns `true` iff `a` and `b` have identical length and contents. The
/// running time is proportional to `a.len()` regardless of where (or whether)
/// the first differing byte occurs, which is the property MAC verification
/// needs to avoid leaking secret tags through timing side channels.
///
/// Using `==` on `&[u8]` or comparing two hex-encoded `String`s would short
/// circuit at the first mismatch and is therefore unsafe in this role.
#[inline]
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn verify_with<D: Digest>(
    key: &[u8],
    message: &[u8],
    expected_tag_hex: &str,
    block_size: usize,
) -> bool {
    match hex::decode(expected_tag_hex) {
        Ok(expected) => ct_eq(&hmac::<D>(key, message, block_size), &expected),
        Err(_) => false,
    }
}

/// Verifies an HMAC-MD5 tag against a freshly computed MAC in constant time.
pub fn verify_md5(key: &[u8], message: &[u8], expected_tag_hex: &str) -> bool {
    verify_with::<Md5>(key, message, expected_tag_hex, 64)
}

/// Verifies an HMAC-SHA1 tag against a freshly computed MAC in constant time.
pub fn verify_sha1(key: &[u8], message: &[u8], expected_tag_hex: &str) -> bool {
    verify_with::<Sha1>(key, message, expected_tag_hex, 64)
}

/// Verifies an HMAC-SHA256 tag against a freshly computed MAC in constant time.
pub fn verify_sha256(key: &[u8], message: &[u8], expected_tag_hex: &str) -> bool {
    verify_with::<Sha256>(key, message, expected_tag_hex, 64)
}

/// Verifies an HMAC-SHA512 tag against a freshly computed MAC in constant time.
pub fn verify_sha512(key: &[u8], message: &[u8], expected_tag_hex: &str) -> bool {
    verify_with::<Sha512>(key, message, expected_tag_hex, 128)
}

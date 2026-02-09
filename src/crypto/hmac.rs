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
    }
    Ok(())
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

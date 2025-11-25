use anyhow::Result;
use clap::Subcommand;
use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};

#[derive(Subcommand)]
pub enum HashAction {
    #[command(about = "Generate MD5 hash")]
    Md5 {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Generate SHA1 hash")]
    Sha1 {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Generate SHA256 hash")]
    Sha256 {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Generate SHA512 hash")]
    Sha512 {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Generate all hashes")]
    All {
        #[arg(help = "Input text")]
        input: String,
    },
}

pub fn run(action: HashAction) -> Result<()> {
    match action {
        HashAction::Md5 { input } => {
            println!("{}", md5_hash(&input));
        }
        HashAction::Sha1 { input } => {
            println!("{}", sha1_hash(&input));
        }
        HashAction::Sha256 { input } => {
            println!("{}", sha256_hash(&input));
        }
        HashAction::Sha512 { input } => {
            println!("{}", sha512_hash(&input));
        }
        HashAction::All { input } => {
            println!("MD5:    {}", md5_hash(&input));
            println!("SHA1:   {}", sha1_hash(&input));
            println!("SHA256: {}", sha256_hash(&input));
            println!("SHA512: {}", sha512_hash(&input));
        }
    }
    Ok(())
}

pub fn md5_hash(input: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn sha1_hash(input: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn sha256_hash(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn sha512_hash(input: &str) -> String {
    let mut hasher = Sha512::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

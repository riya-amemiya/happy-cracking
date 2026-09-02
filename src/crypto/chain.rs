use anyhow::{Context, Result};
use clap::Subcommand;

use crate::crypto;

const MAX_OPERATIONS: usize = 50;
const MAX_OUTPUT_SIZE: usize = 50 * 1024 * 1024; // 50MB

#[derive(Subcommand)]
pub enum ChainAction {
    #[command(about = "Apply a chain of operations")]
    Run {
        #[arg(help = "Input data")]
        input: String,
        #[arg(
            short,
            long,
            help = "Comma-separated list of operations (e.g. base64-decode,rot13,hex-encode)"
        )]
        ops: String,
    },
}

pub fn run(action: ChainAction) -> Result<()> {
    match action {
        ChainAction::Run { input, ops } => {
            println!("{}", chain(&input, &ops)?);
        }
    }
    Ok(())
}

fn apply_operation(input: &str, op: &str) -> Result<String> {
    match op {
        "base64-encode" => Ok(crypto::base64::encode(input)),
        "base64-decode" => crypto::base64::decode(input),
        "base32-encode" => Ok(crypto::base32::encode(input)),
        "base32-decode" => crypto::base32::decode(input),
        "hex-encode" => Ok(crypto::hex::encode(input)),
        "hex-decode" => crypto::hex::decode(input),
        "url-encode" => Ok(crypto::url::encode(input)),
        "url-decode" => crypto::url::decode(input),
        "binary-encode" => Ok(crypto::binary::encode(input)),
        "binary-decode" => crypto::binary::decode(input),
        "rot13" => Ok(crypto::rot::rot13(input)),
        "rot47" => Ok(crypto::rot::rot47(input)),
        "reverse" => Ok(input.chars().rev().collect()),
        "upper" => Ok(input.to_uppercase()),
        "lower" => Ok(input.to_lowercase()),
        _ => anyhow::bail!("Unknown operation: {op}"),
    }
}

pub fn chain(input: &str, ops: &str) -> Result<String> {
    if ops.trim().is_empty() {
        return Ok(input.to_string());
    }

    let op_count = ops.split(',').count();
    if op_count > MAX_OPERATIONS {
        anyhow::bail!("Too many operations: {op_count} (limit: {MAX_OPERATIONS})");
    }

    ops.split(',')
        .map(str::trim)
        .try_fold(input.to_string(), |current, op| {
            if current.len() > MAX_OUTPUT_SIZE {
                anyhow::bail!(
                    "Output size limit exceeded: {} bytes (limit: {} bytes)",
                    current.len(),
                    MAX_OUTPUT_SIZE
                );
            }
            apply_operation(&current, op).with_context(|| format!("Failed at operation '{op}'"))
        })
}

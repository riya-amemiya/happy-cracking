use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum HashIdAction {
    #[command(about = "Identify hash type")]
    Identify {
        #[arg(help = "Hash string to identify")]
        input: String,
    },
}

pub fn run(action: HashIdAction) -> Result<()> {
    match action {
        HashIdAction::Identify { input } => {
            let results = identify(&input);
            if results.is_empty() {
                println!("Unknown hash format");
            } else {
                println!("Possible hash types:");
                for result in results {
                    println!("  - {}", result);
                }
            }
        }
    }
    Ok(())
}

pub fn identify(input: &str) -> Vec<&'static str> {
    let input = input.trim();
    let len = input.len();
    let is_hex = input.chars().all(|c| c.is_ascii_hexdigit());
    let is_base64 = input
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=');

    let mut results = Vec::new();

    if is_hex {
        match len {
            32 => {
                results.push("MD5");
                results.push("MD4");
                results.push("MD2");
                results.push("NTLM");
            }
            40 => {
                results.push("SHA-1");
                results.push("RIPEMD-160");
            }
            56 => {
                results.push("SHA-224");
            }
            64 => {
                results.push("SHA-256");
                results.push("RIPEMD-256");
                results.push("Snefru-256");
                results.push("GOST R 34.11-94");
            }
            96 => {
                results.push("SHA-384");
            }
            128 => {
                results.push("SHA-512");
                results.push("Whirlpool");
                results.push("SHA-512/256");
            }
            _ => {}
        }
    }

    // Check for bcrypt
    if input.starts_with("$2a$")
        || input.starts_with("$2b$")
        || input.starts_with("$2y$")
        || input.starts_with("$2x$")
    {
        results.push("bcrypt");
    }

    // Check for Argon2
    if input.starts_with("$argon2") {
        results.push("Argon2");
    }

    // Check for Unix crypt formats
    if input.starts_with("$1$") {
        results.push("MD5 Crypt (Unix)");
    }
    if input.starts_with("$5$") {
        results.push("SHA-256 Crypt (Unix)");
    }
    if input.starts_with("$6$") {
        results.push("SHA-512 Crypt (Unix)");
    }

    // Check for Base64-encoded hashes
    if is_base64 && !is_hex {
        match len {
            24 => {
                results.push("MD5 (Base64)");
            }
            28 => {
                results.push("SHA-1 (Base64)");
            }
            44 => {
                results.push("SHA-256 (Base64)");
            }
            88 => {
                results.push("SHA-512 (Base64)");
            }
            _ => {}
        }
    }

    // MySQL
    if len == 16 && is_hex {
        results.push("MySQL (old)");
    }
    if len == 41 && input.starts_with('*') {
        results.push("MySQL 5.x");
    }

    results
}

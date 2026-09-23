use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum RailFenceAction {
    #[command(about = "Encrypt with Rail Fence cipher")]
    Encrypt {
        #[arg(help = "Input text")]
        input: String,
        #[arg(short, long, help = "Number of rails", default_value = "3")]
        rails: usize,
    },
    #[command(about = "Decrypt Rail Fence cipher")]
    Decrypt {
        #[arg(help = "Encrypted text")]
        input: String,
        #[arg(short, long, help = "Number of rails", default_value = "3")]
        rails: usize,
    },
    #[command(about = "Bruteforce all possible rail counts")]
    Bruteforce {
        #[arg(help = "Encrypted text")]
        input: String,
        #[arg(short, long, help = "Maximum rails to try", default_value = "10")]
        max_rails: usize,
    },
}

pub fn run(action: RailFenceAction) -> Result<()> {
    match action {
        RailFenceAction::Encrypt { input, rails } => {
            println!("{}", encrypt(&input, rails)?);
        }
        RailFenceAction::Decrypt { input, rails } => {
            println!("{}", decrypt(&input, rails)?);
        }
        RailFenceAction::Bruteforce { input, max_rails } => {
            check_max_rails(max_rails)?;
            for r in 2..=max_rails {
                if let Ok(result) = decrypt(&input, r) {
                    println!("Rails {r}: {result}");
                }
            }
        }
    }
    Ok(())
}

pub const MAX_RAILS: usize = 10_000;

pub fn check_max_rails(max_rails: usize) -> Result<()> {
    if max_rails > MAX_RAILS {
        anyhow::bail!(
            "Maximum rails {max_rails} exceeds the maximum allowed of {MAX_RAILS} to prevent Denial of Service"
        );
    }
    Ok(())
}

fn visit_zigzag_indices(len: usize, rails: usize, mut visit: impl FnMut(usize)) {
    let cycle = 2 * (rails - 1);
    for rail in 0..rails {
        let mut i = rail;
        if rail == 0 || rail == rails - 1 {
            while i < len {
                visit(i);
                i += cycle;
            }
        } else {
            let mut down = true;
            while i < len {
                visit(i);
                i += if down { cycle - 2 * rail } else { 2 * rail };
                down = !down;
            }
        }
    }
}

pub fn encrypt(input: &str, rails: usize) -> Result<String> {
    if rails < 2 {
        anyhow::bail!("Number of rails must be at least 2");
    }

    let chars: Vec<char> = input.chars().collect();
    if chars.is_empty() {
        return Ok(String::new());
    }

    // If rails >= length, it's just the identity transformation
    // This prevents DoS via massive allocation (e.g. 100M rails for 10 chars)
    if rails >= chars.len() {
        return Ok(input.to_string());
    }

    let mut out = String::with_capacity(chars.len());
    visit_zigzag_indices(chars.len(), rails, |i| out.push(chars[i]));
    Ok(out)
}

pub fn decrypt(input: &str, rails: usize) -> Result<String> {
    if rails < 2 {
        anyhow::bail!("Number of rails must be at least 2");
    }

    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    if len == 0 {
        return Ok(String::new());
    }

    // If rails >= length, it's just the identity transformation
    // This prevents DoS via massive allocation (e.g. 100M rails for 10 chars)
    if rails >= len {
        return Ok(input.to_string());
    }

    let mut out = vec!['\0'; len];
    let mut src = 0usize;
    visit_zigzag_indices(len, rails, |i| {
        out[i] = chars[src];
        src += 1;
    });
    Ok(out.into_iter().collect())
}

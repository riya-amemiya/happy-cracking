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
            for r in 2..=max_rails {
                if let Ok(result) = decrypt(&input, r) {
                    println!("Rails {}: {}", r, result);
                }
            }
        }
    }
    Ok(())
}

pub fn encrypt(input: &str, rails: usize) -> Result<String> {
    if rails < 2 {
        anyhow::bail!("Number of rails must be at least 2");
    }

    let chars: Vec<char> = input.chars().collect();
    if chars.is_empty() {
        return Ok(String::new());
    }

    let mut fence: Vec<Vec<char>> = vec![Vec::new(); rails];
    let mut rail = 0;
    let mut direction = 1i32;

    for c in chars {
        fence[rail].push(c);
        rail = (rail as i32 + direction) as usize;

        if rail == 0 || rail == rails - 1 {
            direction = -direction;
        }
    }

    Ok(fence.into_iter().flatten().collect())
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

    // Calculate the length of each rail
    let mut rail_lengths = vec![0usize; rails];
    let mut rail = 0;
    let mut direction = 1i32;

    for _ in 0..len {
        rail_lengths[rail] += 1;
        rail = (rail as i32 + direction) as usize;
        if rail == 0 || rail == rails - 1 {
            direction = -direction;
        }
    }

    // Split input into rails
    let mut fence: Vec<Vec<char>> = Vec::new();
    let mut pos = 0;
    for &length in &rail_lengths {
        fence.push(chars[pos..pos + length].to_vec());
        pos += length;
    }

    // Read off the fence
    let mut result = String::new();
    let mut rail_indices = vec![0usize; rails];
    rail = 0;
    direction = 1;

    for _ in 0..len {
        result.push(fence[rail][rail_indices[rail]]);
        rail_indices[rail] += 1;
        rail = (rail as i32 + direction) as usize;
        if rail == 0 || rail == rails - 1 {
            direction = -direction;
        }
    }

    Ok(result)
}

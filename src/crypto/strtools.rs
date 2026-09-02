use anyhow::Result;
use clap::Subcommand;
use umt_rust::string::umt_reverse_string;

#[derive(Subcommand)]
pub enum StrToolsAction {
    #[command(about = "Reverse a string")]
    Reverse {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Show ASCII/Unicode code points for each character")]
    Ord {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Convert space-separated code points to characters")]
    Chr {
        #[arg(help = "Space-separated code points (e.g. \"72 101 108 108 111\")")]
        input: String,
    },
}

pub fn run(action: StrToolsAction) -> Result<()> {
    match action {
        StrToolsAction::Reverse { input } => {
            println!("{}", reverse(&input));
        }
        StrToolsAction::Ord { input } => {
            println!("{}", ord(&input));
        }
        StrToolsAction::Chr { input } => {
            println!("{}", chr(&input)?);
        }
    }
    Ok(())
}

#[must_use]
pub fn reverse(input: &str) -> String {
    umt_reverse_string(input)
}

#[must_use]
pub fn ord(input: &str) -> String {
    if input.is_empty() {
        return String::new();
    }

    let mut out = String::with_capacity(input.len() * 10);
    let mut first = true;

    for c in input.chars() {
        if !first {
            out.push(' ');
        }
        first = false;

        out.push(c);
        out.push('=');

        let mut n = c as u32;
        if n == 0 {
            out.push('0');
        } else {
            let mut buf = [0u8; 10];
            let mut i = 10;
            while n > 0 {
                i -= 1;
                buf[i] = (n % 10) as u8 + b'0';
                n /= 10;
            }
            for &digit in &buf[i..] {
                out.push(char::from(digit));
            }
        }
    }

    out
}

pub fn chr(input: &str) -> Result<String> {
    if input.is_empty() {
        return Ok(String::new());
    }

    input
        .split_whitespace()
        .map(|s| {
            let n: u32 = s
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid code point: {s}"))?;
            char::from_u32(n).ok_or_else(|| anyhow::anyhow!("Invalid Unicode code point: {n}"))
        })
        .collect()
}

use anyhow::{Context, Result};
use clap::Subcommand;

#[derive(Subcommand)]
pub enum NumberSysAction {
    #[command(about = "Convert number between bases (2-36)")]
    Convert {
        #[arg(help = "Input number as string")]
        input: String,
        #[arg(long, default_value_t = 10, help = "Source base (2-36)")]
        from: u32,
        #[arg(long, default_value_t = 16, help = "Target base (2-36)")]
        to: u32,
    },
}

pub fn run(action: NumberSysAction) -> Result<()> {
    match action {
        NumberSysAction::Convert { input, from, to } => {
            println!("{}", convert(&input, from, to)?);
        }
    }
    Ok(())
}

pub fn convert(input: &str, from_base: u32, to_base: u32) -> Result<String> {
    let input = input.trim();

    if !(2..=36).contains(&from_base) {
        anyhow::bail!("Source base must be between 2 and 36, got {}", from_base);
    }
    if !(2..=36).contains(&to_base) {
        anyhow::bail!("Target base must be between 2 and 36, got {}", to_base);
    }
    if input.is_empty() {
        anyhow::bail!("Input cannot be empty");
    }

    let value = u128::from_str_radix(input, from_base).context("Failed to parse input number")?;

    if value == 0 {
        return Ok("0".to_string());
    }

    let mut result = Vec::new();
    let mut v = value;

    while v > 0 {
        let digit = (v % to_base as u128) as u8;
        let c = if digit < 10 {
            b'0' + digit
        } else {
            b'a' + digit - 10
        };
        result.push(c as char);
        v /= to_base as u128;
    }

    result.reverse();
    Ok(result.into_iter().collect())
}

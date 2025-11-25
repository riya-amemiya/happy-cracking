use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum AtbashAction {
    #[command(about = "Apply Atbash cipher (A↔Z, B↔Y, ...)")]
    Transform {
        #[arg(help = "Input text")]
        input: String,
    },
}

pub fn run(action: AtbashAction) -> Result<()> {
    match action {
        AtbashAction::Transform { input } => {
            println!("{}", transform(&input));
        }
    }
    Ok(())
}

pub fn transform(input: &str) -> String {
    input
        .chars()
        .map(|c| {
            if c.is_ascii_uppercase() {
                (b'Z' - (c as u8 - b'A')) as char
            } else if c.is_ascii_lowercase() {
                (b'z' - (c as u8 - b'a')) as char
            } else {
                c
            }
        })
        .collect()
}

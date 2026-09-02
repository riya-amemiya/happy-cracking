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

#[must_use]
pub fn transform(input: &str) -> String {
    let mut bytes = input.as_bytes().to_vec();
    for b in &mut bytes {
        if b.is_ascii_uppercase() {
            *b = b'Z' - (*b - b'A');
        } else if b.is_ascii_lowercase() {
            *b = b'z' - (*b - b'a');
        }
    }
    String::from_utf8(bytes).expect("atbash transform preserves UTF-8 validity")
}

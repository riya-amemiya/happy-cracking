use anyhow::{Context, Result};
use clap::Subcommand;

#[derive(Subcommand)]
pub enum NatoAction {
    #[command(about = "Encode text to NATO phonetic alphabet")]
    Encode {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Decode NATO phonetic alphabet to text")]
    Decode {
        #[arg(help = "NATO phonetic words (space-separated)")]
        input: String,
    },
}

pub fn run(action: NatoAction) -> Result<()> {
    match action {
        NatoAction::Encode { input } => {
            println!("{}", encode(&input));
        }
        NatoAction::Decode { input } => {
            println!("{}", decode(&input)?);
        }
    }
    Ok(())
}

const NATO_TABLE: &[(char, &str)] = &[
    ('A', "ALFA"),
    ('B', "BRAVO"),
    ('C', "CHARLIE"),
    ('D', "DELTA"),
    ('E', "ECHO"),
    ('F', "FOXTROT"),
    ('G', "GOLF"),
    ('H', "HOTEL"),
    ('I', "INDIA"),
    ('J', "JULIET"),
    ('K', "KILO"),
    ('L', "LIMA"),
    ('M', "MIKE"),
    ('N', "NOVEMBER"),
    ('O', "OSCAR"),
    ('P', "PAPA"),
    ('Q', "QUEBEC"),
    ('R', "ROMEO"),
    ('S', "SIERRA"),
    ('T', "TANGO"),
    ('U', "UNIFORM"),
    ('V', "VICTOR"),
    ('W', "WHISKEY"),
    ('X', "XRAY"),
    ('Y', "YANKEE"),
    ('Z', "ZULU"),
    ('0', "ZERO"),
    ('1', "ONE"),
    ('2', "TWO"),
    ('3', "THREE"),
    ('4', "FOUR"),
    ('5', "FIVE"),
    ('6', "SIX"),
    ('7', "SEVEN"),
    ('8', "EIGHT"),
    ('9', "NINE"),
];

pub fn encode(input: &str) -> String {
    input
        .to_uppercase()
        .chars()
        .filter_map(|c| {
            NATO_TABLE
                .iter()
                .find(|&&(ch, _)| ch == c)
                .map(|&(_, word)| word)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn decode(input: &str) -> Result<String> {
    if input.trim().is_empty() {
        return Ok(String::new());
    }

    input
        .split_whitespace()
        .map(|word| {
            let upper = word.to_uppercase();
            NATO_TABLE
                .iter()
                .find(|&&(_, nato)| nato == upper)
                .map(|&(c, _)| c)
                .context(format!("Unknown NATO word: {}", word))
        })
        .collect::<Result<String>>()
}

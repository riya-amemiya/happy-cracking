use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum EntropyAction {
    #[command(about = "Calculate Shannon entropy of input")]
    Analyze {
        #[arg(help = "Input text")]
        input: String,
    },
}

pub fn run(action: EntropyAction) -> Result<()> {
    match action {
        EntropyAction::Analyze { input } => {
            let entropy = calculate(&input);
            let classification = classify(entropy);
            println!("Entropy: {entropy:.4} bits/byte");
            println!("Classification: {classification}");
        }
    }
    Ok(())
}

// H = -Σ p(x) * log2(p(x)), maximum is 8.0 bits/byte.
#[must_use]
pub fn calculate(input: &str) -> f64 {
    if input.is_empty() {
        return 0.0;
    }

    let bytes = input.as_bytes();
    let len = bytes.len() as f64;

    let mut counts = [0usize; 256];
    for &b in bytes {
        counts[b as usize] += 1;
    }

    let mut entropy = 0.0;
    for &count in &counts {
        if count > 0 {
            let p = count as f64 / len;
            entropy -= p * p.log2();
        }
    }

    entropy
}

#[must_use]
pub fn classify(entropy: f64) -> &'static str {
    if entropy >= 7.5 {
        "High (likely encrypted/compressed)"
    } else if entropy >= 4.0 {
        "Medium (natural language/code)"
    } else {
        "Low (low entropy data)"
    }
}

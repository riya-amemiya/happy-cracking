use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum PrimesAction {
    #[command(about = "Factorize a number into prime factors")]
    Factorize {
        #[arg(help = "Number to factorize")]
        n: String,
    },
    #[command(about = "Check if a number is prime")]
    Isprime {
        #[arg(help = "Number to test")]
        n: String,
    },
}

pub fn run(action: PrimesAction) -> Result<()> {
    match action {
        PrimesAction::Factorize { n } => {
            let n: u128 = n.parse().map_err(|_| anyhow::anyhow!("Invalid number"))?;
            let factors = factorize(n);
            println!("{}", format_factors(&factors));
        }
        PrimesAction::Isprime { n } => {
            let n: u128 = n.parse().map_err(|_| anyhow::anyhow!("Invalid number"))?;
            if is_prime(n) {
                println!("{} is prime", n);
            } else {
                println!("{} is not prime", n);
            }
        }
    }
    Ok(())
}

// Trial division for prime factorization.
// Returns a list of (prime, exponent) pairs.
pub fn factorize(mut n: u128) -> Vec<(u128, u32)> {
    let mut factors = Vec::new();

    if n <= 1 {
        return factors;
    }

    // Optimization: Handle 2 separately to skip all even numbers in the loop
    if n.is_multiple_of(2) {
        let mut count = 0_u32;
        while n.is_multiple_of(2) {
            count += 1;
            n /= 2;
        }
        factors.push((2, count));
    }

    let mut d = 3_u128;
    while d * d <= n {
        let mut count = 0_u32;
        while n.is_multiple_of(d) {
            count += 1;
            n /= d;
        }
        if count > 0 {
            factors.push((d, count));
        }
        d += 2;
    }

    if n > 1 {
        factors.push((n, 1));
    }

    factors
}

// Trial division primality test.
pub fn is_prime(n: u128) -> bool {
    if n < 2 {
        return false;
    }
    if n < 4 {
        return true;
    }
    if n.is_multiple_of(2) || n.is_multiple_of(3) {
        return false;
    }
    let mut i = 5_u128;
    while i * i <= n {
        if n.is_multiple_of(i) || n.is_multiple_of(i + 2) {
            return false;
        }
        i += 6;
    }
    true
}

// Format factorization result as "2^2 × 3 × 7".
pub fn format_factors(factors: &[(u128, u32)]) -> String {
    if factors.is_empty() {
        return "1".to_string();
    }

    factors
        .iter()
        .map(|(p, e)| {
            if *e == 1 {
                p.to_string()
            } else {
                format!("{}^{}", p, e)
            }
        })
        .collect::<Vec<_>>()
        .join(" × ")
}

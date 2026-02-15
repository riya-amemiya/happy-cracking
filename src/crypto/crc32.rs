use anyhow::Result;
use clap::Subcommand;
use std::sync::LazyLock;

#[derive(Subcommand)]
pub enum Crc32Action {
    #[command(about = "Compute CRC32 checksum")]
    Compute {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Verify CRC32 checksum")]
    Verify {
        #[arg(help = "Input text")]
        input: String,
        #[arg(long, help = "Expected CRC32 hex checksum")]
        checksum: String,
    },
}

pub fn run(action: Crc32Action) -> Result<()> {
    match action {
        Crc32Action::Compute { input } => {
            println!("{}", compute(&input));
        }
        Crc32Action::Verify { input, checksum } => {
            let computed = compute(&input);
            if computed == checksum.to_lowercase() {
                println!("Match");
            } else {
                println!("Mismatch (expected {}, got {})", checksum, computed);
            }
        }
    }
    Ok(())
}

static CRC32_TABLE: LazyLock<[u32; 256]> = LazyLock::new(|| {
    let mut table = [0u32; 256];
    for i in 0..256u32 {
        let mut crc = i;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
        }
        table[i as usize] = crc;
    }
    table
});

pub fn compute_bytes(data: &[u8]) -> u32 {
    let table = &*CRC32_TABLE;
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        let index = ((crc ^ u32::from(byte)) & 0xFF) as usize;
        crc = (crc >> 8) ^ table[index];
    }
    crc ^ 0xFFFF_FFFF
}

pub fn compute(input: &str) -> String {
    format!("{:08x}", compute_bytes(input.as_bytes()))
}

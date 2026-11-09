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
    let input = match &action {
        Crc32Action::Compute { input } | Crc32Action::Verify { input, .. } => input.as_str(),
    };
    check_crc32_input_len(input, MAX_CRC32_BYTES)?;
    match action {
        Crc32Action::Compute { input } => {
            println!("{}", compute(&input));
        }
        Crc32Action::Verify { input, checksum } => {
            let expected = normalize_crc32_checksum(&checksum)?;
            let computed = compute(&input);
            if computed == expected {
                println!("Match");
            } else {
                println!("Mismatch (expected {checksum}, got {computed})");
            }
        }
    }
    Ok(())
}

pub const MAX_CRC32_BYTES: usize = 16 * 1024 * 1024;

pub fn check_crc32_input_len(input: &str, max_bytes: usize) -> Result<()> {
    let max_bytes = max_bytes.min(MAX_CRC32_BYTES);
    if input.len() > max_bytes {
        anyhow::bail!(
            "Input exceeds maximum size of {max_bytes} bytes to prevent Denial of Service"
        );
    }
    Ok(())
}

pub const MAX_CRC32_CHECKSUM_HEX_LEN: usize = 8;

pub fn normalize_crc32_checksum(checksum: &str) -> Result<String> {
    if checksum.len() > MAX_CRC32_CHECKSUM_HEX_LEN {
        anyhow::bail!("Checksum exceeds maximum length of {MAX_CRC32_CHECKSUM_HEX_LEN}");
    }
    Ok(checksum.to_ascii_lowercase())
}

static CRC32_SLICE8: LazyLock<[[u32; 256]; 8]> = LazyLock::new(|| {
    let mut tables = [[0u32; 256]; 8];
    for (i, slot) in tables[0].iter_mut().enumerate() {
        let mut crc = i as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
        *slot = crc;
    }
    let t0 = tables[0];
    for i in 0..256 {
        let mut crc = t0[i];
        for table in tables.iter_mut().skip(1) {
            crc = (crc >> 8) ^ t0[(crc & 0xFF) as usize];
            table[i] = crc;
        }
    }
    tables
});

#[must_use]
pub fn compute_bytes(data: &[u8]) -> u32 {
    let tables = &*CRC32_SLICE8;
    let mut crc = 0xFFFF_FFFFu32;
    let (chunks, rest) = data.as_chunks::<8>();
    for chunk in chunks {
        let [b0, b1, b2, b3, b4, b5, b6, b7] = *chunk;
        let lo = crc ^ u32::from_le_bytes([b0, b1, b2, b3]);
        let hi = u32::from_le_bytes([b4, b5, b6, b7]);
        crc = tables[7][(lo & 0xFF) as usize]
            ^ tables[6][((lo >> 8) & 0xFF) as usize]
            ^ tables[5][((lo >> 16) & 0xFF) as usize]
            ^ tables[4][(lo >> 24) as usize]
            ^ tables[3][(hi & 0xFF) as usize]
            ^ tables[2][((hi >> 8) & 0xFF) as usize]
            ^ tables[1][((hi >> 16) & 0xFF) as usize]
            ^ tables[0][(hi >> 24) as usize];
    }
    let (words, tail) = rest.as_chunks::<4>();
    for word in words {
        let v = crc ^ u32::from_le_bytes(*word);
        crc = tables[3][(v & 0xFF) as usize]
            ^ tables[2][((v >> 8) & 0xFF) as usize]
            ^ tables[1][((v >> 16) & 0xFF) as usize]
            ^ tables[0][(v >> 24) as usize];
    }
    for &byte in tail {
        crc = (crc >> 8) ^ tables[0][((crc ^ u32::from(byte)) & 0xFF) as usize];
    }
    crc ^ 0xFFFF_FFFF
}

#[must_use]
pub fn compute(input: &str) -> String {
    format!("{:08x}", compute_bytes(input.as_bytes()))
}

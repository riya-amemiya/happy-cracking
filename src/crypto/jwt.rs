use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use clap::Subcommand;

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
    }
    Ok(())
}

pub struct JwtParts {
    pub header: String,
    pub payload: String,
    pub signature_hex: String,
}

pub fn decode(token: &str) -> Result<JwtParts> {
    let token = token.trim();
    let segments: Vec<&str> = token.split('.').collect();
    if segments.len() != 3 {
        anyhow::bail!(
            "Invalid JWT format: expected 3 dot-separated parts, got {}",
            segments.len()
        );
    }

    let header_bytes = URL_SAFE_NO_PAD
        .decode(segments[0])
        .context("Failed to decode JWT header (invalid base64url)")?;
    let header = String::from_utf8(header_bytes).context("JWT header is not valid UTF-8")?;

    let payload_bytes = URL_SAFE_NO_PAD
        .decode(segments[1])
        .context("Failed to decode JWT payload (invalid base64url)")?;
    let payload = String::from_utf8(payload_bytes).context("JWT payload is not valid UTF-8")?;

    let sig_bytes = URL_SAFE_NO_PAD
        .decode(segments[2])
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

    // Parse JSON safely using serde_json
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

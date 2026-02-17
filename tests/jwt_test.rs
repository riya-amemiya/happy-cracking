use happy_cracking::crypto::jwt;

// Test token: {"alg":"HS256","typ":"JWT"}.{"sub":"1234567890","name":"John Doe","iat":1516239022}.signature
// Generated from jwt.io with secret "secret"
const TEST_TOKEN: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";

#[test]
fn decode_valid_jwt() {
    let parts = jwt::decode(TEST_TOKEN).unwrap();
    assert!(parts.header.contains("HS256"));
    assert!(parts.header.contains("JWT"));
    assert!(parts.payload.contains("1234567890"));
    assert!(parts.payload.contains("John Doe"));
}

#[test]
fn decode_header_content() {
    let parts = jwt::decode(TEST_TOKEN).unwrap();
    assert_eq!(parts.header, r#"{"alg":"HS256","typ":"JWT"}"#);
}

#[test]
fn decode_payload_content() {
    let parts = jwt::decode(TEST_TOKEN).unwrap();
    assert_eq!(
        parts.payload,
        r#"{"sub":"1234567890","name":"John Doe","iat":1516239022}"#
    );
}

#[test]
fn decode_signature_is_hex() {
    let parts = jwt::decode(TEST_TOKEN).unwrap();
    assert!(parts.signature_hex.chars().all(|c| c.is_ascii_hexdigit()));
    assert!(!parts.signature_hex.is_empty());
}

#[test]
fn decode_invalid_format_no_dots() {
    assert!(jwt::decode("nodots").is_err());
}

#[test]
fn decode_invalid_format_one_dot() {
    assert!(jwt::decode("one.dot").is_err());
}

#[test]
fn decode_invalid_format_four_dots() {
    assert!(jwt::decode("a.b.c.d").is_err());
}

#[test]
fn decode_invalid_base64url() {
    assert!(jwt::decode("!!!.!!!.!!!").is_err());
}

#[test]
fn extract_algorithm_hs256() {
    let header = r#"{"alg":"HS256","typ":"JWT"}"#;
    assert_eq!(jwt::extract_algorithm(header), "HS256");
}

#[test]
fn extract_algorithm_rs256() {
    let header = r#"{"alg":"RS256","typ":"JWT"}"#;
    assert_eq!(jwt::extract_algorithm(header), "RS256");
}

#[test]
fn extract_algorithm_none() {
    let header = r#"{"alg":"none","typ":"JWT"}"#;
    assert_eq!(jwt::extract_algorithm(header), "none");
}

#[test]
fn extract_algorithm_missing() {
    let header = r#"{"typ":"JWT"}"#;
    assert_eq!(jwt::extract_algorithm(header), "unknown");
}

#[test]
fn find_vulnerabilities_alg_none() {
    let header = r#"{"alg":"none","typ":"JWT"}"#;
    let warnings = jwt::find_vulnerabilities(header);
    assert!(warnings.iter().any(|w| w.contains("none")));
}

#[test]
fn find_vulnerabilities_hs256_confusion() {
    let header = r#"{"alg":"HS256","typ":"JWT"}"#;
    let warnings = jwt::find_vulnerabilities(header);
    assert!(warnings.iter().any(|w| w.contains("algorithm confusion")));
}

#[test]
fn find_vulnerabilities_rs256_clean() {
    let header = r#"{"alg":"RS256","typ":"JWT"}"#;
    let warnings = jwt::find_vulnerabilities(header);
    assert!(warnings.is_empty());
}

#[test]
fn find_vulnerabilities_jku_header() {
    let header = r#"{"alg":"RS256","jku":"https://evil.com/jwks.json"}"#;
    let warnings = jwt::find_vulnerabilities(header);
    assert!(warnings.iter().any(|w| w.contains("jku")));
}

#[test]
fn find_vulnerabilities_kid_header() {
    let header = r#"{"alg":"RS256","kid":"../../etc/passwd"}"#;
    let warnings = jwt::find_vulnerabilities(header);
    assert!(warnings.iter().any(|w| w.contains("kid")));
}

#[test]
fn find_vulnerabilities_jwk_header() {
    let header = r#"{"alg":"RS256","jwk":"{}"}"#;
    let warnings = jwt::find_vulnerabilities(header);
    assert!(warnings.iter().any(|w| w.contains("jwk")));
}

// CTF-style token with flag in payload
#[test]
fn decode_ctf_token() {
    // Header: {"alg":"none","typ":"JWT"}
    // Payload: {"flag":"flag{jwt_d3c0d3d}","admin":true}
    // Signature: empty
    let token = "eyJhbGciOiJub25lIiwidHlwIjoiSldUIn0.eyJmbGFnIjoiZmxhZ3tqd3RfZDNjMGQzZH0iLCJhZG1pbiI6dHJ1ZX0.";
    let parts = jwt::decode(token).unwrap();
    assert!(parts.payload.contains("flag{jwt_d3c0d3d}"));
    assert_eq!(jwt::extract_algorithm(&parts.header), "none");
}

#[test]
fn extract_algorithm_security_cases() {
    // Test for parser robustness against shadowing and confusing inputs

    // Case 1: "alg" inside a comment/string value
    let header_1 = r#"{"comment": "fake \"alg\": \"HS256\"", "alg": "none"}"#;
    assert_eq!(jwt::extract_algorithm(header_1), "none");

    // Case 2: "alg" shadowed by value in preceding field
    let header_2 = r#"{"foo": "alg", "alg": "none"}"#;
    assert_eq!(jwt::extract_algorithm(header_2), "none");

    // Case 3: "alg" shadowed by similar key prefix
    let header_3 = r#"{"algo": "foo", "alg": "none"}"#;
    assert_eq!(jwt::extract_algorithm(header_3), "none");
}

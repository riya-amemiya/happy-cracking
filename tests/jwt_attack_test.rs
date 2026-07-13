use happy_cracking::crypto::jwt::{
    crack_hmac_secret_list, decode, forge_alg_confusion, forge_none, verify_hs,
};

// HS256 token for header/payload from the classic jwt.io example, signed with secret "secret"
// (the historical jwt.io signature bytes do not match HMAC-SHA256("secret"); this one does)
const TEST_TOKEN: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.XbPfbIHMI6arZ3Y922BhjWgQzWXcXNrz0ogtVhfEd2o";

#[test]
fn verify_hs_accepts_correct_secret() {
    assert!(verify_hs(TEST_TOKEN, b"secret").unwrap());
    assert!(!verify_hs(TEST_TOKEN, b"wrong").unwrap());
}

#[test]
fn crack_finds_secret_in_list() {
    let found =
        crack_hmac_secret_list(TEST_TOKEN, &["password", "admin", "secret", "test"]).unwrap();
    assert_eq!(found.as_deref(), Some("secret"));
}

#[test]
fn crack_misses_absent_secret() {
    let found = crack_hmac_secret_list(TEST_TOKEN, &["nope", "nada"]).unwrap();
    assert!(found.is_none());
}

#[test]
fn forge_none_produces_decodable_token() {
    let forged = forge_none(r#"{"sub":"admin","role":"root"}"#).unwrap();
    assert!(forged.ends_with('.'));
    let parts = decode(&forged).unwrap();
    assert!(parts.header.to_ascii_lowercase().contains("none"));
    assert!(parts.payload.contains("admin"));
    assert!(parts.signature_hex.is_empty() || parts.signature_hex == "");
}

#[test]
fn forge_alg_confusion_hs256() {
    let forged = forge_alg_confusion(TEST_TOKEN, b"public-key-material", "HS256").unwrap();
    assert!(verify_hs(&forged, b"public-key-material").unwrap());
    let parts = decode(&forged).unwrap();
    assert!(parts.header.contains("HS256"));
    assert!(parts.payload.contains("John Doe"));
}

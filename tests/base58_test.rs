use happy_cracking::crypto::base58::{self, Base58Action, MAX_BASE58_BYTES};

#[test]
fn test_encode_basic() {
    assert_eq!(base58::encode("Hello"), "9Ajdvzr");
}

#[test]
fn test_decode_basic() {
    assert_eq!(base58::decode("9Ajdvzr").unwrap(), "Hello");
}

#[test]
fn test_roundtrip() {
    let original = "Hello World!";
    let encoded = base58::encode(original);
    let decoded = base58::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn test_empty_string() {
    assert_eq!(base58::encode(""), "");
    assert_eq!(base58::decode("").unwrap(), "");
}

#[test]
fn test_roundtrip_flag() {
    let original = "flag{base58_encoding}";
    let encoded = base58::encode(original);
    let decoded = base58::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn test_leading_zero_bytes() {
    assert_eq!(base58::encode("\0"), "1");
    assert_eq!(base58::encode("\0\0"), "11");
    assert_eq!(base58::decode("1").unwrap(), "\0");
    assert_eq!(base58::decode("11").unwrap(), "\0\0");
}

#[test]
fn test_decode_trims_whitespace() {
    assert_eq!(base58::decode("  9Ajdvzr\n").unwrap(), "Hello");
}

#[test]
fn test_decode_invalid_char() {
    assert!(base58::decode("0OIl").is_err());
    assert!(base58::decode("!!!").is_err());
}

#[test]
fn test_roundtrip_long() {
    let original = "The quick brown fox jumps over the lazy dog";
    let encoded = base58::encode(original);
    let decoded = base58::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn decode_rejects_oversized_input() {
    let start = std::time::Instant::now();
    let too_long = "2".repeat(MAX_BASE58_BYTES + 1);
    let err = base58::decode(&too_long).unwrap_err().to_string();
    assert!(
        err.contains(&MAX_BASE58_BYTES.to_string()),
        "unexpected error: {err}"
    );
    assert!(
        start.elapsed().as_millis() < 100,
        "oversized base58 input must be rejected before BigUint conversion"
    );
}

#[test]
fn decode_accepts_input_at_length_cap() {
    let input = "1".repeat(MAX_BASE58_BYTES);
    let decoded = base58::decode(&input).unwrap();
    assert_eq!(decoded.len(), MAX_BASE58_BYTES);
    assert!(decoded.bytes().all(|b| b == 0));
}

#[test]
fn decode_trims_before_length_check() {
    let padded = format!("{}9Ajdvzr{}", " ".repeat(MAX_BASE58_BYTES), " ".repeat(8));
    assert_eq!(base58::decode(&padded).unwrap(), "Hello");
}

#[test]
fn run_rejects_oversized_encode_input() {
    let start = std::time::Instant::now();
    let err = base58::run(Base58Action::Encode {
        input: "a".repeat(MAX_BASE58_BYTES + 1),
    })
    .unwrap_err()
    .to_string();
    assert!(
        err.contains(&MAX_BASE58_BYTES.to_string()),
        "unexpected error: {err}"
    );
    assert!(
        start.elapsed().as_millis() < 100,
        "oversized base58 encode must be rejected before BigUint conversion"
    );
}

#[test]
fn run_rejects_oversized_decode_input() {
    let err = base58::run(Base58Action::Decode {
        input: "2".repeat(MAX_BASE58_BYTES + 1),
    })
    .unwrap_err()
    .to_string();
    assert!(err.contains(&MAX_BASE58_BYTES.to_string()));
}

#[test]
fn check_base58_input_len_accepts_limit() {
    base58::check_base58_input_len(&"a".repeat(MAX_BASE58_BYTES)).unwrap();
    base58::check_base58_input_len("").unwrap();
}

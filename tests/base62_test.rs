use happy_cracking::crypto::base62::{self, Base62Action, MAX_BASE62_BYTES};

#[test]
fn test_encode_basic() {
    let encoded = base62::encode("Hello");
    assert!(!encoded.is_empty());
    assert!(encoded.chars().all(|c| c.is_ascii_alphanumeric()));
}

#[test]
fn test_decode_basic() {
    let encoded = base62::encode("Hello");
    let decoded = base62::decode(&encoded).unwrap();
    assert_eq!(decoded, "Hello");
}

#[test]
fn test_roundtrip() {
    let original = "Hello, World!";
    let encoded = base62::encode(original);
    let decoded = base62::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn test_roundtrip_long() {
    let original = "The quick brown fox jumps over the lazy dog";
    let encoded = base62::encode(original);
    let decoded = base62::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn test_empty_string() {
    assert_eq!(base62::encode(""), "");
    assert_eq!(base62::decode("").unwrap(), "");
}

#[test]
fn test_ctf_flag() {
    let original = "flag{base62_encoding}";
    let encoded = base62::encode(original);
    let decoded = base62::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn test_single_char() {
    let original = "A";
    let encoded = base62::encode(original);
    let decoded = base62::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn test_only_alphanumeric_output() {
    let encoded = base62::encode("test data 123!@#");
    assert!(encoded.chars().all(|c| c.is_ascii_alphanumeric()));
}

#[test]
fn test_decode_invalid_char() {
    assert!(base62::decode("!!!").is_err());
}

#[test]
fn test_binary_data_roundtrip() {
    let original = "\x01\x02\x03";
    let encoded = base62::encode(original);
    let decoded = base62::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn decode_rejects_oversized_input() {
    let start = std::time::Instant::now();
    let too_long = "2".repeat(MAX_BASE62_BYTES + 1);
    let err = base62::decode(&too_long).unwrap_err().to_string();
    assert!(
        err.contains(&MAX_BASE62_BYTES.to_string()),
        "unexpected error: {err}"
    );
    assert!(
        start.elapsed().as_millis() < 100,
        "oversized base62 input must be rejected before BigUint conversion"
    );
}

#[test]
fn decode_accepts_input_at_length_cap() {
    base62::decode(&"0".repeat(MAX_BASE62_BYTES)).unwrap();
}

#[test]
fn decode_trims_before_length_check() {
    let encoded = base62::encode("Hello");
    let padded = format!("{}{encoded}{}", " ".repeat(MAX_BASE62_BYTES), " ".repeat(8));
    assert_eq!(base62::decode(&padded).unwrap(), "Hello");
}

#[test]
fn run_rejects_oversized_encode_input() {
    let start = std::time::Instant::now();
    let err = base62::run(Base62Action::Encode {
        input: "a".repeat(MAX_BASE62_BYTES + 1),
    })
    .unwrap_err()
    .to_string();
    assert!(
        err.contains(&MAX_BASE62_BYTES.to_string()),
        "unexpected error: {err}"
    );
    assert!(
        start.elapsed().as_millis() < 100,
        "oversized base62 encode must be rejected before BigUint conversion"
    );
}

#[test]
fn run_rejects_oversized_decode_input() {
    let err = base62::run(Base62Action::Decode {
        input: "2".repeat(MAX_BASE62_BYTES + 1),
    })
    .unwrap_err()
    .to_string();
    assert!(err.contains(&MAX_BASE62_BYTES.to_string()));
}

#[test]
fn check_base62_input_len_accepts_limit() {
    base62::check_base62_input_len(&"a".repeat(MAX_BASE62_BYTES)).unwrap();
    base62::check_base62_input_len("").unwrap();
}

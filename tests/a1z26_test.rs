use happy_cracking::crypto::a1z26::{self, A1z26Action, MAX_A1Z26_DIGITS};

#[test]
fn encode_empty_string() {
    assert_eq!(a1z26::encode(""), "");
}

#[test]
fn encode_hello() {
    assert_eq!(a1z26::encode("HELLO"), "8-5-12-12-15");
}

#[test]
fn encode_lowercase() {
    assert_eq!(a1z26::encode("hello"), "8-5-12-12-15");
}

#[test]
fn encode_with_spaces() {
    assert_eq!(a1z26::encode("HI THERE"), "8-9 20-8-5-18-5");
}

#[test]
fn encode_with_numbers() {
    assert_eq!(a1z26::encode("A1B2"), "1122");
}

#[test]
fn encode_single_char() {
    assert_eq!(a1z26::encode("A"), "1");
    assert_eq!(a1z26::encode("Z"), "26");
}

#[test]
fn decode_empty_string() {
    assert_eq!(a1z26::decode("").unwrap(), "");
}

#[test]
fn decode_hello() {
    assert_eq!(a1z26::decode("8-5-12-12-15").unwrap(), "HELLO");
}

#[test]
fn decode_with_spaces() {
    assert_eq!(a1z26::decode("8 5 12 12 15").unwrap(), "HELLO");
}

#[test]
fn decode_with_commas() {
    assert_eq!(a1z26::decode("8,5,12,12,15").unwrap(), "HELLO");
}

#[test]
fn decode_single_number() {
    assert_eq!(a1z26::decode("1").unwrap(), "A");
    assert_eq!(a1z26::decode("26").unwrap(), "Z");
}

#[test]
fn decode_out_of_range() {
    assert!(a1z26::decode("27").is_err());
    assert!(a1z26::decode("0").is_err());
}

#[test]
fn roundtrip() {
    let original = "HELLO";
    let encoded = a1z26::encode(original);
    let decoded = a1z26::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn roundtrip_flag() {
    let original = "FLAG";
    let encoded = a1z26::encode(original);
    assert_eq!(encoded, "6-12-1-7");
    let decoded = a1z26::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn decode_rejects_oversized_digit_run() {
    let start = std::time::Instant::now();
    let too_long = "9".repeat(MAX_A1Z26_DIGITS + 1);
    let err = a1z26::decode(&too_long).unwrap_err().to_string();
    assert!(
        err.contains(&MAX_A1Z26_DIGITS.to_string()),
        "unexpected error: {err}"
    );
    assert!(
        start.elapsed().as_millis() < 100,
        "oversized a1z26 digit run must be rejected before integer parse"
    );
}

#[test]
fn decode_rejects_leading_zero_padding_beyond_digit_cap() {
    let start = std::time::Instant::now();
    let padded = format!("{}26", "0".repeat(64 * 1024));
    let err = a1z26::decode(&padded).unwrap_err().to_string();
    assert!(
        err.contains(&MAX_A1Z26_DIGITS.to_string()),
        "unexpected error: {err}"
    );
    assert!(
        start.elapsed().as_millis() < 100,
        "padded a1z26 digit run must be rejected before integer parse"
    );
}

#[test]
fn decode_accepts_two_digit_values() {
    assert_eq!(a1z26::decode("26").unwrap(), "Z");
    assert_eq!(a1z26::decode("01").unwrap(), "A");
}

#[test]
fn run_rejects_oversized_decode_input() {
    let err = a1z26::run(A1z26Action::Decode {
        input: "9".repeat(MAX_A1Z26_DIGITS + 1),
    })
    .unwrap_err()
    .to_string();
    assert!(err.contains(&MAX_A1Z26_DIGITS.to_string()));
}

#[test]
fn check_a1z26_digits_accepts_limit() {
    a1z26::check_a1z26_digits(&"9".repeat(MAX_A1Z26_DIGITS)).unwrap();
    a1z26::check_a1z26_digits("").unwrap();
}

use happy_cracking::crypto::crc32::{self, Crc32Action, MAX_CRC32_CHECKSUM_HEX_LEN};

#[test]
fn test_known_vector() {
    assert_eq!(crc32::compute("123456789"), "cbf43926");
}

#[test]
fn test_compute_bytes() {
    assert_eq!(crc32::compute_bytes(b"123456789"), 0xCBF43926);
}

#[test]
fn compute_bytes_matches_bytewise_across_remainders() {
    for len in 0..=24 {
        let data: Vec<u8> = (0..len).map(|i| (i * 31) as u8).collect();
        assert_eq!(
            crc32::compute_bytes(&data),
            happy_cracking::crypto::crc32_forge::crc32_compute(&data),
            "len={len}"
        );
    }
}

#[test]
fn test_empty_string() {
    assert_eq!(crc32::compute(""), "00000000");
}

#[test]
fn test_hello() {
    let result = crc32::compute("Hello");
    assert_eq!(result.len(), 8);
    assert!(result.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn test_single_char() {
    let result = crc32::compute("a");
    assert_eq!(result.len(), 8);
}

#[test]
fn test_flag_format() {
    let checksum = crc32::compute("flag{crc32_test}");
    assert_eq!(checksum.len(), 8);
    assert_eq!(crc32::compute("flag{crc32_test}"), checksum);
}

#[test]
fn test_deterministic() {
    let a = crc32::compute("test data");
    let b = crc32::compute("test data");
    assert_eq!(a, b);
}

#[test]
fn test_different_inputs_differ() {
    assert_ne!(crc32::compute("abc"), crc32::compute("abd"));
}

#[test]
fn normalize_crc32_checksum_rejects_oversized() {
    let start = std::time::Instant::now();
    let err = crc32::normalize_crc32_checksum(&"a".repeat(MAX_CRC32_CHECKSUM_HEX_LEN + 1))
        .unwrap_err()
        .to_string();
    assert!(
        err.contains(&MAX_CRC32_CHECKSUM_HEX_LEN.to_string()),
        "unexpected error: {err}"
    );
    assert!(
        start.elapsed().as_millis() < 100,
        "oversized checksum must be rejected before case folding"
    );
}

#[test]
fn normalize_crc32_checksum_accepts_len_at_limit() {
    let checksum = crc32::normalize_crc32_checksum("CBF43926").unwrap();
    assert_eq!(checksum, "cbf43926");
}

#[test]
fn normalize_crc32_checksum_accepts_shorter() {
    let checksum = crc32::normalize_crc32_checksum("abc").unwrap();
    assert_eq!(checksum, "abc");
}

#[test]
fn normalize_crc32_checksum_accepts_empty() {
    let checksum = crc32::normalize_crc32_checksum("").unwrap();
    assert!(checksum.is_empty());
}

#[test]
fn verify_run_match_and_mismatch() {
    crc32::run(Crc32Action::Verify {
        input: "123456789".into(),
        checksum: "CBF43926".into(),
    })
    .unwrap();
    crc32::run(Crc32Action::Verify {
        input: "123456789".into(),
        checksum: "00000000".into(),
    })
    .unwrap();
}

#[test]
fn verify_run_rejects_oversized_checksum() {
    let start = std::time::Instant::now();
    let err = crc32::run(Crc32Action::Verify {
        input: "hello".into(),
        checksum: "a".repeat(MAX_CRC32_CHECKSUM_HEX_LEN + 1),
    })
    .unwrap_err()
    .to_string();
    assert!(
        err.contains(&MAX_CRC32_CHECKSUM_HEX_LEN.to_string()),
        "unexpected error: {err}"
    );
    assert!(
        start.elapsed().as_millis() < 100,
        "oversized checksum must be rejected before CRC32 compare"
    );
}

#[test]
fn check_crc32_input_len_rejects_oversized() {
    let start = std::time::Instant::now();
    let err = crc32::check_crc32_input_len("abcd", 2).unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
    assert!(
        start.elapsed().as_millis() < 100,
        "oversized crc32 input must be rejected before checksum computation"
    );
}

#[test]
fn check_crc32_input_len_accepts_len_at_limit() {
    crc32::check_crc32_input_len("ab", 2).unwrap();
}

#[test]
fn check_crc32_input_len_accepts_empty() {
    crc32::check_crc32_input_len("", 16).unwrap();
}

#[test]
fn run_rejects_oversized_crc32_input() {
    let start = std::time::Instant::now();
    let err = crc32::run(Crc32Action::Compute {
        input: "a".repeat(crc32::MAX_CRC32_BYTES + 1),
    })
    .unwrap_err()
    .to_string();
    assert!(err.contains("Denial of Service"), "unexpected error: {err}");
    assert!(
        start.elapsed().as_millis() < 100,
        "oversized crc32 CLI input must be rejected before checksum computation"
    );
}

#[test]
fn verify_run_rejects_oversized_crc32_input() {
    let start = std::time::Instant::now();
    let err = crc32::run(Crc32Action::Verify {
        input: "a".repeat(crc32::MAX_CRC32_BYTES + 1),
        checksum: "cbf43926".into(),
    })
    .unwrap_err()
    .to_string();
    assert!(err.contains("Denial of Service"), "unexpected error: {err}");
    assert!(
        start.elapsed().as_millis() < 100,
        "oversized crc32 verify input must be rejected before checksum compare"
    );
}

#[test]
fn run_computes_under_limit() {
    crc32::run(Crc32Action::Compute {
        input: "123456789".into(),
    })
    .unwrap();
}

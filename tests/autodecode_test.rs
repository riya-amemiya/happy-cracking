use happy_cracking::crypto::autodecode;

#[test]
fn test_detect_base64() {
    let results = autodecode::detect_and_decode("SGVsbG8gV29ybGQ=");
    assert!(
        results
            .iter()
            .any(|(enc, dec)| *enc == "Base64" && dec == "Hello World")
    );
}

#[test]
fn test_detect_hex() {
    let results = autodecode::detect_and_decode("48656c6c6f");
    assert!(
        results
            .iter()
            .any(|(enc, dec)| *enc == "Hex" && dec == "Hello")
    );
}

#[test]
fn test_detect_url() {
    let results = autodecode::detect_and_decode("Hello%20World");
    assert!(
        results
            .iter()
            .any(|(enc, dec)| *enc == "URL" && dec == "Hello World")
    );
}

#[test]
fn test_detect_binary() {
    let results = autodecode::detect_and_decode("01001000 01101001");
    assert!(
        results
            .iter()
            .any(|(enc, dec)| *enc == "Binary" && dec == "Hi")
    );
}

#[test]
fn test_detect_morse() {
    let results = autodecode::detect_and_decode(".... ..");
    assert!(
        results
            .iter()
            .any(|(enc, dec)| *enc == "Morse" && dec == "HI")
    );
}

#[test]
fn test_check_decode_depth_rejects_excessive() {
    use std::time::Instant;

    let start = Instant::now();
    let res = autodecode::check_decode_depth(autodecode::MAX_DECODE_DEPTH + 1);
    assert!(res.is_err(), "Expected error for excessive decode depth");
    let err = res.unwrap_err().to_string();
    assert!(
        err.contains("exceeds the maximum allowed limit"),
        "unexpected error: {err}"
    );
    assert!(
        start.elapsed().as_millis() < 100,
        "depth check should fail immediately, not recurse"
    );
}

#[test]
fn test_recursive_run_rejects_excessive_max_depth() {
    let res = autodecode::run(autodecode::AutoDecodeAction::Decode {
        input: "SGVsbG8=".to_string(),
        recursive: true,
        max_depth: autodecode::MAX_DECODE_DEPTH + 1,
        aggressive: false,
        max_nodes: 32,
    });
    assert!(
        res.is_err(),
        "recursive --max-depth above the cap must fail closed"
    );
}

#[test]
fn test_check_decode_depth_accepts_limit() {
    autodecode::check_decode_depth(autodecode::MAX_DECODE_DEPTH).unwrap();
    autodecode::check_decode_depth(1).unwrap();
}

#[test]
fn test_recursive_run_accepts_capped_depth() {
    autodecode::run(autodecode::AutoDecodeAction::Decode {
        input: "SGVsbG8=".to_string(),
        recursive: true,
        max_depth: 5,
        aggressive: false,
        max_nodes: 32,
    })
    .unwrap();
}

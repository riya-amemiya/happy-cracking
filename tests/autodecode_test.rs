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

#[test]
fn recursive_node_cap_stops_at_one() {
    let nodes = autodecode::decode_recursive_with_limit("SGVsbG8=", 5, 1);
    assert_eq!(nodes, 1);
}

#[test]
fn recursive_node_cap_clamps_to_max_tree_nodes() {
    use std::time::Instant;

    let start = Instant::now();
    let nodes = autodecode::decode_recursive_with_limit(
        "SGVsbG8=",
        autodecode::MAX_DECODE_DEPTH,
        usize::MAX,
    );
    assert!(nodes >= 1);
    assert!(nodes <= autodecode::MAX_TREE_NODES);
    assert!(
        start.elapsed().as_millis() < 1000,
        "recursive decode must honor the node cap instead of expanding without bound"
    );
}

#[test]
fn recursive_run_completes_with_node_cap() {
    use std::time::Instant;

    let start = Instant::now();
    autodecode::run(autodecode::AutoDecodeAction::Decode {
        input: "SGVsbG8=".to_string(),
        recursive: true,
        max_depth: autodecode::MAX_DECODE_DEPTH,
        aggressive: false,
        max_nodes: 32,
    })
    .unwrap();
    assert!(
        start.elapsed().as_millis() < 1000,
        "recursive CLI path must finish under the node cap"
    );
}

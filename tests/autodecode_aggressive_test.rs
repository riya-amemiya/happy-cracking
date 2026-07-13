use happy_cracking::crypto::autodecode::{decode_tree, detect_and_decode, score_decode_candidate};

#[test]
fn decode_tree_unwraps_base64() {
    let results = decode_tree("ZmxhZ3t0ZXN0fQ==", 4, 16);
    assert!(
        results.iter().any(|(_, t)| t.contains("flag{")),
        "got {:?}",
        results
    );
}

#[test]
fn decode_tree_nested_hex_base64() {
    // "flag" -> base64 "ZmxhZw==" -> hex
    let inner = "ZmxhZw==";
    let hexed = hex::encode(inner.as_bytes());
    let results = decode_tree(&hexed, 5, 32);
    assert!(
        results
            .iter()
            .any(|(_, t)| t == "flag" || t.contains("flag")),
        "got {:?}",
        results
    );
}

#[test]
fn score_prefers_flag() {
    assert!(score_decode_candidate("flag{x}") > score_decode_candidate("::::"));
}

#[test]
fn detect_and_decode_still_works() {
    let r = detect_and_decode("aGVsbG8=");
    assert!(r.iter().any(|(n, v)| *n == "Base64" && v == "hello"));
}

use happy_cracking::crypto::chain;

#[test]
fn chain_single_base64_encode() {
    let result = chain::chain("Hello", "base64-encode").unwrap();
    assert_eq!(result, "SGVsbG8=");
}

#[test]
fn chain_single_rot13() {
    let result = chain::chain("Hello", "rot13").unwrap();
    assert_eq!(result, "Uryyb");
}

#[test]
fn chain_single_reverse() {
    let result = chain::chain("Hello", "reverse").unwrap();
    assert_eq!(result, "olleH");
}

#[test]
fn chain_single_upper() {
    let result = chain::chain("hello", "upper").unwrap();
    assert_eq!(result, "HELLO");
}

#[test]
fn chain_single_lower() {
    let result = chain::chain("HELLO", "lower").unwrap();
    assert_eq!(result, "hello");
}

#[test]
fn chain_base64_roundtrip() {
    let result = chain::chain("flag{chain}", "base64-encode,base64-decode").unwrap();
    assert_eq!(result, "flag{chain}");
}

#[test]
fn chain_hex_roundtrip() {
    let result = chain::chain("CTF", "hex-encode,hex-decode").unwrap();
    assert_eq!(result, "CTF");
}

#[test]
fn chain_multiple_operations() {
    let result = chain::chain("hello", "upper,base64-encode").unwrap();
    let expected = happy_cracking::crypto::base64::encode("HELLO");
    assert_eq!(result, expected);
}

#[test]
fn chain_rot13_twice_is_identity() {
    let result = chain::chain("Secret Message", "rot13,rot13").unwrap();
    assert_eq!(result, "Secret Message");
}

#[test]
fn chain_rot47_twice_is_identity() {
    let result = chain::chain("flag{rot47}", "rot47,rot47").unwrap();
    assert_eq!(result, "flag{rot47}");
}

#[test]
fn chain_unknown_operation_errors() {
    let result = chain::chain("test", "unknown-op");
    assert!(result.is_err());
}

#[test]
fn chain_empty_ops_returns_input() {
    let result = chain::chain("unchanged", "").unwrap();
    assert_eq!(result, "unchanged");
}

#[test]
fn chain_url_roundtrip() {
    let result = chain::chain("hello world&foo=bar", "url-encode,url-decode").unwrap();
    assert_eq!(result, "hello world&foo=bar");
}

#[test]
fn chain_binary_roundtrip() {
    let result = chain::chain("Hi", "binary-encode,binary-decode").unwrap();
    assert_eq!(result, "Hi");
}

#[test]
fn chain_base32_roundtrip() {
    let result = chain::chain("Test", "base32-encode,base32-decode").unwrap();
    assert_eq!(result, "Test");
}

#[test]
fn chain_too_many_operations() {
    let ops: Vec<&str> = (0..51).map(|_| "rot13").collect();
    let ops_str = ops.join(",");
    let result = chain::chain("test", &ops_str);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Too many operations")
    );
}

#[test]
fn chain_output_too_large() {
    // 2^20 bytes = 1MB
    let input = "A".repeat(1024 * 1024);
    // 6 hex-encodes: 1 -> 2 -> 4 -> 8 -> 16 -> 32 -> 64MB
    let ops = "hex-encode,hex-encode,hex-encode,hex-encode,hex-encode,hex-encode";
    let result = chain::chain(&input, ops);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Output size limit exceeded")
    );
}

use happy_cracking::crypto::semaphore;

#[test]
fn test_encode_basic() {
    assert_eq!(semaphore::encode("A").unwrap(), "1-2");
}

#[test]
fn test_encode_hello() {
    assert_eq!(semaphore::encode("HELLO").unwrap(), "2-3 1-6 3-2 3-2 3-6");
}

#[test]
fn test_encode_lowercase() {
    assert_eq!(semaphore::encode("abc").unwrap(), "1-2 1-3 1-4");
}

#[test]
fn test_decode_basic() {
    assert_eq!(semaphore::decode("1-2").unwrap(), "A");
}

#[test]
fn test_decode_hello() {
    assert_eq!(semaphore::decode("2-3 1-6 3-2 3-2 3-6").unwrap(), "HELLO");
}

#[test]
fn test_roundtrip() {
    let original = "HELLOWORLD";
    let encoded = semaphore::encode(original).unwrap();
    let decoded = semaphore::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn test_roundtrip_alphabet() {
    let alpha = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let encoded = semaphore::encode(alpha).unwrap();
    let decoded = semaphore::decode(&encoded).unwrap();
    assert_eq!(decoded, alpha);
}

#[test]
fn test_empty_input() {
    assert_eq!(semaphore::encode("").unwrap(), "");
    assert_eq!(semaphore::decode("").unwrap(), "");
}

#[test]
fn test_ctf_flag() {
    let encoded = semaphore::encode("FLAG").unwrap();
    assert_eq!(encoded, "1-7 3-2 1-2 1-8");
    let decoded = semaphore::decode(&encoded).unwrap();
    assert_eq!(decoded, "FLAG");
}

#[test]
fn test_non_alpha_ignored() {
    assert_eq!(semaphore::encode("A1B").unwrap(), "1-2 1-3");
}

#[test]
fn test_decode_invalid() {
    assert!(semaphore::decode("9-9").is_err());
}

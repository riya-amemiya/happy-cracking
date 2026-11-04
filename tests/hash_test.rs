use happy_cracking::crypto::hash;

#[test]
fn test_md5() {
    assert_eq!(hash::md5_hash("hello"), "5d41402abc4b2a76b9719d911017c592");
}

#[test]
fn test_sha1() {
    assert_eq!(
        hash::sha1_hash("hello"),
        "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d"
    );
}

#[test]
fn test_sha256() {
    assert_eq!(
        hash::sha256_hash("hello"),
        "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
    );
}

#[test]
fn test_sha512() {
    assert_eq!(
        hash::sha512_hash("hello"),
        "9b71d224bd62f3785d96d46ad3ea3d73319bfbc2890caadae2dff72519673ca72323c3d99ba5c11d7c7acc6e14b8c5da0c4663475c2e5c3adef46f73bcdec043"
    );
}

#[test]
fn test_empty_string() {
    assert_eq!(hash::md5_hash(""), "d41d8cd98f00b204e9800998ecf8427e");
}

#[test]
fn check_hash_input_len_rejects_oversized() {
    let start = std::time::Instant::now();
    let err = hash::check_hash_input_len("abcd", 2).unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
    assert!(
        start.elapsed().as_millis() < 100,
        "oversized hash input must be rejected before digest computation"
    );
}

#[test]
fn check_hash_input_len_accepts_len_at_limit() {
    hash::check_hash_input_len("ab", 2).unwrap();
}

#[test]
fn check_hash_input_len_accepts_empty() {
    hash::check_hash_input_len("", 16).unwrap();
}

#[test]
fn run_rejects_oversized_hash_input() {
    let start = std::time::Instant::now();
    let err = hash::run(hash::HashAction::Sha256 {
        input: "a".repeat(hash::MAX_HASH_BYTES + 1),
    })
    .unwrap_err()
    .to_string();
    assert!(err.contains("Denial of Service"), "unexpected error: {err}");
    assert!(
        start.elapsed().as_millis() < 100,
        "oversized hash CLI input must be rejected before digest computation"
    );
}

#[test]
fn run_hashes_under_limit() {
    hash::run(hash::HashAction::Md5 {
        input: "hello".into(),
    })
    .unwrap();
}

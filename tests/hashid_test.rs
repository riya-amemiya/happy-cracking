use happy_cracking::crypto::hashid::{self, HashIdAction, MAX_HASHID_LEN};

#[test]
fn test_identify_md5() {
    let results = hashid::identify("5d41402abc4b2a76b9719d911017c592");
    assert!(results.contains(&"MD5"));
}

#[test]
fn test_identify_sha1() {
    let results = hashid::identify("aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d");
    assert!(results.contains(&"SHA-1"));
}

#[test]
fn test_identify_sha256() {
    let results =
        hashid::identify("2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
    assert!(results.contains(&"SHA-256"));
}

#[test]
fn test_identify_sha512() {
    let hash = "9b71d224bd62f3785d96d46ad3ea3d73319bfbc2890caadae2dff72519673ca72323c3d99ba5c11d7c7acc6e14b8c5da0c4663475c2e5c3adef46f73bcdec043";
    let results = hashid::identify(hash);
    assert!(results.contains(&"SHA-512"));
}

#[test]
fn test_identify_bcrypt() {
    let results = hashid::identify("$2a$10$N9qo8uLOickgx2ZMRZoMy.");
    assert!(results.contains(&"bcrypt"));
}

#[test]
fn test_identify_unknown() {
    let results = hashid::identify("xyz123");
    assert!(results.is_empty());
}

#[test]
fn identify_rejects_oversized_input() {
    let start = std::time::Instant::now();
    let too_long = "a".repeat(MAX_HASHID_LEN + 1);
    assert!(hashid::identify(&too_long).is_empty());
    assert!(
        start.elapsed().as_millis() < 100,
        "oversized hashid input must be rejected before classification"
    );
}

#[test]
fn identify_accepts_input_at_length_cap() {
    let input = "a".repeat(MAX_HASHID_LEN);
    assert!(hashid::identify(&input).is_empty());
}

#[test]
fn identify_trims_before_length_check() {
    let hash = "5d41402abc4b2a76b9719d911017c592";
    let padded = format!("{}{hash}{}", " ".repeat(MAX_HASHID_LEN), " ".repeat(8));
    assert!(hashid::identify(&padded).contains(&"MD5"));
}

#[test]
fn run_rejects_oversized_hashid_input() {
    let start = std::time::Instant::now();
    let err = hashid::run(HashIdAction::Identify {
        input: "a".repeat(MAX_HASHID_LEN + 1),
    })
    .unwrap_err()
    .to_string();
    assert!(
        err.contains(&MAX_HASHID_LEN.to_string()),
        "unexpected error: {err}"
    );
    assert!(
        start.elapsed().as_millis() < 100,
        "oversized hashid input must be rejected before classification"
    );
}

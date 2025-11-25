use happy_cracking::crypto::hashid;

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

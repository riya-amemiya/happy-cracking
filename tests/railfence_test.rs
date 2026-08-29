use happy_cracking::crypto::railfence;

#[test]
fn test_encrypt_3_rails() {
    let original = "WEAREDISCOVEREDFLEEATONCE";
    let encrypted = railfence::encrypt(original, 3).unwrap();
    let decrypted = railfence::decrypt(&encrypted, 3).unwrap();
    assert_eq!(decrypted, original);
}

#[test]
fn test_decrypt_3_rails() {
    let encrypted = railfence::encrypt("HELLOWORLD", 3).unwrap();
    let decrypted = railfence::decrypt(&encrypted, 3).unwrap();
    assert_eq!(decrypted, "HELLOWORLD");
}

#[test]
fn test_encrypt_2_rails() {
    assert_eq!(railfence::encrypt("HELLO", 2).unwrap(), "HLOEL");
}

#[test]
fn test_decrypt_2_rails() {
    assert_eq!(railfence::decrypt("HLOEL", 2).unwrap(), "HELLO");
}

#[test]
fn test_roundtrip() {
    let original = "ATTACKATDAWN";
    for rails in 2..=5 {
        let encrypted = railfence::encrypt(original, rails).unwrap();
        let decrypted = railfence::decrypt(&encrypted, rails).unwrap();
        assert_eq!(decrypted, original, "Failed for {} rails", rails);
    }
}

#[test]
fn test_invalid_rails() {
    assert!(railfence::encrypt("HELLO", 1).is_err());
}

#[test]
fn test_check_max_rails_rejects_excessive() {
    let err = railfence::check_max_rails(railfence::MAX_RAILS + 1).unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
}

#[test]
fn test_check_max_rails_accepts_limit() {
    railfence::check_max_rails(railfence::MAX_RAILS).unwrap();
}

#[test]
fn test_check_max_rails_accepts_default() {
    railfence::check_max_rails(10).unwrap();
}

#[test]
fn bruteforce_run_rejects_excessive_max_rails() {
    let err = railfence::run(railfence::RailFenceAction::Bruteforce {
        input: String::new(),
        max_rails: railfence::MAX_RAILS + 1,
    })
    .unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
}

#[test]
fn bruteforce_run_accepts_default_max_rails() {
    railfence::run(railfence::RailFenceAction::Bruteforce {
        input: "HOREL OLLWD".to_string(),
        max_rails: 10,
    })
    .unwrap();
}

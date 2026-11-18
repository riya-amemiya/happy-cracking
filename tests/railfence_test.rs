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
fn test_encrypt_hello_world_3_rails() {
    assert_eq!(railfence::encrypt("HELLO WORLD", 3).unwrap(), "HOREL OLLWD");
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
fn test_roundtrip_unicode() {
    let original = "日本語🚀flag{rail}";
    for rails in 2..=8 {
        let encrypted = railfence::encrypt(original, rails).unwrap();
        let decrypted = railfence::decrypt(&encrypted, rails).unwrap();
        assert_eq!(decrypted, original, "Failed for {rails} rails");
    }
}

#[test]
fn test_empty_and_identity() {
    assert_eq!(railfence::encrypt("", 3).unwrap(), "");
    assert_eq!(railfence::decrypt("", 3).unwrap(), "");
    assert_eq!(railfence::encrypt("HI", 10).unwrap(), "HI");
    assert_eq!(railfence::decrypt("HI", 10).unwrap(), "HI");
    let original = "flag{ascii_bytes}";
    let encrypted = railfence::encrypt(original, 4).unwrap();
    assert_eq!(railfence::decrypt(&encrypted, 4).unwrap(), original);
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

#[test]
fn test_check_bruteforce_input_len_rejects_excessive() {
    let input = "A".repeat(railfence::MAX_BRUTEFORCE_BYTES + 1);
    let err =
        railfence::check_bruteforce_input_len(&input, railfence::MAX_BRUTEFORCE_BYTES).unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
}

#[test]
fn test_check_bruteforce_input_len_accepts_limit() {
    let input = "A".repeat(railfence::MAX_BRUTEFORCE_BYTES);
    railfence::check_bruteforce_input_len(&input, railfence::MAX_BRUTEFORCE_BYTES).unwrap();
}

#[test]
fn bruteforce_run_rejects_excessive_input() {
    let err = railfence::run(railfence::RailFenceAction::Bruteforce {
        input: "A".repeat(railfence::MAX_BRUTEFORCE_BYTES + 1),
        max_rails: 10,
    })
    .unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
}

#[test]
fn encrypt_decrypt_accept_input_beyond_bruteforce_cap() {
    let original = "A".repeat(railfence::MAX_BRUTEFORCE_BYTES + 8);
    let encrypted = railfence::encrypt(&original, 3).unwrap();
    assert_eq!(railfence::decrypt(&encrypted, 3).unwrap(), original);
}

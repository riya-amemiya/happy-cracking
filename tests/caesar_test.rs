use happy_cracking::crypto::caesar;
use happy_cracking::crypto::rot;

#[test]
fn encrypt_decrypt_roundtrip() {
    let plaintext = "The quick brown fox";
    let encrypted = rot::rotate(plaintext, 7);
    let decrypted = rot::rotate(&encrypted, 26 - 7);
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_check_bruteforce_input_len_rejects_excessive() {
    let input = "A".repeat(caesar::MAX_BRUTEFORCE_BYTES + 1);
    let err = caesar::check_bruteforce_input_len(&input, caesar::MAX_BRUTEFORCE_BYTES).unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
}

#[test]
fn test_check_bruteforce_input_len_accepts_limit() {
    let input = "A".repeat(caesar::MAX_BRUTEFORCE_BYTES);
    caesar::check_bruteforce_input_len(&input, caesar::MAX_BRUTEFORCE_BYTES).unwrap();
}

#[test]
fn bruteforce_run_rejects_excessive_input() {
    let err = caesar::run(caesar::CaesarAction::Bruteforce {
        input: "A".repeat(caesar::MAX_BRUTEFORCE_BYTES + 1),
    })
    .unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
}

#[test]
fn bruteforce_run_accepts_default_ciphertext() {
    caesar::run(caesar::CaesarAction::Bruteforce {
        input: "Khoor".to_string(),
    })
    .unwrap();
}

#[test]
fn encrypt_decrypt_accept_input_beyond_bruteforce_cap() {
    let original = "A".repeat(caesar::MAX_BRUTEFORCE_BYTES + 8);
    let encrypted = rot::rotate(&original, 3);
    assert_eq!(rot::rotate(&encrypted, 23), original);
}

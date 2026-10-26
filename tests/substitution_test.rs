use happy_cracking::crypto::substitution;

#[test]
fn test_encode_atbash_alphabet() {
    let alphabet = "ZYXWVUTSRQPONMLKJIHGFEDCBA";
    assert_eq!(substitution::encode("HELLO", alphabet).unwrap(), "SVOOL");
}

#[test]
fn test_decode_atbash_alphabet() {
    let alphabet = "ZYXWVUTSRQPONMLKJIHGFEDCBA";
    assert_eq!(substitution::decode("SVOOL", alphabet).unwrap(), "HELLO");
}

#[test]
fn test_roundtrip() {
    let alphabet = "QWERTYUIOPASDFGHJKLZXCVBNM";
    let original = "The quick brown fox";
    let encoded = substitution::encode(original, alphabet).unwrap();
    let decoded = substitution::decode(&encoded, alphabet).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn test_preserve_non_alpha() {
    let alphabet = "ZYXWVUTSRQPONMLKJIHGFEDCBA";
    assert_eq!(
        substitution::encode("Hello, World!", alphabet).unwrap(),
        "Svool, Dliow!"
    );
}

#[test]
fn test_invalid_alphabet_too_short() {
    assert!(substitution::encode("HELLO", "ABC").is_err());
}

#[test]
fn test_invalid_alphabet_duplicate() {
    assert!(substitution::encode("HELLO", "AAXWVUTSRQPONMLKJIHGFEDCBA").is_err());
}

#[test]
fn test_empty_input() {
    let alphabet = "ZYXWVUTSRQPONMLKJIHGFEDCBA";
    assert_eq!(substitution::encode("", alphabet).unwrap(), "");
}

#[test]
fn test_lowercase_preserved() {
    let alphabet = "ZYXWVUTSRQPONMLKJIHGFEDCBA";
    assert_eq!(substitution::encode("hello", alphabet).unwrap(), "svool");
}

#[test]
fn test_solve_returns_result() {
    // Use enough English text so frequency analysis has data to work with
    let alphabet = "ZYXWVUTSRQPONMLKJIHGFEDCBA";
    let plaintext = "THE QUICK BROWN FOX JUMPS OVER THE LAZY DOG THIS IS A LONGER TEXT TO HELP WITH FREQUENCY ANALYSIS";
    let ciphertext = substitution::encode(plaintext, alphabet).unwrap();
    let (key, result, score) = substitution::solve(&ciphertext, 10000).unwrap();
    assert_eq!(key, "UCYLVMTNSDAGFKRJXIHPWEBQZO");
    assert!(!result.is_empty());
    assert!(score > 0.0);
    assert_eq!(key.len(), 26);
}

#[test]
fn test_solve_empty_and_single_letter() {
    let (key, text, score) = substitution::solve("", 100).unwrap();
    assert_eq!(key.len(), 26);
    assert!(text.is_empty());
    assert_eq!(score, 0.0);

    let (key, text, score) = substitution::solve("A", 100).unwrap();
    assert_eq!(key.len(), 26);
    assert_eq!(text, "E");
    assert_eq!(score, 0.0);
}

#[test]
fn test_solve_rejects_excessive_iterations() {
    use std::time::Instant;

    let start = Instant::now();
    let res = substitution::solve("HELLOWORLD", substitution::MAX_SOLVE_ITERATIONS + 1);
    assert!(res.is_err(), "Expected error for excessive iterations");
    let err = res.unwrap_err().to_string();
    assert!(
        err.contains("exceeds the maximum allowed limit"),
        "unexpected error: {err}"
    );
    assert!(
        start.elapsed().as_millis() < 100,
        "Rejection took too long: {:?}",
        start.elapsed()
    );
}

#[test]
fn test_flag_format() {
    let alphabet = "QWERTYUIOPASDFGHJKLZXCVBNM";
    let encoded = substitution::encode("flag{substitution_cipher}", alphabet).unwrap();
    let decoded = substitution::decode(&encoded, alphabet).unwrap();
    assert_eq!(decoded, "flag{substitution_cipher}");
}

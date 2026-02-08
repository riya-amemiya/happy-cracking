use happy_cracking::crypto::entropy;

#[test]
fn test_empty_string() {
    assert!((entropy::calculate("")).abs() < 0.001);
}

#[test]
fn test_single_char_repeated() {
    // All same bytes => entropy is 0
    assert!((entropy::calculate("aaaaaaa")).abs() < 0.001);
}

#[test]
fn test_two_equal_chars() {
    // "ab" has 2 symbols each with p=0.5 => H = 1.0
    assert!((entropy::calculate("ab") - 1.0).abs() < 0.01);
}

#[test]
fn test_four_equal_chars() {
    // "abcd" has 4 symbols each with p=0.25 => H = 2.0
    assert!((entropy::calculate("abcd") - 2.0).abs() < 0.01);
}

#[test]
fn test_natural_language() {
    let text = "The quick brown fox jumps over the lazy dog";
    let e = entropy::calculate(text);
    assert!(e > 3.0 && e < 5.0);
}

#[test]
fn test_classify_high() {
    assert_eq!(entropy::classify(7.5), "High (likely encrypted/compressed)");
    assert_eq!(entropy::classify(8.0), "High (likely encrypted/compressed)");
}

#[test]
fn test_classify_medium() {
    assert_eq!(entropy::classify(4.0), "Medium (natural language/code)");
    assert_eq!(entropy::classify(6.0), "Medium (natural language/code)");
}

#[test]
fn test_classify_low() {
    assert_eq!(entropy::classify(0.0), "Low (low entropy data)");
    assert_eq!(entropy::classify(3.99), "Low (low entropy data)");
}

#[test]
fn test_high_entropy() {
    // All 95 printable ASCII characters (0x20-0x7E) each once => H = log2(95) ≈ 6.57
    let input: String = (0x20u8..=0x7Eu8).map(|b| b as char).collect();
    let e = entropy::calculate(&input);
    let expected = (95.0_f64).log2();
    assert!((e - expected).abs() < 0.01);
}

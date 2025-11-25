use happy_cracking::crypto::frequency;

#[test]
fn test_analyze_basic() {
    let result = frequency::analyze("aab", false);
    assert_eq!(result.total_chars, 3);
    assert_eq!(result.frequencies[0].0, 'a');
    assert_eq!(result.frequencies[0].1, 2);
    assert!((result.frequencies[0].2 - 66.666).abs() < 0.01);
    assert_eq!(result.frequencies[1].0, 'b');
    assert_eq!(result.frequencies[1].1, 1);
}

#[test]
fn test_analyze_alpha_only() {
    let result = frequency::analyze("a1b2c3", true);
    assert_eq!(result.total_chars, 3);
}

#[test]
fn test_analyze_case_insensitive_alpha() {
    let result = frequency::analyze("AaA", true);
    assert_eq!(result.total_chars, 3);
    assert_eq!(result.frequencies[0].0, 'A');
    assert_eq!(result.frequencies[0].1, 3);
}

#[test]
fn test_analyze_empty() {
    let result = frequency::analyze("", false);
    assert_eq!(result.total_chars, 0);
    assert!(result.frequencies.is_empty());
}

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

#[test]
fn test_analyze_sort_order() {
    // A: 3, B: 2, C: 2, D: 1
    // Expected order: A (3), B (2), C (2), D (1)
    // Since we iterate A..Z in implementation, B comes before C in the list.
    // Stable sort by count descending (3, 2, 2, 1) should keep B before C.
    let input = "AAABBCCD";
    let result = frequency::analyze(input, true);

    assert_eq!(result.total_chars, 8);
    assert_eq!(result.frequencies.len(), 4);

    assert_eq!(result.frequencies[0].0, 'A');
    assert_eq!(result.frequencies[0].1, 3);

    // Check B and C have count 2
    assert_eq!(result.frequencies[1].1, 2);
    assert_eq!(result.frequencies[2].1, 2);

    // Check stability (alphabetical for ties)
    assert_eq!(result.frequencies[1].0, 'B');
    assert_eq!(result.frequencies[2].0, 'C');

    assert_eq!(result.frequencies[3].0, 'D');
    assert_eq!(result.frequencies[3].1, 1);
}

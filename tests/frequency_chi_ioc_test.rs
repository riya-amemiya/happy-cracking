use happy_cracking::crypto::frequency;

#[test]
fn chi_squared_english_text() {
    let english =
        "The quick brown fox jumps over the lazy dog and then some more text to make this longer";
    let score = frequency::chi_squared(english);
    // English text should have a relatively low chi-squared score
    assert!(
        score < 100.0,
        "Expected low chi-squared for English text, got {}",
        score
    );
}

#[test]
fn chi_squared_random_text() {
    let random = "ZZZZQQQQXXXX";
    let score = frequency::chi_squared(random);
    // Random/non-English distribution should have high chi-squared
    assert!(
        score > 50.0,
        "Expected high chi-squared for non-English text, got {}",
        score
    );
}

#[test]
fn chi_squared_empty() {
    let score = frequency::chi_squared("");
    assert!(score.is_infinite());
}

#[test]
fn ioc_english_text() {
    let english =
        "The quick brown fox jumps over the lazy dog and then some more text to make this longer";
    let ioc = frequency::index_of_coincidence(english);
    // English text IoC should be around 0.0667
    assert!(
        ioc > 0.05 && ioc < 0.08,
        "Expected IoC near 0.0667 for English, got {}",
        ioc
    );
}

#[test]
fn ioc_uniform_distribution() {
    // All 26 letters equally represented
    let uniform: String = (b'A'..=b'Z').map(|b| b as char).collect();
    let ioc = frequency::index_of_coincidence(&uniform);
    // With exactly 1 of each letter, IoC = 0 (each n*(n-1) = 0)
    assert!(
        (ioc - 0.0).abs() < 0.001,
        "Expected IoC near 0 for uniform, got {}",
        ioc
    );
}

#[test]
fn ioc_single_char() {
    let ioc = frequency::index_of_coincidence("A");
    assert_eq!(ioc, 0.0);
}

#[test]
fn ioc_empty() {
    let ioc = frequency::index_of_coincidence("");
    assert_eq!(ioc, 0.0);
}

#[test]
fn ioc_repeated_char() {
    let repeated = "AAAAAAAAAA";
    let ioc = frequency::index_of_coincidence(repeated);
    // All same letter: IoC = 1.0
    assert!(
        (ioc - 1.0).abs() < 0.001,
        "Expected IoC = 1.0 for repeated char, got {}",
        ioc
    );
}

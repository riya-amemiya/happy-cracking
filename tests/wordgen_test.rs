use happy_cracking::crypto::wordgen::{write_enumerated, write_masked};

#[test]
fn enumerate_streams_shorter_lengths_first() {
    let mut output = Vec::new();
    write_enumerated(&mut output, "ab", 1, 2, false).unwrap();
    assert_eq!(
        String::from_utf8(output).unwrap(),
        "a\nb\naa\nab\nba\nbb\n"
    );
}

#[test]
fn enumerate_deduplicates_unicode_charset_in_first_seen_order() {
    let mut output = Vec::new();
    write_enumerated(&mut output, "猫犬猫", 1, 1, false).unwrap();
    assert_eq!(String::from_utf8(output).unwrap(), "猫\n犬\n");
}

#[test]
fn enumerate_rejects_spaces_over_the_default_limit() {
    let error =
        write_enumerated(&mut std::io::sink(), "0123456789", 10, 10, false).unwrap_err();
    assert!(error.to_string().contains("1,000,000,000"));
}

#[test]
fn mask_expands_only_custom_positions() {
    let mut output = Vec::new();
    write_masked(&mut output, "AB?c?cZ", "01", false).unwrap();
    assert_eq!(
        String::from_utf8(output).unwrap(),
        "AB00Z\nAB01Z\nAB10Z\nAB11Z\n"
    );
}

#[test]
fn mask_supports_literal_question_mark() {
    let mut output = Vec::new();
    write_masked(&mut output, "A??B?c", "x", false).unwrap();
    assert_eq!(String::from_utf8(output).unwrap(), "A?Bx\n");
}

#[test]
fn mask_rejects_unknown_tokens() {
    let error = write_masked(&mut std::io::sink(), "?x", "ab", false).unwrap_err();
    assert!(error.to_string().contains("Unknown mask token"));
}

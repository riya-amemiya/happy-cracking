use happy_cracking::crypto::wordgen::write_enumerated;

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

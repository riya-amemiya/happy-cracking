use happy_cracking::crypto::wordgen::{write_enumerated, write_masked, write_random};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

#[test]
fn enumerate_streams_shorter_lengths_first() {
    let mut output = Vec::new();
    write_enumerated(&mut output, "ab", 1, 2, false).unwrap();
    assert_eq!(String::from_utf8(output).unwrap(), "a\nb\naa\nab\nba\nbb\n");
}

#[test]
fn enumerate_deduplicates_unicode_charset_in_first_seen_order() {
    let mut output = Vec::new();
    write_enumerated(&mut output, "猫犬猫", 1, 1, false).unwrap();
    assert_eq!(String::from_utf8(output).unwrap(), "猫\n犬\n");
}

#[test]
fn enumerate_rejects_spaces_over_the_default_limit() {
    let error = write_enumerated(&mut std::io::sink(), "0123456789", 10, 10, false).unwrap_err();
    assert!(error.to_string().contains("1,000,000,000"));
}

#[test]
fn enumerate_rejects_empty_charset_and_invalid_range() {
    let empty = write_enumerated(&mut std::io::sink(), "", 1, 1, false).unwrap_err();
    assert!(empty.to_string().contains("--charset"));

    let range = write_enumerated(&mut std::io::sink(), "ab", 3, 2, false).unwrap_err();
    assert!(range.to_string().contains("--max-len"));
}

#[test]
fn generators_reject_line_breaks() {
    let newline = write_enumerated(&mut std::io::sink(), "a\n", 1, 1, false).unwrap_err();
    assert!(newline.to_string().contains("line break"));

    let carriage_return = write_random(&mut std::io::sink(), "a\r", 1, 1, false).unwrap_err();
    assert!(carriage_return.to_string().contains("line break"));

    let literal = write_masked(&mut std::io::sink(), "fixed\n?c", "ab", false).unwrap_err();
    assert!(literal.to_string().contains("line break"));
}

#[test]
fn generators_enforce_charset_and_output_length_boundaries() {
    let charset_256: String = (0..256)
        .map(|offset| char::from_u32(0x100 + offset).unwrap())
        .collect();
    let charset_257 = format!("{charset_256}\u{200}");

    let mut charset_output = Vec::new();
    write_enumerated(&mut charset_output, &charset_256, 1, 1, false).unwrap();
    assert_eq!(
        charset_output
            .iter()
            .filter(|&&value| value == b'\n')
            .count(),
        256
    );
    assert!(write_enumerated(&mut std::io::sink(), &charset_257, 1, 1, false).is_err());

    let mask_4096 = "x".repeat(4_096);
    let mut mask_output = Vec::new();
    write_masked(&mut mask_output, &mask_4096, "a", false).unwrap();
    assert_eq!(mask_output.len(), 4_097);
    assert!(write_masked(&mut std::io::sink(), &format!("{mask_4096}x"), "a", false).is_err());
}

#[test]
fn length_errors_name_the_offending_option() {
    let min = write_enumerated(&mut std::io::sink(), "ab", 0, 1, false).unwrap_err();
    assert!(min.to_string().contains("--min-len"));

    let max = write_enumerated(&mut std::io::sink(), "ab", 1, 4_097, false).unwrap_err();
    assert!(max.to_string().contains("--max-len"));

    let random = write_random(&mut std::io::sink(), "ab", 0, 1, false).unwrap_err();
    assert!(random.to_string().contains("--length"));
}

#[test]
fn forced_generation_still_rejects_candidate_count_overflow() {
    let charset_256: String = (0..256)
        .map(|offset| char::from_u32(0x100 + offset).unwrap())
        .collect();
    let error = write_enumerated(&mut std::io::sink(), &charset_256, 16, 16, true).unwrap_err();
    assert!(error.to_string().contains("overflow"));
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

#[test]
fn mask_rejects_dangling_token_and_oversized_space() {
    let dangling = write_masked(&mut std::io::sink(), "fixed?", "ab", false).unwrap_err();
    assert!(dangling.to_string().contains("dangling"));

    let mask = "?c".repeat(10);
    let oversized = write_masked(&mut std::io::sink(), &mask, "0123456789", false).unwrap_err();
    assert!(oversized.to_string().contains("1,000,000,000"));
}

#[test]
fn random_writes_requested_shape_and_count() {
    let mut output = Vec::new();
    write_random(&mut output, "x", 4, 3, false).unwrap();
    assert_eq!(String::from_utf8(output).unwrap(), "xxxx\nxxxx\nxxxx\n");
}

#[test]
fn random_rejects_zero_count() {
    let error = write_random(&mut std::io::sink(), "ab", 4, 0, false).unwrap_err();
    assert!(error.to_string().contains("--count"));
}

#[test]
fn random_rejects_oversized_count_without_force() {
    let error = write_random(&mut std::io::sink(), "ab", 4, 1_000_000_001, false).unwrap_err();
    assert!(error.to_string().contains("--force"));
}

#[test]
fn wordgen_enumerate_cli_streams_lines() {
    let output = Command::new(env!("CARGO_BIN_EXE_happy-cracking"))
        .args([
            "wordgen",
            "enumerate",
            "--charset",
            "ab",
            "--min-len",
            "1",
            "--max-len",
            "1",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output.stderr);
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "a\nb\n");
}

#[test]
fn wordgen_mask_cli_streams_fixed_pattern() {
    let output = Command::new(env!("CARGO_BIN_EXE_happy-cracking"))
        .args(["wordgen", "mask", "A?c", "--charset", "01"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output.stderr);
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "A0\nA1\n");
}

#[test]
fn wordgen_random_cli_streams_requested_count() {
    let output = Command::new(env!("CARGO_BIN_EXE_happy-cracking"))
        .args([
            "wordgen",
            "random",
            "--length",
            "4",
            "--count",
            "3",
            "--charset",
            "x",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output.stderr);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "xxxx\nxxxx\nxxxx\n"
    );
}

#[test]
fn wordgen_exits_successfully_when_downstream_pipe_closes() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_happy-cracking"))
        .args([
            "wordgen",
            "enumerate",
            "--charset",
            "ab",
            "--min-len",
            "1",
            "--max-len",
            "25",
            "--force",
        ])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    let mut first_line = String::new();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    output.read_line(&mut first_line).unwrap();
    assert_eq!(first_line, "a\n");
    drop(output);

    assert!(child.wait().unwrap().success());
}

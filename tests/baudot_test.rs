use happy_cracking::crypto::baudot;

#[test]
fn test_encode_basic() {
    assert_eq!(baudot::encode("A").unwrap(), "00011");
}

#[test]
fn test_encode_lowercase() {
    assert_eq!(
        baudot::encode("hello").unwrap(),
        baudot::encode("HELLO").unwrap()
    );
}

#[test]
fn test_encode_skips_unknown_chars() {
    assert_eq!(
        baudot::encode("HE@LLO").unwrap(),
        baudot::encode("HELLO").unwrap()
    );
}

#[test]
fn test_encode_figures_in_letter_word() {
    // '!' shares F's ITA2 slot and needs a FIGS shift (and LTRS to resume).
    assert_eq!(
        baudot::encode("HE!LLO").unwrap(),
        "10100 00001 11011 01101 11111 10010 10010 11000"
    );
}

#[test]
fn test_encode_with_space() {
    assert_eq!(
        baudot::encode("HI THERE").unwrap(),
        "10100 00110 00100 10000 10100 00001 01010 00001"
    );
}

#[test]
fn test_decode_basic() {
    assert_eq!(baudot::decode("00011").unwrap(), "A");
}

#[test]
fn test_decode_hello() {
    assert_eq!(
        baudot::decode("10100 00001 10010 10010 11000").unwrap(),
        "HELLO"
    );
}

#[test]
fn test_roundtrip() {
    let original = "HELLO WORLD";
    let encoded = baudot::encode(original).unwrap();
    let decoded = baudot::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn test_empty_input() {
    assert_eq!(baudot::encode("").unwrap(), "");
    assert_eq!(baudot::decode("").unwrap(), "");
}

#[test]
fn test_ctf_flag() {
    let encoded = baudot::encode("FLAG").unwrap();
    let decoded = baudot::decode(&encoded).unwrap();
    assert_eq!(decoded, "FLAG");
}

#[test]
fn test_figures_shift() {
    let encoded = baudot::encode("A3").unwrap();
    // A=00011, then FIGS=11011, 3(=E position)=00001
    assert_eq!(encoded, "00011 11011 00001");
}

#[test]
fn test_decode_figures() {
    // FIGS shift, then code for '3' (same position as E = 00001)
    let decoded = baudot::decode("11011 00001").unwrap();
    assert_eq!(decoded, "3");
}

#[test]
fn test_decode_invalid_code() {
    assert!(baudot::decode("111").is_err());
}

#[test]
fn test_letters_after_figures() {
    // A, then FIGS, 3, then LTRS, B
    let decoded = baudot::decode("00011 11011 00001 11111 11001").unwrap();
    assert_eq!(decoded, "A3B");
}

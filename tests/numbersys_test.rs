use happy_cracking::crypto::numbersys;

#[test]
fn decimal_to_hex() {
    assert_eq!(numbersys::convert("255", 10, 16).unwrap(), "ff");
}

#[test]
fn hex_to_decimal() {
    assert_eq!(numbersys::convert("ff", 16, 10).unwrap(), "255");
}

#[test]
fn binary_to_decimal() {
    assert_eq!(numbersys::convert("11111111", 2, 10).unwrap(), "255");
}

#[test]
fn decimal_to_binary() {
    assert_eq!(numbersys::convert("42", 10, 2).unwrap(), "101010");
}

#[test]
fn octal_to_hex() {
    assert_eq!(numbersys::convert("777", 8, 16).unwrap(), "1ff");
}

#[test]
fn zero_conversion() {
    assert_eq!(numbersys::convert("0", 10, 16).unwrap(), "0");
}

#[test]
fn base36_conversion() {
    assert_eq!(numbersys::convert("zz", 36, 10).unwrap(), "1295");
}

#[test]
fn roundtrip_decimal_hex() {
    let original = "12345";
    let hex = numbersys::convert(original, 10, 16).unwrap();
    let back = numbersys::convert(&hex, 16, 10).unwrap();
    assert_eq!(back, original);
}

#[test]
fn invalid_base_too_low() {
    assert!(numbersys::convert("10", 1, 10).is_err());
}

#[test]
fn invalid_base_too_high() {
    assert!(numbersys::convert("10", 10, 37).is_err());
}

#[test]
fn invalid_digit_for_base() {
    assert!(numbersys::convert("2", 2, 10).is_err());
}

#[test]
fn empty_input() {
    assert!(numbersys::convert("", 10, 16).is_err());
}

#[test]
fn large_number() {
    let result = numbersys::convert("1000000", 10, 16).unwrap();
    assert_eq!(result, "f4240");
}

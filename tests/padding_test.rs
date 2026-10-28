use happy_cracking::crypto::padding;

#[test]
fn test_pkcs7_pad_basic() {
    let data = vec![0x01, 0x02, 0x03];
    let padded = padding::pkcs7_pad(&data, 4).unwrap();
    assert_eq!(padded, vec![0x01, 0x02, 0x03, 0x01]);
}

#[test]
fn test_pkcs7_pad_full_block() {
    // When data is already aligned, a full block of padding is added
    let data = vec![0x01, 0x02, 0x03, 0x04];
    let padded = padding::pkcs7_pad(&data, 4).unwrap();
    assert_eq!(padded, vec![0x01, 0x02, 0x03, 0x04, 0x04, 0x04, 0x04, 0x04]);
}

#[test]
fn test_pkcs7_pad_16() {
    let data = b"Hello".to_vec();
    let padded = padding::pkcs7_pad(&data, 16).unwrap();
    assert_eq!(padded.len(), 16);
    assert!(padded[5..].iter().all(|&b| b == 11));
}

#[test]
fn test_pkcs7_roundtrip() {
    let data = vec![0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE];
    let padded = padding::pkcs7_pad(&data, 8).unwrap();
    let unpadded = padding::pkcs7_unpad(&padded, 8).unwrap();
    assert_eq!(unpadded, data);
}

#[test]
fn test_pkcs7_unpad_invalid_padding_value() {
    // Last byte is 0, which is invalid
    let data = vec![0x01, 0x02, 0x03, 0x00];
    assert!(padding::pkcs7_unpad(&data, 4).is_err());
}

#[test]
fn test_pkcs7_unpad_inconsistent_padding() {
    // Last byte says 3 bytes of padding, but they don't all match
    let data = vec![0x01, 0x02, 0x03, 0x03];
    assert!(padding::pkcs7_unpad(&data, 4).is_err());
}

#[test]
fn test_pkcs7_unpad_empty() {
    assert!(padding::pkcs7_unpad(&[], 16).is_err());
}

#[test]
fn test_pkcs7_invalid_block_size() {
    assert!(padding::pkcs7_pad(&[0x01], 0).is_err());
    assert!(padding::pkcs7_pad(&[0x01], 256).is_err());
}

#[test]
fn test_pkcs7_pad_empty_input() {
    let padded = padding::pkcs7_pad(&[], 4).unwrap();
    assert_eq!(padded, vec![0x04, 0x04, 0x04, 0x04]);
}

#[test]
fn test_zero_pad_basic() {
    let data = vec![0x01, 0x02, 0x03];
    let padded = padding::zero_pad(&data, 4).unwrap();
    assert_eq!(padded, vec![0x01, 0x02, 0x03, 0x00]);
}

#[test]
fn test_zero_pad_aligned() {
    let data = vec![0x01, 0x02, 0x03, 0x04];
    let padded = padding::zero_pad(&data, 4).unwrap();
    assert_eq!(padded, data);
}

#[test]
fn test_zero_pad_16() {
    let data = b"Hello".to_vec();
    let padded = padding::zero_pad(&data, 16).unwrap();
    assert_eq!(padded.len(), 16);
    assert!(padded[5..].iter().all(|&b| b == 0));
}

#[test]
fn test_zero_unpad() {
    let data = vec![0x01, 0x02, 0x03, 0x00, 0x00];
    let unpadded = padding::zero_unpad(&data);
    assert_eq!(unpadded, vec![0x01, 0x02, 0x03]);
}

#[test]
fn test_zero_unpad_no_padding() {
    let data = vec![0x01, 0x02, 0x03];
    let unpadded = padding::zero_unpad(&data);
    assert_eq!(unpadded, data);
}

#[test]
fn test_zero_unpad_all_zeros() {
    let data = vec![0x00, 0x00, 0x00];
    let unpadded = padding::zero_unpad(&data);
    assert!(unpadded.is_empty());
}

#[test]
fn test_zero_roundtrip() {
    let data = vec![0xCA, 0xFE, 0xBA, 0xBE, 0x01];
    let padded = padding::zero_pad(&data, 8).unwrap();
    let unpadded = padding::zero_unpad(&padded);
    assert_eq!(unpadded, data);
}

#[test]
fn test_zero_pad_empty() {
    let padded = padding::zero_pad(&[], 16).unwrap();
    assert!(padded.is_empty());
}

#[test]
fn test_zero_pad_excessive_block_size_rejected() {
    // Security: block_size that would allocate excessive memory must be rejected
    let data = vec![0x01];
    assert!(padding::zero_pad(&data, 256 * 1024 * 1024).is_err());
}

#[test]
fn test_pkcs7_unpad_not_multiple_of_block_size() {
    let data = vec![0x01, 0x02, 0x03, 0x04, 0x05];
    assert!(padding::pkcs7_unpad(&data, 4).is_err());
}

#[test]
fn decode_padding_hex_with_limit_rejects_oversized_dump() {
    let err = padding::decode_padding_hex_with_limit("41424344", 2).unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
}

#[test]
fn decode_padding_hex_with_limit_accepts_dump_at_limit() {
    let data = padding::decode_padding_hex_with_limit("4142", 2).unwrap();
    assert_eq!(data, b"AB");
}

#[test]
fn decode_padding_hex_with_limit_accepts_empty() {
    let data = padding::decode_padding_hex_with_limit("", 16).unwrap();
    assert!(data.is_empty());
}

#[test]
fn decode_padding_hex_with_limit_rejects_invalid_hex() {
    let err = padding::decode_padding_hex_with_limit("zz", 16).unwrap_err();
    assert!(!err.to_string().contains("Denial of Service"));
}

fn to_hex(data: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(data.len() * 2);
    for &b in data {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

fn hex_cli_pkcs7_pad(input: &str, block_size: usize) -> String {
    let data = padding::decode_padding_hex_with_limit(input, padding::MAX_PADDING_BYTES).unwrap();
    to_hex(&padding::pkcs7_pad(&data, block_size).unwrap())
}

fn hex_cli_pkcs7_unpad(input: &str, block_size: usize) -> String {
    let data = padding::decode_padding_hex_with_limit(input, padding::MAX_PADDING_BYTES).unwrap();
    to_hex(&padding::pkcs7_unpad(&data, block_size).unwrap())
}

fn hex_cli_zero_pad(input: &str, block_size: usize) -> String {
    let data = padding::decode_padding_hex_with_limit(input, padding::MAX_PADDING_BYTES).unwrap();
    to_hex(&padding::zero_pad(&data, block_size).unwrap())
}

fn hex_cli_zero_unpad(input: &str) -> String {
    let data = padding::decode_padding_hex_with_limit(input, padding::MAX_PADDING_BYTES).unwrap();
    to_hex(&padding::zero_unpad(&data))
}

#[test]
fn pkcs7_pad_hex_matches_decode_encode_cli() {
    assert_eq!(
        padding::pkcs7_pad_hex("deadbeef", 8).unwrap(),
        hex_cli_pkcs7_pad("deadbeef", 8)
    );
    assert_eq!(
        padding::pkcs7_pad_hex("DEADBEEF", 8).unwrap(),
        hex_cli_pkcs7_pad("DEADBEEF", 8)
    );
    assert_eq!(
        padding::pkcs7_pad_hex("01020304", 4).unwrap(),
        hex_cli_pkcs7_pad("01020304", 4)
    );
    assert_eq!(
        padding::pkcs7_pad_hex("", 4).unwrap(),
        hex_cli_pkcs7_pad("", 4)
    );
}

#[test]
fn pkcs7_unpad_hex_matches_decode_encode_cli() {
    let padded = padding::pkcs7_pad_hex("deadbeef", 8).unwrap();
    assert_eq!(
        padding::pkcs7_unpad_hex(&padded, 8).unwrap(),
        hex_cli_pkcs7_unpad(&padded, 8)
    );
    let upper = padded.to_ascii_uppercase();
    assert_eq!(
        padding::pkcs7_unpad_hex(&upper, 8).unwrap(),
        hex_cli_pkcs7_unpad(&upper, 8)
    );
}

#[test]
fn zero_pad_hex_matches_decode_encode_cli() {
    assert_eq!(
        padding::zero_pad_hex("deadbeef", 8).unwrap(),
        hex_cli_zero_pad("deadbeef", 8)
    );
    assert_eq!(
        padding::zero_pad_hex("DEADBEEF", 8).unwrap(),
        "deadbeef00000000"
    );
    assert_eq!(
        padding::zero_pad_hex("01020304", 4).unwrap(),
        hex_cli_zero_pad("01020304", 4)
    );
    assert_eq!(padding::zero_pad_hex("", 16).unwrap(), "");
}

#[test]
fn zero_unpad_hex_matches_decode_encode_cli() {
    assert_eq!(
        padding::zero_unpad_hex("cafe0000").unwrap(),
        hex_cli_zero_unpad("cafe0000")
    );
    assert_eq!(padding::zero_unpad_hex("CAFE0000").unwrap(), "cafe");
    assert_eq!(padding::zero_unpad_hex("000000").unwrap(), "");
}

#[test]
fn pkcs7_pad_hex_rejects_invalid_hex_before_block_size() {
    let err = padding::pkcs7_pad_hex("zz", 16).unwrap_err();
    assert!(err.to_string().contains("Failed to decode input hex"));
    let err = padding::pkcs7_pad_hex("abc", 16).unwrap_err();
    assert!(err.to_string().contains("Failed to decode input hex"));
}

#[test]
fn pkcs7_unpad_hex_empty_is_empty_data() {
    let err = padding::pkcs7_unpad_hex("", 16).unwrap_err();
    assert!(err.to_string().contains("Input data is empty"));
}

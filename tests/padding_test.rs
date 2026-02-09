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
    let padded = padding::zero_pad(&data, 4);
    assert_eq!(padded, vec![0x01, 0x02, 0x03, 0x00]);
}

#[test]
fn test_zero_pad_aligned() {
    let data = vec![0x01, 0x02, 0x03, 0x04];
    let padded = padding::zero_pad(&data, 4);
    assert_eq!(padded, data);
}

#[test]
fn test_zero_pad_16() {
    let data = b"Hello".to_vec();
    let padded = padding::zero_pad(&data, 16);
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
    let padded = padding::zero_pad(&data, 8);
    let unpadded = padding::zero_unpad(&padded);
    assert_eq!(unpadded, data);
}

#[test]
fn test_zero_pad_empty() {
    let padded = padding::zero_pad(&[], 16);
    assert!(padded.is_empty());
}

#[test]
fn test_pkcs7_unpad_not_multiple_of_block_size() {
    let data = vec![0x01, 0x02, 0x03, 0x04, 0x05];
    assert!(padding::pkcs7_unpad(&data, 4).is_err());
}

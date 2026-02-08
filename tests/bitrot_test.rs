use happy_cracking::crypto::bitrot;

#[test]
fn test_rotate_left_8_by_1() {
    // 0x80 (10000000) rotl 1 = 0x01 (00000001)
    let result = bitrot::rotate_left(&[0x80], 1, 8).unwrap();
    assert_eq!(result, vec![0x01]);
}

#[test]
fn test_rotate_right_8_by_1() {
    // 0x01 (00000001) rotr 1 = 0x80 (10000000)
    let result = bitrot::rotate_right(&[0x01], 1, 8).unwrap();
    assert_eq!(result, vec![0x80]);
}

#[test]
fn test_rotate_left_roundtrip_8() {
    let data = vec![0xAB, 0xCD];
    let rotated = bitrot::rotate_left(&data, 3, 8).unwrap();
    let restored = bitrot::rotate_right(&rotated, 3, 8).unwrap();
    assert_eq!(restored, data);
}

#[test]
fn test_rotate_left_8_full_rotation() {
    let data = vec![0xAB];
    let result = bitrot::rotate_left(&data, 8, 8).unwrap();
    assert_eq!(result, data);
}

#[test]
fn test_rotate_right_8_full_rotation() {
    let data = vec![0xCD];
    let result = bitrot::rotate_right(&data, 8, 8).unwrap();
    assert_eq!(result, data);
}

#[test]
fn test_rotate_left_16() {
    // 0x0001 rotl 1 = 0x0002
    let result = bitrot::rotate_left(&[0x00, 0x01], 1, 16).unwrap();
    assert_eq!(result, vec![0x00, 0x02]);
}

#[test]
fn test_rotate_right_16() {
    // 0x0002 rotr 1 = 0x0001
    let result = bitrot::rotate_right(&[0x00, 0x02], 1, 16).unwrap();
    assert_eq!(result, vec![0x00, 0x01]);
}

#[test]
fn test_rotate_left_roundtrip_16() {
    let data = vec![0xAB, 0xCD];
    let rotated = bitrot::rotate_left(&data, 5, 16).unwrap();
    let restored = bitrot::rotate_right(&rotated, 5, 16).unwrap();
    assert_eq!(restored, data);
}

#[test]
fn test_rotate_left_32() {
    // 0x00000001 rotl 1 = 0x00000002
    let result = bitrot::rotate_left(&[0x00, 0x00, 0x00, 0x01], 1, 32).unwrap();
    assert_eq!(result, vec![0x00, 0x00, 0x00, 0x02]);
}

#[test]
fn test_rotate_right_32() {
    // 0x00000002 rotr 1 = 0x00000001
    let result = bitrot::rotate_right(&[0x00, 0x00, 0x00, 0x02], 1, 32).unwrap();
    assert_eq!(result, vec![0x00, 0x00, 0x00, 0x01]);
}

#[test]
fn test_rotate_left_roundtrip_32() {
    let data = vec![0xDE, 0xAD, 0xBE, 0xEF];
    let rotated = bitrot::rotate_left(&data, 13, 32).unwrap();
    let restored = bitrot::rotate_right(&rotated, 13, 32).unwrap();
    assert_eq!(restored, data);
}

#[test]
fn test_empty_input() {
    let result = bitrot::rotate_left(&[], 1, 8).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_width() {
    let result = bitrot::rotate_left(&[0xFF], 1, 7);
    assert!(result.is_err());
}

#[test]
fn test_invalid_16_alignment() {
    let result = bitrot::rotate_left(&[0xFF], 1, 16);
    assert!(result.is_err());
}

#[test]
fn test_invalid_32_alignment() {
    let result = bitrot::rotate_left(&[0xFF, 0xFF], 1, 32);
    assert!(result.is_err());
}

#[test]
fn test_multiple_bytes_8() {
    let data = vec![0xFF, 0x00, 0xAA];
    let left = bitrot::rotate_left(&data, 4, 8).unwrap();
    let right = bitrot::rotate_right(&left, 4, 8).unwrap();
    assert_eq!(right, data);
}

use happy_cracking::crypto::hill;

#[test]
fn test_encrypt_2x2_basic() {
    // Key matrix: [[3, 3], [2, 5]]
    // "HI" -> H=7, I=8
    // [3*7+3*8, 2*7+5*8] = [45, 54] mod 26 = [19, 2] -> "TC"
    let result = hill::encrypt("HI", "3 3 2 5").unwrap();
    assert_eq!(result, "TC");
}

#[test]
fn test_decrypt_2x2_basic() {
    let result = hill::decrypt("TC", "3 3 2 5").unwrap();
    assert_eq!(result, "HI");
}

#[test]
fn test_roundtrip_2x2() {
    let key = "3 3 2 5";
    let original = "HELLO";
    let encrypted = hill::encrypt(original, key).unwrap();
    let decrypted = hill::decrypt(&encrypted, key).unwrap();
    // "HELLO" has 5 chars, padded to "HELLOX" (6 chars, 3 pairs for 2x2)
    assert_eq!(&decrypted[..5], original);
}

#[test]
fn test_encrypt_3x3() {
    // Key matrix: [[6, 24, 1], [13, 16, 10], [20, 17, 15]]
    let key = "6 24 1 13 16 10 20 17 15";
    let result = hill::encrypt("ACT", key).unwrap();
    // A=0, C=2, T=19
    // [6*0+24*2+1*19, 13*0+16*2+10*19, 20*0+17*2+15*19] = [67, 222, 319] mod 26 = [15, 14, 7] -> "POH"
    assert_eq!(result, "POH");
}

#[test]
fn test_decrypt_3x3() {
    let key = "6 24 1 13 16 10 20 17 15";
    let result = hill::decrypt("POH", key).unwrap();
    assert_eq!(result, "ACT");
}

#[test]
fn test_roundtrip_3x3() {
    let key = "6 24 1 13 16 10 20 17 15";
    let original = "ATTACK";
    let encrypted = hill::encrypt(original, key).unwrap();
    let decrypted = hill::decrypt(&encrypted, key).unwrap();
    assert_eq!(decrypted, original);
}

#[test]
fn test_padding_with_x() {
    // "AB" with 3x3 key should pad to "ABX"
    let key = "6 24 1 13 16 10 20 17 15";
    let encrypted = hill::encrypt("AB", key).unwrap();
    assert_eq!(encrypted.len(), 3);
    let decrypted = hill::decrypt(&encrypted, key).unwrap();
    assert_eq!(decrypted, "ABX");
}

#[test]
fn test_empty_input() {
    assert_eq!(hill::encrypt("", "3 3 2 5").unwrap(), "");
    assert_eq!(hill::decrypt("", "3 3 2 5").unwrap(), "");
}

#[test]
fn test_non_square_key_error() {
    assert!(hill::encrypt("HELLO", "1 2 3").is_err());
}

#[test]
fn test_non_invertible_key_error() {
    // [[2, 4], [6, 8]] has det = 2*8 - 4*6 = -8; gcd(18, 26) != 1
    assert!(hill::decrypt("AB", "2 4 6 8").is_err());
}

#[test]
fn test_decrypt_wrong_length_error() {
    // Ciphertext length 3 is not divisible by 2 (2x2 matrix)
    assert!(hill::decrypt("ABC", "3 3 2 5").is_err());
}

#[test]
fn test_ctf_flag() {
    let key = "3 3 2 5";
    let original = "FLAGXY";
    let encrypted = hill::encrypt(original, key).unwrap();
    let decrypted = hill::decrypt(&encrypted, key).unwrap();
    assert_eq!(decrypted, original);
}

#[test]
fn test_lowercase_treated_as_uppercase() {
    let enc_upper = hill::encrypt("HELLO", "3 3 2 5").unwrap();
    let enc_lower = hill::encrypt("hello", "3 3 2 5").unwrap();
    assert_eq!(enc_upper, enc_lower);
}

#[test]
fn test_non_alpha_ignored() {
    let enc1 = hill::encrypt("H I", "3 3 2 5").unwrap();
    let enc2 = hill::encrypt("HI", "3 3 2 5").unwrap();
    assert_eq!(enc1, enc2);
}

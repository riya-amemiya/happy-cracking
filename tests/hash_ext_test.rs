use happy_cracking::crypto::hash_ext;
use sha2::{Digest, Sha256};

// Verifies the hash length extension attack by computing the expected result
// directly: SHA-256(secret || message || padding || append) must equal the
// hash produced by the extension function.
fn verify_extension(secret: &[u8], message: &[u8], append: &[u8]) {
    // Compute H(secret || message)
    let mut hasher = Sha256::new();
    hasher.update(secret);
    hasher.update(message);
    let original_hash = hex::encode(hasher.finalize());

    let original_len = (secret.len() + message.len()) as u64;

    // Perform the extension attack
    let result = hash_ext::sha256_extend(&original_hash, original_len, append).unwrap();

    // Build the full forged message: secret || message || forged_suffix
    let mut full_message = Vec::new();
    full_message.extend_from_slice(secret);
    full_message.extend_from_slice(message);
    full_message.extend_from_slice(&result.forged_suffix);

    // Compute the expected hash directly
    let mut expected_hasher = Sha256::new();
    expected_hasher.update(&full_message);
    let expected_hash = hex::encode(expected_hasher.finalize());

    assert_eq!(
        result.new_hash, expected_hash,
        "Extension hash does not match direct SHA-256 computation"
    );

    // Verify the forged suffix starts with padding and ends with append data
    let suffix_len = result.forged_suffix.len();
    assert!(suffix_len > append.len());
    assert_eq!(&result.forged_suffix[suffix_len - append.len()..], append);

    // Verify padding starts with 0x80
    assert_eq!(result.forged_suffix[0], 0x80);
}

#[test]
fn extend_basic() {
    verify_extension(b"secret", b"data", b"append");
}

#[test]
fn extend_empty_message() {
    verify_extension(b"secretkey", b"", b"admin=true");
}

#[test]
fn extend_long_secret() {
    verify_extension(b"a_very_long_secret_key_for_testing", b"msg", b"extra");
}

#[test]
fn extend_ctf_style() {
    verify_extension(b"key", b"user=guest", b"&admin=true");
}

#[test]
fn extend_single_byte_append() {
    verify_extension(b"secret", b"data", b"x");
}

#[test]
fn extend_large_message() {
    let message = b"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    verify_extension(b"key", message, b"end");
}

// Tests that cross a block boundary (message + secret > 55 bytes,
// causing the original padding to span two blocks)
#[test]
fn extend_crosses_block_boundary() {
    let secret = b"0123456789abcdef0123456789abcdef";
    let message = b"0123456789abcdef0123456789abcdef";
    verify_extension(secret, message, b"ext");
}

#[test]
fn extend_invalid_hash_length() {
    let result = hash_ext::sha256_extend("aabbcc", 10, b"data");
    assert!(result.is_err());
}

#[test]
fn extend_invalid_hex() {
    let result = hash_ext::sha256_extend(
        "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz",
        10,
        b"data",
    );
    assert!(result.is_err());
}

#[test]
fn extend_flag_format() {
    verify_extension(b"hmac_secret", b"flag{hash_", b"extension}");
}

fn dummy_sha256_hex() -> &'static str {
    // Valid 32-byte hash (SHA-256 of empty string); contents unused on overflow reject.
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
}

#[test]
fn extend_rejects_bit_length_overflow() {
    // SHA-256 length is a 64-bit bit count; byte_len * 8 must not overflow.
    let too_big = u64::MAX / 8 + 1;
    match hash_ext::sha256_extend(dummy_sha256_hex(), too_big, b"x") {
        Ok(_) => panic!("expected overflow error"),
        Err(err) => {
            let msg = format!("{err:#}").to_ascii_lowercase();
            assert!(
                msg.contains("length") || msg.contains("64-bit"),
                "unexpected error: {msg}"
            );
        }
    }
}

#[test]
fn extend_rejects_padded_length_overflow() {
    // original_len * 8 fits, but adding SHA-256 padding exceeds the 64-bit field.
    let max_bytes = u64::MAX / 8;
    match hash_ext::sha256_extend(dummy_sha256_hex(), max_bytes, b"") {
        Ok(_) => panic!("expected overflow error"),
        Err(err) => {
            let msg = format!("{err:#}").to_ascii_lowercase();
            assert!(
                msg.contains("length") || msg.contains("overflow") || msg.contains("64-bit"),
                "unexpected error: {msg}"
            );
        }
    }
}

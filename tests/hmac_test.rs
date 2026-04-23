use happy_cracking::crypto::hmac;

// RFC 2104 test vector
#[test]
fn test_hmac_md5_rfc2104() {
    let key = vec![0x0b; 16];
    assert_eq!(
        hmac::hmac_md5(&key, b"Hi There"),
        "9294727a3638bb1c13f48ef8158bfc9d"
    );
}

#[test]
fn test_hmac_md5_jefe() {
    assert_eq!(
        hmac::hmac_md5(b"Jefe", b"what do ya want for nothing?"),
        "750c783e6ab0b503eaa86e310a5db738"
    );
}

// RFC 4231 test vectors
#[test]
fn test_hmac_sha256_rfc4231_case1() {
    let key = vec![0x0b; 20];
    assert_eq!(
        hmac::hmac_sha256(&key, b"Hi There"),
        "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
    );
}

#[test]
fn test_hmac_sha256_jefe() {
    assert_eq!(
        hmac::hmac_sha256(b"Jefe", b"what do ya want for nothing?"),
        "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
    );
}

#[test]
fn test_hmac_sha1_rfc2202_case1() {
    let key = vec![0x0b; 20];
    assert_eq!(
        hmac::hmac_sha1(&key, b"Hi There"),
        "b617318655057264e28bc0b6fb378c8ef146be00"
    );
}

#[test]
fn test_hmac_sha1_jefe() {
    assert_eq!(
        hmac::hmac_sha1(b"Jefe", b"what do ya want for nothing?"),
        "effcdf6ae5eb2fa2d27416d5f184df9c259a7c79"
    );
}

#[test]
fn test_hmac_sha512_rfc4231_case1() {
    let key = vec![0x0b; 20];
    assert_eq!(
        hmac::hmac_sha512(&key, b"Hi There"),
        "87aa7cdea5ef619d4ff0b4241a1d6cb02379f4e2ce4ec2787ad0b30545e17cdedaa833b7d6b8a702038b274eaea3f4e4be9d914eeb61f1702e696c203a126854"
    );
}

#[test]
fn test_hmac_empty_message() {
    let result = hmac::hmac_sha256(b"key", b"");
    assert_eq!(result.len(), 64);
}

#[test]
fn test_hmac_long_key() {
    // Key longer than block size (64 bytes) should be hashed first
    let key = vec![0xaa; 131];
    let msg = b"Test Using Larger Than Block-Size Key - Hash Key First";
    let result = hmac::hmac_sha256(&key, msg);
    assert_eq!(
        result,
        "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
    );
}

#[test]
fn test_verify_md5_accepts_matching_tag() {
    let key = b"Jefe";
    let msg = b"what do ya want for nothing?";
    let tag = hmac::hmac_md5(key, msg);
    assert!(hmac::verify_md5(key, msg, &tag));
}

#[test]
fn test_verify_md5_rejects_flipped_bit() {
    let key = b"Jefe";
    let msg = b"what do ya want for nothing?";
    let mut tag = hmac::hmac_md5(key, msg);
    tag.replace_range(0..1, if tag.starts_with('0') { "1" } else { "0" });
    assert!(!hmac::verify_md5(key, msg, &tag));
}

#[test]
fn test_verify_sha1_accepts_matching_tag() {
    let key = vec![0x0b; 20];
    let msg = b"Hi There";
    let tag = hmac::hmac_sha1(&key, msg);
    assert!(hmac::verify_sha1(&key, msg, &tag));
}

#[test]
fn test_verify_sha256_accepts_matching_tag() {
    let key = b"Jefe";
    let msg = b"what do ya want for nothing?";
    let tag = hmac::hmac_sha256(key, msg);
    assert!(hmac::verify_sha256(key, msg, &tag));
}

#[test]
fn test_verify_sha256_rejects_last_byte_flip() {
    let key = b"Jefe";
    let msg = b"what do ya want for nothing?";
    let mut tag = hmac::hmac_sha256(key, msg);
    let last = tag.pop().expect("non-empty tag");
    let replacement = if last == 'f' { '0' } else { 'f' };
    tag.push(replacement);
    assert!(!hmac::verify_sha256(key, msg, &tag));
}

#[test]
fn test_verify_sha256_rejects_wrong_key() {
    let msg = b"Hi There";
    let tag = hmac::hmac_sha256(b"correct-key", msg);
    assert!(!hmac::verify_sha256(b"wrong-key!!", msg, &tag));
}

#[test]
fn test_verify_sha512_accepts_matching_tag() {
    let key = vec![0x0b; 20];
    let msg = b"Hi There";
    let tag = hmac::hmac_sha512(&key, msg);
    assert!(hmac::verify_sha512(&key, msg, &tag));
}

#[test]
fn test_verify_rejects_malformed_hex_tag() {
    assert!(!hmac::verify_sha256(b"k", b"m", "zz"));
    assert!(!hmac::verify_sha256(b"k", b"m", "abc"));
}

#[test]
fn test_verify_rejects_wrong_length_tag() {
    assert!(!hmac::verify_sha256(b"k", b"m", "deadbeef"));
}

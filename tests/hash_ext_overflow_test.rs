use happy_cracking::crypto::hash_ext;

/// Security regression test: sha256_extend must reject original_len values
/// that would cause integer overflow in bit-length calculations.
/// Without the fix, `original_len * 8` wraps around silently, producing
/// incorrect forged hashes without any error.
#[test]
fn sha256_extend_rejects_overflowing_original_len() {
    // A valid 32-byte SHA-256 hash (all zeros)
    let fake_hash = "0000000000000000000000000000000000000000000000000000000000000000";

    // original_len = u64::MAX would overflow when multiplied by 8
    let result = hash_ext::sha256_extend(fake_hash, u64::MAX, b"data");
    assert!(
        result.is_err(),
        "sha256_extend must reject original_len=u64::MAX to prevent overflow"
    );

    // original_len just above the safe maximum (u64::MAX / 8)
    let result = hash_ext::sha256_extend(fake_hash, u64::MAX / 8 + 1, b"data");
    assert!(
        result.is_err(),
        "sha256_extend must reject original_len > u64::MAX/8"
    );
}

/// Verify that values at the safe boundary still work.
#[test]
fn sha256_extend_accepts_boundary_original_len() {
    let fake_hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    // original_len = 0 should work fine (edge case)
    let result = hash_ext::sha256_extend(fake_hash, 0, b"data");
    assert!(result.is_ok(), "sha256_extend should accept original_len=0");
}

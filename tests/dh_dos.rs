use happy_cracking::crypto::dh;
use num_bigint::BigUint;

#[test]
fn test_dh_bsgs_dos() {
    let g = BigUint::from(2u32);
    let target = BigUint::from(5u32);
    let p = BigUint::from(29u32);
    // Extremely large order, sqrt is ~2^32, which would allocate ~4 billion hashmap entries
    let order_str = "18446744073709551616"; // 2^64
    let order = order_str.parse::<BigUint>().unwrap();

    // This should return an error quickly, not hang or OOM
    let result = dh::baby_step_giant_step(&g, &target, &p, &order);
    assert!(result.is_err());
}

#[test]
fn test_pubkey_rejects_oversized_modulus() {
    let g = BigUint::from(2u32);
    let a = BigUint::from(6u32);
    let p = BigUint::from(1u32) << dh::MAX_DH_BITS;
    let err = dh::compute_pubkey(&g, &a, &p).unwrap_err();
    assert!(err.to_string().contains("exceeds the maximum allowed"));
}

#[test]
fn test_pubkey_rejects_oversized_exponent() {
    let g = BigUint::from(2u32);
    let a = BigUint::from(1u32) << dh::MAX_DH_BITS;
    let p = BigUint::from(23u32);
    let err = dh::compute_pubkey(&g, &a, &p).unwrap_err();
    assert!(err.to_string().contains("exceeds the maximum allowed"));
}

#[test]
fn test_pubkey_accepts_modulus_at_bit_limit() {
    let g = BigUint::from(2u32);
    let a = BigUint::from(1u32);
    let p = BigUint::from(1u32) << (dh::MAX_DH_BITS - 1);
    assert_eq!(p.bits(), dh::MAX_DH_BITS);
    assert_eq!(dh::compute_pubkey(&g, &a, &p).unwrap(), g);
}

#[test]
fn test_shared_secret_rejects_oversized_modulus() {
    let pk = BigUint::from(8u32);
    let a = BigUint::from(15u32);
    let p = BigUint::from(1u32) << dh::MAX_DH_BITS;
    let err = dh::compute_shared_secret(&pk, &a, &p).unwrap_err();
    assert!(err.to_string().contains("exceeds the maximum allowed"));
}

#[test]
fn test_bsgs_rejects_oversized_modulus() {
    let g = BigUint::from(2u32);
    let target = BigUint::from(5u32);
    let p = BigUint::from(1u32) << dh::MAX_DH_BITS;
    let order = BigUint::from(28u32);
    let err = dh::baby_step_giant_step(&g, &target, &p, &order).unwrap_err();
    assert!(err.to_string().contains("exceeds the maximum allowed"));
}

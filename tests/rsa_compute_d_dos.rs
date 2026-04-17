use happy_cracking::crypto::rsa::compute_d;
use num_bigint::BigUint;

#[test]
fn test_compute_d_rejects_zero_p() {
    let p = BigUint::from(0u32);
    let q = BigUint::from(53u32);
    let e = BigUint::from(17u32);
    let err = compute_d(&p, &q, &e).expect_err("must error on p == 0");
    assert!(err.to_string().contains("non-zero"));
}

#[test]
fn test_compute_d_rejects_zero_q() {
    let p = BigUint::from(61u32);
    let q = BigUint::from(0u32);
    let e = BigUint::from(17u32);
    let err = compute_d(&p, &q, &e).expect_err("must error on q == 0");
    assert!(err.to_string().contains("non-zero"));
}

#[test]
fn test_compute_d_works_for_textbook_values() {
    let p = BigUint::from(61u32);
    let q = BigUint::from(53u32);
    let e = BigUint::from(17u32);
    let d = compute_d(&p, &q, &e).expect("must succeed for valid parameters");
    assert_eq!(d, BigUint::from(2753u32));
}

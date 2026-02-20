use happy_cracking::crypto::ec::{ECPoint, is_on_curve, point_add};
use num_bigint::BigInt;

#[test]
fn test_ec_zero_modulus_handling() {
    let p = BigInt::from(0);
    let a = BigInt::from(1);
    let b = BigInt::from(1);
    let pt = ECPoint::Affine {
        x: BigInt::from(1),
        y: BigInt::from(1),
    };

    // This should now return false instead of panicking
    assert_eq!(is_on_curve(&pt, &a, &b, &p), false);

    // This should now return an error
    let result = point_add(&pt, &pt, &a, &p);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().to_string(),
        "Modulus p must be non-zero"
    );
}

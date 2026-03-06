use happy_cracking::crypto::dh;
use num_bigint::BigUint;
use num_traits::One;

#[test]
fn test_dh_bsgs_dos() {
    let g = BigUint::from(2u32);
    let target = BigUint::from(10u32);
    let p = BigUint::from(23u32);
    // Huge order that will exhaust memory
    let order = BigUint::one() << 60;
    let res = dh::baby_step_giant_step(&g, &target, &p, &order);
    assert!(res.is_err(), "Expected error for excessively large order");
}

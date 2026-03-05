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

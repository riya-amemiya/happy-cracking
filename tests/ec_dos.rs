use num_bigint::BigUint;
use num_traits::ToPrimitive;
use std::str::FromStr;

#[test]
fn test_factor_large_composite_dos_protection() {
    // This test verifies that factoring a large composite number (which would
    // trigger a hang with naive trial division) completes quickly using Pollard's Rho.

    // P = 1099511627791 (prime, 2^40 + 15)
    let p_str = "1099511627791";
    // Q = 618970019642690137449562111 (prime, 2^89 - 1)
    let q_str = "618970019642690137449562111";

    let p = BigUint::from_str(p_str).unwrap();
    let q = BigUint::from_str(q_str).unwrap();
    let n = &p * &q;

    // Ensure n > u128::MAX to trigger BigUint path
    assert!(n.to_u128().is_none(), "n should be larger than u128");

    // This call should finish quickly (< 1s in release, < 10s in debug)
    // If it used trial division, it would take ~10^12 iterations (hours/days).
    // Note: factorize_biguint is available in primes module.
    let factors = happy_cracking::crypto::primes::factorize_biguint(n.clone());

    // Verify factors: P and Q.
    assert_eq!(factors.len(), 2);
    // Factors are sorted
    assert_eq!(factors[0].0, p);
    assert_eq!(factors[0].1, 1);
    assert_eq!(factors[1].0, q);
    assert_eq!(factors[1].1, 1);
}

use happy_cracking::crypto::ec::{self, EcAction};
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

#[test]
fn test_pohlig_hellman_dos_large_order() {
    // This test attempts to trigger OOM by providing a large prime order to Pohlig-Hellman.
    // The BSGS algorithm attempts to allocate sqrt(order) entries.
    // Order = 2^61 - 1 (Mersenne prime), sqrt approx 2^30.5 (1.5 billion entries).
    // This should fail gracefully with the fix.

    // Using a simple curve: y^2 = x^3 + x + 6 mod 1000000007
    // Point (2, 4) is on the curve.

    let action = EcAction::PohligHellman {
        generator: "2,4".to_string(),
        target: "2,4".to_string(),
        a: "1".to_string(),
        b: "6".to_string(),
        p: "1000000007".to_string(),
        order: "2305843009213693951".to_string(), // 2^61 - 1
    };

    let result = ec::run(action);

    // We expect an error.
    assert!(result.is_err());

    let err_msg = result.unwrap_err().to_string();
    println!("Error message: {}", err_msg);
    assert!(err_msg.contains("Order too large") || err_msg.contains("limit"));
}

#[test]
fn test_point_order_dos_large_p() {
    // This test attempts to trigger an infinite loop (DoS) in point_order
    // by providing a curve with a large order that exceeds the brute-force limit.

    // p = 1000000007 (approx 10^9), which is > 2^22 (approx 4*10^6).
    // The order of a point is likely near p.

    let action = EcAction::Order {
        point: "2,4".to_string(),
        a: "1".to_string(),
        b: "6".to_string(),
        p: "1000000007".to_string(),
    };

    let result = ec::run(action);

    // We expect an error due to iteration limit.
    assert!(result.is_err());

    let err_msg = result.unwrap_err().to_string();
    println!("Error message: {}", err_msg);
    assert!(err_msg.contains("limit exceeded") || err_msg.contains("Point order calculation limit"));
}

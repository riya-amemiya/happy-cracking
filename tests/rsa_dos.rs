use happy_cracking::crypto::rsa;
use num_bigint::BigUint;
use num_traits::{One, Zero};

#[test]
fn test_common_modulus_zero_modulus_panic() {
    let n = BigUint::zero();
    let e1 = BigUint::from(19u32);
    let e2 = BigUint::from(17u32);
    let c1 = BigUint::from(1u32);
    let c2 = BigUint::from(2u32);

    let res = rsa::common_modulus_attack(&n, &e1, &e2, &c1, &c2);
    assert!(res.is_err());
}

#[test]
fn test_hastad_zero_modulus_panic() {
    let c1 = BigUint::from(1u32);
    let c2 = BigUint::from(2u32);
    let c3 = BigUint::from(3u32);
    let n1 = BigUint::zero(); // Panic here
    let n2 = BigUint::from(5u32);
    let n3 = BigUint::from(7u32);
    let e = 3;

    let cs = vec![c1, c2, c3];
    let ns = vec![n1, n2, n3];

    let res = rsa::hastad_broadcast(&cs, &ns, e);
    assert!(res.is_err());
}

#[test]
fn test_pollard_p1_zero_modulus_no_panic() {
    let n = BigUint::zero();
    let b = 100;
    let res = rsa::pollard_p1(&n, b);
    assert!(res.is_err());
}

#[test]
fn test_pollard_rho_factor_zero_modulus_no_panic() {
    let n = BigUint::zero();
    // pollard_rho_factor was moved to primes::pollard_rho_biguint
    let res = happy_cracking::crypto::primes::pollard_rho_biguint(&n);
    assert!(res.is_err());
}

#[test]
fn test_fermat_factor_zero_modulus_no_panic() {
    let n = BigUint::zero();
    let res = rsa::fermat_factor(&n, 100);
    assert!(res.is_err());
}

#[test]
fn test_wiener_attack_zero_modulus_no_panic() {
    let n = BigUint::zero();
    let e = BigUint::from(17u32);
    let res = rsa::wiener_attack(&e, &n);
    assert!(res.is_err());
}

#[test]
fn test_fermat_factor_one_modulus() {
    let n = BigUint::one();
    let res = rsa::fermat_factor(&n, 100);
    assert!(res.is_err());
}
#[test]
fn test_pollard_p1_dos_large_bound() {
    let n = BigUint::from(3233u32);
    let b = rsa::MAX_POLLARD_P1_BOUND + 1;
    let res = rsa::pollard_p1(&n, b);
    assert!(res.is_err());
    let err_msg = res.unwrap_err().to_string();
    assert!(err_msg.contains("exceeds the maximum allowed limit"));
}

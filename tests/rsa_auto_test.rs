use num_bigint::BigUint;
use num_traits::One;

use happy_cracking::crypto::rsa;

#[test]
fn auto_small_e_recovers_message() {
    let m = BigUint::from_bytes_be(b"flag");
    let e = BigUint::from(3u32);
    let c = m.pow(3);
    // n larger than c so small-e path applies without mod
    let n = BigUint::from(10u32).pow(20);
    let result = rsa::auto_attack(&n, &e, Some(&c), 1000, 10_000).unwrap();
    assert!(result.method.starts_with("small-e"));
    assert_eq!(result.m.unwrap(), m);
}

#[test]
fn auto_fermat_close_primes() {
    // Close primes: 10007 and 10009
    let p = BigUint::from(10007u32);
    let q = BigUint::from(10009u32);
    let n = &p * &q;
    let e = BigUint::from(65537u32);
    // Ensure e is coprime to phi
    let phi = (&p - BigUint::one()) * (&q - BigUint::one());
    let d = rsa::big_modinv(&e, &phi).unwrap();
    let m = BigUint::from(42u32);
    let c = rsa::big_modpow(&m, &e, &n).unwrap();

    let result = rsa::auto_attack(&n, &e, Some(&c), 1000, 100_000).unwrap();
    assert!(
        result.method == "fermat"
            || result.method == "pollard-rho"
            || result.method == "pollard-p1"
            || result.method == "wiener",
        "unexpected method {}",
        result.method
    );
    if let Some(recovered) = result.m {
        assert_eq!(recovered, m);
    } else if let (Some(rp), Some(rq)) = (result.p, result.q) {
        assert_eq!(&rp * &rq, n);
        let _ = d;
    } else {
        panic!("expected factors or message");
    }
}

#[test]
fn auto_pollard_rho_small_semiprime() {
    let p = BigUint::from(61u32);
    let q = BigUint::from(53u32);
    let n = &p * &q; // 3233
    let e = BigUint::from(17u32);
    let m = BigUint::from(65u32);
    let c = rsa::big_modpow(&m, &e, &n).unwrap();
    let result = rsa::auto_attack(&n, &e, Some(&c), 10_000, 100_000).unwrap();
    assert_eq!(result.m.unwrap(), m);
}

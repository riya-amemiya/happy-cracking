use num_bigint::BigUint;
use num_traits::One;

use happy_cracking::crypto::rsa;

// --- Hastad's Broadcast Attack tests ---

#[test]
fn test_hastad_broadcast_e3() {
    // e=3, message m=42, three different moduli
    let m = BigUint::from(42u32);
    let e = 3u32;

    let n1 = BigUint::from(1013u32) * BigUint::from(1019u32); // 1032247
    let n2 = BigUint::from(1021u32) * BigUint::from(1031u32); // 1052651
    let n3 = BigUint::from(1033u32) * BigUint::from(1039u32); // 1073287

    let c1 = m.modpow(&BigUint::from(e), &n1);
    let c2 = m.modpow(&BigUint::from(e), &n2);
    let c3 = m.modpow(&BigUint::from(e), &n3);

    let recovered = rsa::hastad_broadcast(&[c1, c2, c3], &[n1, n2, n3], e).unwrap();
    assert_eq!(recovered, m);
}

#[test]
fn test_hastad_broadcast_e3_flag() {
    // CTF-style: small numeric message that fits well under moduli
    let m = BigUint::from(999999u32);
    let e = 3u32;

    let n1 = BigUint::from(104729u64) * BigUint::from(104743u64);
    let n2 = BigUint::from(104759u64) * BigUint::from(104761u64);
    let n3 = BigUint::from(104773u64) * BigUint::from(104779u64);

    let c1 = m.modpow(&BigUint::from(e), &n1);
    let c2 = m.modpow(&BigUint::from(e), &n2);
    let c3 = m.modpow(&BigUint::from(e), &n3);

    let recovered = rsa::hastad_broadcast(&[c1, c2, c3], &[n1, n2, n3], e).unwrap();
    assert_eq!(recovered, m);
}

#[test]
fn test_hastad_broadcast_insufficient_ciphertexts() {
    let c1 = BigUint::from(100u32);
    let n1 = BigUint::from(1000u32);
    // e=3 but only 1 pair provided
    assert!(rsa::hastad_broadcast(&[c1], &[n1], 3).is_err());
}

#[test]
fn test_hastad_broadcast_e_zero() {
    assert!(rsa::hastad_broadcast(&[], &[], 0).is_err());
}

// --- Common Modulus Attack tests ---

#[test]
fn test_common_modulus_basic() {
    // p=61, q=53, n=3233
    let p = BigUint::from(61u32);
    let q = BigUint::from(53u32);
    let n = &p * &q;
    let e1 = BigUint::from(17u32);
    let e2 = BigUint::from(19u32);
    let m = BigUint::from(42u32);

    let c1 = rsa::big_modpow(&m, &e1, &n).unwrap();
    let c2 = rsa::big_modpow(&m, &e2, &n).unwrap();

    let recovered = rsa::common_modulus_attack(&n, &e1, &e2, &c1, &c2).unwrap();
    assert_eq!(recovered, m);
}

#[test]
fn test_common_modulus_larger() {
    // p=1009, q=1013, n=1022117
    let p = BigUint::from(1009u32);
    let q = BigUint::from(1013u32);
    let n = &p * &q;
    let e1 = BigUint::from(65537u32);
    let e2 = BigUint::from(17u32);
    let m = BigUint::from(12345u32);

    let c1 = rsa::big_modpow(&m, &e1, &n).unwrap();
    let c2 = rsa::big_modpow(&m, &e2, &n).unwrap();

    let recovered = rsa::common_modulus_attack(&n, &e1, &e2, &c1, &c2).unwrap();
    assert_eq!(recovered, m);
}

#[test]
fn test_common_modulus_flag() {
    // m must be < n for correct RSA
    let p = BigUint::from(104729u64);
    let q = BigUint::from(104743u64);
    let n = &p * &q;
    let e1 = BigUint::from(65537u32);
    let e2 = BigUint::from(3u32);
    let m = BigUint::from(123456u32);

    let c1 = rsa::big_modpow(&m, &e1, &n).unwrap();
    let c2 = rsa::big_modpow(&m, &e2, &n).unwrap();

    let recovered = rsa::common_modulus_attack(&n, &e1, &e2, &c1, &c2).unwrap();
    assert_eq!(recovered, m);
}

#[test]
fn test_common_modulus_non_coprime_exponents() {
    // gcd(e1, e2) != 1 should fail
    let n = BigUint::from(3233u32);
    let e1 = BigUint::from(6u32);
    let e2 = BigUint::from(10u32); // gcd(6,10) = 2
    let c1 = BigUint::from(100u32);
    let c2 = BigUint::from(200u32);
    assert!(rsa::common_modulus_attack(&n, &e1, &e2, &c1, &c2).is_err());
}

// --- Pollard's p-1 Attack tests ---

#[test]
fn test_pollard_p1_smooth() {
    // p = 1049 (p-1 = 1048 = 2^3 * 131, B-smooth for B >= 131)
    // q = 1061 (q-1 = 1060 = 2^2 * 5 * 53)
    let p = BigUint::from(1049u32);
    let q = BigUint::from(1061u32);
    let n = &p * &q; // 1112989

    let (fp, fq) = rsa::pollard_p1(&n, 1000).unwrap();
    assert_eq!(&fp * &fq, n);
}

#[test]
fn test_pollard_p1_classic_smooth() {
    // p = 23 (p-1 = 22 = 2 * 11), q = 29 (q-1 = 28 = 2^2 * 7)
    // Use a larger bound so the periodic GCD check catches it
    let p = BigUint::from(23u32);
    let q = BigUint::from(29u32);
    let n = &p * &q; // 667

    let (fp, fq) = rsa::pollard_p1(&n, 1000).unwrap();
    assert_eq!(&fp * &fq, n);
}

#[test]
fn test_pollard_p1_even() {
    let n = BigUint::from(100u32);
    let (p, q) = rsa::pollard_p1(&n, 100).unwrap();
    assert_eq!(&p * &q, n);
    assert_eq!(p, BigUint::from(2u32));
}

#[test]
fn test_pollard_p1_full_ctf_scenario() {
    // Factor n, compute d, decrypt
    let p = BigUint::from(1049u32);
    let q = BigUint::from(1061u32);
    let n = &p * &q;
    let e = BigUint::from(65537u32);
    let m = BigUint::from(42u32);

    let c = rsa::big_modpow(&m, &e, &n).unwrap();

    let (fp, fq) = rsa::pollard_p1(&n, 1000).unwrap();
    let phi = (&fp - BigUint::one()) * (&fq - BigUint::one());
    let d = rsa::big_modinv(&e, &phi).unwrap();
    let decrypted = rsa::big_modpow(&c, &d, &n).unwrap();
    assert_eq!(decrypted, m);
}

// --- Pollard's Rho tests ---

#[test]
fn test_pollard_rho_small() {
    // n = 91 = 7 * 13
    let n = BigUint::from(91u32);
    let (p, q) = rsa::pollard_rho_factor(&n).unwrap();
    assert_eq!(&p * &q, n);
}

#[test]
fn test_pollard_rho_medium() {
    // n = 1022117 = 1009 * 1013
    let p = BigUint::from(1009u32);
    let q = BigUint::from(1013u32);
    let n = &p * &q;
    let (fp, fq) = rsa::pollard_rho_factor(&n).unwrap();
    assert_eq!(&fp * &fq, n);
}

#[test]
fn test_pollard_rho_even() {
    let n = BigUint::from(100u32);
    let (p, q) = rsa::pollard_rho_factor(&n).unwrap();
    assert_eq!(&p * &q, n);
    assert_eq!(p, BigUint::from(2u32));
}

#[test]
fn test_pollard_rho_larger() {
    // n = 10403 = 101 * 103
    let n = BigUint::from(10403u32);
    let (p, q) = rsa::pollard_rho_factor(&n).unwrap();
    assert_eq!(&p * &q, n);
}

#[test]
fn test_pollard_rho_full_ctf_scenario() {
    // Factor n via rho, then decrypt. e must be coprime to phi.
    let p = BigUint::from(101u32);
    let q = BigUint::from(103u32);
    let n = &p * &q; // 10403, phi = 100*102 = 10200
    let e = BigUint::from(7u32); // gcd(7, 10200) = 1
    let m = BigUint::from(65u32);

    let c = rsa::big_modpow(&m, &e, &n).unwrap();

    let (fp, fq) = rsa::pollard_rho_factor(&n).unwrap();
    let phi = (&fp - BigUint::one()) * (&fq - BigUint::one());
    let d = rsa::big_modinv(&e, &phi).unwrap();
    let decrypted = rsa::big_modpow(&c, &d, &n).unwrap();
    assert_eq!(decrypted, m);
}

// --- Combined attack scenario ---

#[test]
fn test_hastad_then_verify_roundtrip() {
    // Encrypt m=99 under 3 different RSA keys with e=3, then recover via Hastad
    let m = BigUint::from(99u32);
    let e = 3u32;

    let pairs: Vec<(u32, u32)> = vec![(101, 103), (107, 109), (113, 127)];
    let mut cs = Vec::new();
    let mut ns = Vec::new();

    for (p, q) in &pairs {
        let n = BigUint::from(*p) * BigUint::from(*q);
        let c = m.modpow(&BigUint::from(e), &n);
        cs.push(c);
        ns.push(n);
    }

    let recovered = rsa::hastad_broadcast(&cs, &ns, e).unwrap();
    assert_eq!(recovered, m);
}

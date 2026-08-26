use num_bigint::BigUint;
use num_traits::One;

use happy_cracking::crypto::rsa;

#[test]
fn test_compute_d_small() {
    // Classic RSA textbook example: p=61, q=53, e=17 -> d=2753
    let p = BigUint::from(61u32);
    let q = BigUint::from(53u32);
    let e = BigUint::from(17u32);
    let phi = (&p - BigUint::one()) * (&q - BigUint::one()); // 3120
    let d = rsa::big_modinv(&e, &phi).unwrap();
    assert_eq!(d, BigUint::from(2753u32));
}

#[test]
fn test_encrypt_decrypt_roundtrip() {
    // p=61, q=53, n=3233, e=17, d=2753
    let n = BigUint::from(3233u32);
    let e = BigUint::from(17u32);
    let d = BigUint::from(2753u32);
    let m = BigUint::from(65u32);

    let c = rsa::big_modpow(&m, &e, &n).unwrap();
    let decrypted = rsa::big_modpow(&c, &d, &n).unwrap();
    assert_eq!(decrypted, m);
}

#[test]
fn test_encrypt_decrypt_roundtrip_larger() {
    // p=257, q=263, n=67591, e=17
    let p = BigUint::from(257u32);
    let q = BigUint::from(263u32);
    let n = &p * &q; // 67591
    let e = BigUint::from(17u32);
    let phi = (&p - BigUint::one()) * (&q - BigUint::one()); // 67072
    let d = rsa::big_modinv(&e, &phi).unwrap();

    let m = BigUint::from(12345u32);
    let c = rsa::big_modpow(&m, &e, &n).unwrap();
    let decrypted = rsa::big_modpow(&c, &d, &n).unwrap();
    assert_eq!(decrypted, m);
}

#[test]
fn test_modinv_basic() {
    let a = BigUint::from(3u32);
    let m = BigUint::from(26u32);
    let inv = rsa::big_modinv(&a, &m).unwrap();
    // 3 * 9 = 27 ≡ 1 (mod 26)
    assert_eq!(inv, BigUint::from(9u32));
}

#[test]
fn test_modinv_no_inverse() {
    let a = BigUint::from(2u32);
    let m = BigUint::from(4u32);
    assert!(rsa::big_modinv(&a, &m).is_err());
}

#[test]
fn test_small_e_attack() {
    // e=3, m is small enough that m^3 < n (no modular reduction)
    let m = BigUint::from(100u32);
    let e = 3u32;
    let c = m.pow(e); // 1000000
    let recovered = rsa::integer_nth_root(&c, e);
    assert_eq!(recovered, m);
}

#[test]
fn test_small_e_attack_flag() {
    // Simulate CTF: message is "flag" as bytes -> integer
    let flag_bytes = b"flag";
    let m = BigUint::from_bytes_be(flag_bytes);
    let e = 3u32;
    let c = m.pow(e);
    let recovered = rsa::integer_nth_root(&c, e);
    assert_eq!(recovered, m);
    let ascii = rsa::bigint_to_ascii(&recovered);
    assert_eq!(ascii, "flag");
}

#[test]
fn test_fermat_factor_close_primes() {
    // p=101, q=103 are very close primes
    let p = BigUint::from(101u32);
    let q = BigUint::from(103u32);
    let n = &p * &q; // 10403
    let (fp, fq) = rsa::fermat_factor(&n, 1_000_000).unwrap();
    // Order may vary; check that the product matches
    assert_eq!(&fp * &fq, n);
    assert!(
        (fp == BigUint::from(101u32) && fq == BigUint::from(103u32))
            || (fp == BigUint::from(103u32) && fq == BigUint::from(101u32))
    );
}

#[test]
fn test_fermat_factor_even() {
    let n = BigUint::from(100u32);
    let (p, q) = rsa::fermat_factor(&n, 1_000_000).unwrap();
    assert_eq!(&p * &q, n);
}

#[test]
fn test_fermat_factor_larger_close_primes() {
    // p=1009, q=1013
    let p = BigUint::from(1009u32);
    let q = BigUint::from(1013u32);
    let n = &p * &q; // 1022117
    let (fp, fq) = rsa::fermat_factor(&n, 1_000_000).unwrap();
    assert_eq!(&fp * &fq, n);
}

#[test]
fn test_wiener_attack() {
    // Construct a vulnerable RSA key where d is small.
    // p=503, q=509, n=256027, phi=255016
    // d=7 satisfies Wiener bound: d < n^(1/4)/3 ≈ 7.50
    let p = BigUint::from(503u32);
    let q = BigUint::from(509u32);
    let n = &p * &q; // 256027
    let phi = (&p - BigUint::one()) * (&q - BigUint::one()); // 255016
    let d_expected = BigUint::from(7u32);
    let e = rsa::big_modinv(&d_expected, &phi).unwrap(); // 36431

    let recovered_d = rsa::wiener_attack(&e, &n).unwrap();

    let m = BigUint::from(42u32);
    let c = rsa::big_modpow(&m, &e, &n).unwrap();
    let decrypted = rsa::big_modpow(&c, &recovered_d, &n).unwrap();
    assert_eq!(decrypted, m);
}

#[test]
fn test_bigint_to_ascii_hello() {
    let m = BigUint::from_bytes_be(b"Hello");
    let ascii = rsa::bigint_to_ascii(&m);
    assert_eq!(ascii, "Hello");
}

#[test]
fn test_bigint_to_ascii_flag() {
    let m = BigUint::from_bytes_be(b"flag{rsa_is_fun}");
    let ascii = rsa::bigint_to_ascii(&m);
    assert_eq!(ascii, "flag{rsa_is_fun}");
}

#[test]
fn test_bigint_to_ascii_zero() {
    let m = BigUint::from(0u32);
    let ascii = rsa::bigint_to_ascii(&m);
    assert_eq!(ascii, "");
}

#[test]
fn test_bigint_to_ascii_non_printable() {
    // Bytes with non-printable characters should return empty
    let m = BigUint::from(1u32); // 0x01, non-printable
    let ascii = rsa::bigint_to_ascii(&m);
    assert_eq!(ascii, "");
}

#[test]
fn test_integer_nth_root_square() {
    let n = BigUint::from(144u32);
    assert_eq!(rsa::integer_nth_root(&n, 2), BigUint::from(12u32));
}

#[test]
fn test_integer_nth_root_cube() {
    let n = BigUint::from(1000u32);
    assert_eq!(rsa::integer_nth_root(&n, 3), BigUint::from(10u32));
}

#[test]
fn test_integer_nth_root_zero() {
    let n = BigUint::from(0u32);
    assert_eq!(rsa::integer_nth_root(&n, 3), BigUint::from(0u32));
}

#[test]
fn test_integer_nth_root_one() {
    let n = BigUint::from(1u32);
    assert_eq!(rsa::integer_nth_root(&n, 5), BigUint::from(1u32));
}

#[test]
fn test_integer_nth_root_imperfect() {
    // sqrt(10) should be 3 (floor)
    let n = BigUint::from(10u32);
    assert_eq!(rsa::integer_nth_root(&n, 2), BigUint::from(3u32));
}

#[test]
fn test_full_rsa_ctf_scenario() {
    let p = BigUint::from(127u32);
    let q = BigUint::from(131u32);
    let n = &p * &q; // 16637
    let e = BigUint::from(65537u32);
    let phi = (&p - BigUint::one()) * (&q - BigUint::one());
    let d = rsa::big_modinv(&e, &phi).unwrap();

    let m = BigUint::from(9999u32);
    let c = rsa::big_modpow(&m, &e, &n).unwrap();

    let (fp, fq) = rsa::fermat_factor(&n, 1_000_000).unwrap();
    assert_eq!(&fp * &fq, n);

    let recovered_phi = (&fp - BigUint::one()) * (&fq - BigUint::one());
    let recovered_d = rsa::big_modinv(&e, &recovered_phi).unwrap();
    assert_eq!(recovered_d, d);

    let decrypted = rsa::big_modpow(&c, &recovered_d, &n).unwrap();
    assert_eq!(decrypted, m);
}

#[test]
fn test_integer_nth_root_zero_k() {
    let n = BigUint::from(27u32);
    let result = rsa::integer_nth_root(&n, 0);
    assert_eq!(result, BigUint::from(0u32));
}

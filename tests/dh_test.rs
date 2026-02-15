use num_bigint::BigUint;

use happy_cracking::crypto::dh;

#[test]
fn test_pubkey_basic() {
    // g=2, p=23, a=6 -> 2^6 mod 23 = 64 mod 23 = 18
    let g = BigUint::from(2u32);
    let p = BigUint::from(23u32);
    let a = BigUint::from(6u32);
    assert_eq!(dh::compute_pubkey(&g, &a, &p), BigUint::from(18u32));
}

#[test]
fn test_pubkey_larger() {
    // g=5, p=23, a=15 -> 5^15 mod 23
    let g = BigUint::from(5u32);
    let p = BigUint::from(23u32);
    let a = BigUint::from(15u32);
    let expected = g.modpow(&a, &p);
    assert_eq!(dh::compute_pubkey(&g, &a, &p), expected);
}

#[test]
fn test_shared_secret_agreement() {
    // Classic DH key exchange: both parties compute the same shared secret.
    let g = BigUint::from(5u32);
    let p = BigUint::from(23u32);
    let a = BigUint::from(6u32);
    let b = BigUint::from(15u32);

    let pub_a = dh::compute_pubkey(&g, &a, &p);
    let pub_b = dh::compute_pubkey(&g, &b, &p);

    let secret_a = dh::compute_shared_secret(&pub_b, &a, &p);
    let secret_b = dh::compute_shared_secret(&pub_a, &b, &p);

    assert_eq!(secret_a, secret_b);
}

#[test]
fn test_shared_secret_larger_prime() {
    // p=101 (prime), g=2
    let g = BigUint::from(2u32);
    let p = BigUint::from(101u32);
    let a = BigUint::from(37u32);
    let b = BigUint::from(73u32);

    let pub_a = dh::compute_pubkey(&g, &a, &p);
    let pub_b = dh::compute_pubkey(&g, &b, &p);

    let secret_a = dh::compute_shared_secret(&pub_b, &a, &p);
    let secret_b = dh::compute_shared_secret(&pub_a, &b, &p);

    assert_eq!(secret_a, secret_b);
}

#[test]
fn test_bsgs_basic() {
    // g=2, p=29, order=28 (since 2 is a generator of Z/29Z*)
    // Find x such that 2^x = 5 mod 29
    let g = BigUint::from(2u32);
    let p = BigUint::from(29u32);
    let target = BigUint::from(5u32);
    let order = BigUint::from(28u32);

    let x = dh::baby_step_giant_step(&g, &target, &p, &order).unwrap();
    // Verify: g^x mod p = target
    assert_eq!(g.modpow(&x, &p), target);
}

#[test]
fn test_bsgs_dlog_one() {
    // g^0 = 1 mod p for any g, p
    let g = BigUint::from(3u32);
    let p = BigUint::from(17u32);
    let target = BigUint::from(1u32);
    let order = BigUint::from(16u32);

    let x = dh::baby_step_giant_step(&g, &target, &p, &order).unwrap();
    assert_eq!(g.modpow(&x, &p), target);
}

#[test]
fn test_bsgs_dlog_generator() {
    // g^1 = g mod p
    let g = BigUint::from(3u32);
    let p = BigUint::from(17u32);
    let target = BigUint::from(3u32);
    let order = BigUint::from(16u32);

    let x = dh::baby_step_giant_step(&g, &target, &p, &order).unwrap();
    assert_eq!(g.modpow(&x, &p), target);
}

#[test]
fn test_bsgs_ctf_scenario() {
    // Simulate CTF: given public key, recover private key via BSGS.
    // g=2, p=101, order=100
    let g = BigUint::from(2u32);
    let p = BigUint::from(101u32);
    let order = BigUint::from(100u32);
    let secret = BigUint::from(42u32);

    let pub_key = dh::compute_pubkey(&g, &secret, &p);
    let recovered = dh::baby_step_giant_step(&g, &pub_key, &p, &order).unwrap();

    // The recovered value may differ from secret by a multiple of order,
    // but g^recovered should equal the public key.
    assert_eq!(g.modpow(&recovered, &p), pub_key);
}

#[test]
fn test_full_dh_exchange_and_break() {
    // Full scenario: DH exchange, then break using BSGS.
    let g = BigUint::from(5u32);
    let p = BigUint::from(23u32);
    let order = BigUint::from(22u32);
    let a_priv = BigUint::from(6u32);
    let b_priv = BigUint::from(15u32);

    let a_pub = dh::compute_pubkey(&g, &a_priv, &p);
    let b_pub = dh::compute_pubkey(&g, &b_priv, &p);

    // Shared secret via normal exchange
    let shared = dh::compute_shared_secret(&b_pub, &a_priv, &p);

    // Attacker uses BSGS to recover a's private key from a_pub
    let recovered_a = dh::baby_step_giant_step(&g, &a_pub, &p, &order).unwrap();

    // Attacker computes shared secret using recovered key
    let broken_secret = dh::compute_shared_secret(&b_pub, &recovered_a, &p);
    assert_eq!(shared, broken_secret);
}

#[test]
fn test_pubkey_exponent_zero() {
    // g^0 mod p = 1
    let g = BigUint::from(7u32);
    let p = BigUint::from(23u32);
    let a = BigUint::from(0u32);
    assert_eq!(dh::compute_pubkey(&g, &a, &p), BigUint::from(1u32));
}

#[test]
fn test_pubkey_exponent_one() {
    // g^1 mod p = g mod p
    let g = BigUint::from(7u32);
    let p = BigUint::from(23u32);
    let a = BigUint::from(1u32);
    assert_eq!(dh::compute_pubkey(&g, &a, &p), BigUint::from(7u32));
}

#[test]
fn test_bsgs_order_zero() {
    let g = BigUint::from(2u32);
    let p = BigUint::from(23u32);
    let target = BigUint::from(5u32);
    let order = BigUint::from(0u32);
    assert!(dh::baby_step_giant_step(&g, &target, &p, &order).is_err());
}

use num_bigint::{BigInt, BigUint};
use num_traits::One;

use happy_cracking::crypto::ec;

// Test curve: y^2 = x^3 + x + 6 mod 11

fn small_curve_a() -> BigInt {
    BigInt::from(1)
}

fn small_curve_p() -> BigInt {
    BigInt::from(11)
}

#[test]
fn test_point_add_identity_left() {
    let a = small_curve_a();
    let p = small_curve_p();
    let pt = ec::ECPoint::Affine {
        x: BigInt::from(2),
        y: BigInt::from(4),
    };
    let result = ec::point_add(&ec::ECPoint::Infinity, &pt, &a, &p).unwrap();
    assert_eq!(result, pt);
}

#[test]
fn test_point_add_identity_right() {
    let a = small_curve_a();
    let p = small_curve_p();
    let pt = ec::ECPoint::Affine {
        x: BigInt::from(2),
        y: BigInt::from(4),
    };
    let result = ec::point_add(&pt, &ec::ECPoint::Infinity, &a, &p).unwrap();
    assert_eq!(result, pt);
}

#[test]
fn test_point_add_inverse() {
    // P + (-P) = O
    let a = small_curve_a();
    let p = small_curve_p();
    let pt = ec::ECPoint::Affine {
        x: BigInt::from(2),
        y: BigInt::from(4),
    };
    let neg_pt = ec::ECPoint::Affine {
        x: BigInt::from(2),
        y: BigInt::from(7), // -4 mod 11 = 7
    };
    let result = ec::point_add(&pt, &neg_pt, &a, &p).unwrap();
    assert_eq!(result, ec::ECPoint::Infinity);
}

#[test]
fn test_point_doubling() {
    // y^2 = x^3 + 2x + 3 mod 97
    let a = BigInt::from(2);
    let p = BigInt::from(97);
    let pt = ec::ECPoint::Affine {
        x: BigInt::from(3),
        y: BigInt::from(6),
    };
    // Verify point is on curve: 6^2 = 36, 3^3 + 2*3 + 3 = 27 + 6 + 3 = 36 mod 97. Good.
    let result = ec::point_add(&pt, &pt, &a, &p).unwrap();
    if let ec::ECPoint::Affine { x, y } = &result {
        // Verify on curve: y^2 = x^3 + 2x + 3 mod 97
        let lhs = (y * y) % &p;
        let rhs = ((x * x * x) + &a * x + BigInt::from(3)) % &p;
        let lhs_pos = ((lhs % &p) + &p) % &p;
        let rhs_pos = ((rhs % &p) + &p) % &p;
        assert_eq!(lhs_pos, rhs_pos, "Doubled point must be on curve");
    }
}

#[test]
fn test_scalar_multiply_zero() {
    let a = small_curve_a();
    let p = small_curve_p();
    let pt = ec::ECPoint::Affine {
        x: BigInt::from(2),
        y: BigInt::from(4),
    };
    let result = ec::scalar_multiply(&pt, &BigUint::from(0u32), &a, &p).unwrap();
    assert_eq!(result, ec::ECPoint::Infinity);
}

#[test]
fn test_scalar_multiply_one() {
    let a = small_curve_a();
    let p = small_curve_p();
    let pt = ec::ECPoint::Affine {
        x: BigInt::from(2),
        y: BigInt::from(4),
    };
    let result = ec::scalar_multiply(&pt, &BigUint::one(), &a, &p).unwrap();
    assert_eq!(result, pt);
}

#[test]
fn test_scalar_multiply_two_equals_doubling() {
    let a = BigInt::from(2);
    let p = BigInt::from(97);
    let pt = ec::ECPoint::Affine {
        x: BigInt::from(3),
        y: BigInt::from(6),
    };
    let doubled = ec::point_add(&pt, &pt, &a, &p).unwrap();
    let multiplied = ec::scalar_multiply(&pt, &BigUint::from(2u32), &a, &p).unwrap();
    assert_eq!(doubled, multiplied);
}

#[test]
fn test_scalar_multiply_order() {
    // y^2 = x^3 + 2x + 3 mod 97
    // If n*P = O, then n is a multiple of the point order.
    let a = BigInt::from(2);
    let p = BigInt::from(97);
    let pt = ec::ECPoint::Affine {
        x: BigInt::from(3),
        y: BigInt::from(6),
    };
    let order = ec::point_order(&pt, &a, &p).unwrap();
    let result = ec::scalar_multiply(&pt, &order, &a, &p).unwrap();
    assert_eq!(result, ec::ECPoint::Infinity);
}

#[test]
fn test_point_order_basic() {
    // y^2 = x^3 + 2x + 3 mod 97, point (3, 6)
    let a = BigInt::from(2);
    let p = BigInt::from(97);
    let pt = ec::ECPoint::Affine {
        x: BigInt::from(3),
        y: BigInt::from(6),
    };
    let order = ec::point_order(&pt, &a, &p).unwrap();
    assert!(order > BigUint::one());
    let result = ec::scalar_multiply(&pt, &order, &a, &p).unwrap();
    assert_eq!(result, ec::ECPoint::Infinity);
}

#[test]
fn test_point_order_infinity() {
    let a = small_curve_a();
    let p = small_curve_p();
    let order = ec::point_order(&ec::ECPoint::Infinity, &a, &p).unwrap();
    assert_eq!(order, BigUint::one());
}

#[test]
fn test_parse_point_affine() {
    let pt = ec::parse_point("3,6").unwrap();
    assert_eq!(
        pt,
        ec::ECPoint::Affine {
            x: BigInt::from(3),
            y: BigInt::from(6)
        }
    );
}

#[test]
fn test_parse_point_infinity() {
    assert_eq!(ec::parse_point("inf").unwrap(), ec::ECPoint::Infinity);
    assert_eq!(ec::parse_point("O").unwrap(), ec::ECPoint::Infinity);
    assert_eq!(ec::parse_point("infinity").unwrap(), ec::ECPoint::Infinity);
}

#[test]
fn test_parse_point_invalid() {
    assert!(ec::parse_point("1,2,3").is_err());
    assert!(ec::parse_point("abc").is_err());
}

#[test]
fn test_format_point() {
    let pt = ec::ECPoint::Affine {
        x: BigInt::from(3),
        y: BigInt::from(6),
    };
    assert_eq!(ec::format_point(&pt), "(3, 6)");
    assert_eq!(ec::format_point(&ec::ECPoint::Infinity), "inf");
}

#[test]
fn test_pohlig_hellman_simple() {
    // y^2 = x^3 + 2x + 3 mod 97
    let a = BigInt::from(2);
    let p = BigInt::from(97);
    let generator = ec::ECPoint::Affine {
        x: BigInt::from(3),
        y: BigInt::from(6),
    };
    let order = ec::point_order(&generator, &a, &p).unwrap();

    let n_secret = BigUint::from(42u32);
    let target = ec::scalar_multiply(&generator, &n_secret, &a, &p).unwrap();

    let recovered = ec::pohlig_hellman(&generator, &target, &a, &p, &order).unwrap();

    let check = ec::scalar_multiply(&generator, &recovered, &a, &p).unwrap();
    assert_eq!(check, target);
}

#[test]
fn test_scalar_multiply_associativity() {
    // (a*b)*P = a*(b*P)
    let a_curve = BigInt::from(2);
    let p = BigInt::from(97);
    let pt = ec::ECPoint::Affine {
        x: BigInt::from(3),
        y: BigInt::from(6),
    };
    let a = BigUint::from(5u32);
    let b = BigUint::from(7u32);
    let ab = &a * &b;

    let lhs = ec::scalar_multiply(&pt, &ab, &a_curve, &p).unwrap();
    let bp = ec::scalar_multiply(&pt, &b, &a_curve, &p).unwrap();
    let rhs = ec::scalar_multiply(&bp, &a, &a_curve, &p).unwrap();
    assert_eq!(lhs, rhs);
}

#[test]
fn test_point_add_commutativity() {
    let a = BigInt::from(2);
    let p = BigInt::from(97);
    let p1 = ec::ECPoint::Affine {
        x: BigInt::from(3),
        y: BigInt::from(6),
    };
    let p2 = ec::scalar_multiply(&p1, &BigUint::from(5u32), &a, &p).unwrap();
    let sum1 = ec::point_add(&p1, &p2, &a, &p).unwrap();
    let sum2 = ec::point_add(&p2, &p1, &a, &p).unwrap();
    assert_eq!(sum1, sum2);
}

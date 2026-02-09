use num_bigint::BigUint;
// use num_traits::One; // Not needed if not used directly
// use num_integer::Integer;

fn main() {
    let n = BigUint::from(17u32);
    // This method doesn't exist in 0.4.6 by default unless `rand` feature is enabled?
    // And even then, it's usually `is_probabable_prime(iterations)`.
    // Let's try to compile and see error.
    // println!("{}", n.is_probable_prime(20));
}

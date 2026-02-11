## 2026-02-09 - Trial Division Denial of Service
**Vulnerability:** Naive trial division for primality testing on 128-bit numbers allows for indefinite hanging (DoS) when processing large primes.
**Learning:** Even constrained integer types like u128 are large enough to make O(sqrt(N)) algorithms impractical. Security tools must use efficient algorithms (like Miller-Rabin) to prevent self-DoS.
**Prevention:** Use probabilistic primality tests for large numbers and limit trial division to small factors.

## 2026-02-12 - Division by Zero in Cipher Operations
**Vulnerability:** Cipher implementation using modulo operator on key length (e.g., `i % key.len()`) panicked when key was empty.
**Learning:** Rust's type safety doesn't prevent runtime logic errors like division by zero. Always validate inputs that are used as divisors, especially in loop conditions.
**Prevention:** Check for empty keys/inputs at the start of functions or use `checked_rem` if panic is not acceptable.

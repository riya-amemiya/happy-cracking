## 2026-02-09 - Trial Division Denial of Service
**Vulnerability:** Naive trial division for primality testing on 128-bit numbers allows for indefinite hanging (DoS) when processing large primes.
**Learning:** Even constrained integer types like u128 are large enough to make O(sqrt(N)) algorithms impractical. Security tools must use efficient algorithms (like Miller-Rabin) to prevent self-DoS.
**Prevention:** Use probabilistic primality tests for large numbers and limit trial division to small factors.

## 2026-02-12 - Division by Zero in Cipher Operations
**Vulnerability:** Cipher implementation using modulo operator on key length (e.g., `i % key.len()`) panicked when key was empty.
**Learning:** Rust's type safety doesn't prevent runtime logic errors like division by zero. Always validate inputs that are used as divisors, especially in loop conditions.
**Prevention:** Check for empty keys/inputs at the start of functions or use `checked_rem` if panic is not acceptable.

## 2026-02-12 - Panic on Mathematical Overflow/Zero
**Vulnerability:** `modpow` panicked with modulus 0 (div by zero) and `lcm` panicked on large inputs (overflow).
**Learning:** Mathematical utility functions must handle edge cases (0, overflow) gracefully, especially when exposed to user input via CLI, as panics cause DoS.
**Prevention:** Return `Result` instead of raw types for math functions, and use `checked_` arithmetic operations.

## 2026-02-13 - Panic in RSA Nth Root Calculation
**Vulnerability:** `integer_nth_root` function panicked when the root exponent `k` was 0, causing a division by zero in `div_ceil`. This could be triggered via the CLI `rsa small-e` command with `--e 0`.
**Learning:** Mathematical functions accepting external inputs (like exponents) must validate that inputs are within valid ranges (e.g. > 0) before performing operations that might panic (division, modulo).
**Prevention:** Add input validation guards at the start of mathematical functions and in CLI argument parsing logic to reject invalid parameters like zero exponents.

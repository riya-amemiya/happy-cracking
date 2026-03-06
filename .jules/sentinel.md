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

## 2026-02-13 - DoS via Chain Reaction
**Vulnerability:** The `chain` command allowed unbounded sequencing of operations (e.g., repeating `base64-encode`), leading to exponential memory growth (Zip Bomb) and process crash/hang.
**Learning:** Flexible "pipeline" features in CLI tools are vectors for DoS if not constrained by recursion limits or output size limits.
**Prevention:** Enforce strict limits on operation count and intermediate/final output size for any user-definable processing pipeline.

## 2026-02-16 - RSA Denial of Service
**Vulnerability:** A denial-of-service vulnerability existed in the RSA module where a zero modulus caused a panic in `BigUint::modpow`, crashing the application.
**Learning:** External library functions like `num-bigint`'s `modpow` may panic on specific invalid inputs (like modulus 0) instead of returning an error. Assuming they are safe is dangerous.
**Prevention:** Always validate mathematical inputs (especially divisors and moduli) before passing them to external libraries, or wrap them in functions that return `Result`.

## 2026-02-18 - Algorithm Confusion via JSON Shadowing
**Vulnerability:** The JWT parser used a naive string search to extract the "alg" header, allowing an attacker to hide the real algorithm (e.g., "none") by injecting a fake "alg" string in a preceding field value or key, causing the security scanner to report "unknown" or the wrong algorithm.
**Learning:** Custom parsers for structured data (like JSON) are extremely fragile and prone to "parser differential" attacks where the security tool sees something different than the actual verifier.
**Prevention:** Always use established, robust parsing libraries (like `serde_json`) for structured data, especially when making security decisions based on the content.

## 2026-02-19 - Panic on RSA Zero Modulus
**Vulnerability:** Several RSA attack functions (`common_modulus_attack`, `hastad_broadcast`) panicked when provided with a zero modulus because `num-bigint` operations like modulo and division by zero panic.
**Learning:** Mathematical libraries often panic on invalid inputs (like division by zero) rather than returning errors. Input validation is critical for any function exposed to user input, especially for mathematical parameters like moduli.
**Prevention:** Explicitly check that moduli are non-zero (and preferably > 1) at the beginning of any cryptographic function and return a proper error.

## 2026-02-19 - Panic in Polybius Square Lookup
**Vulnerability:** `find_in_square` used `unwrap()` on search result, assuming input sanitization would always prevent invalid characters. A future change bypassing sanitization could trigger a panic (DoS).
**Learning:** Defense in depth requires internal functions to be robust against invalid inputs, even if sanitization exists at the boundary. `unwrap()` should be avoided in shared utility functions.
**Prevention:** Return `Option` or `Result` from lookup functions and handle missing values explicitly in the caller.

## 2026-02-20 - Panic in Elliptic Curve Operations
**Vulnerability:** `modp` and `mod_inverse` in EC module panicked on zero modulus, allowing DoS via CLI arguments.
**Learning:** Mathematical helper functions in cryptographic implementations often assume valid inputs (e.g. p > 0) and panic otherwise. Input validation must happen at the API boundary (CLI or public function) to prevent this.
**Prevention:** Add explicit `p.is_zero()` checks in all public functions accepting a modulus.

## 2026-02-23 - DoS via Naive Factorization Fallback
**Vulnerability:** The `factor_biguint` helper function fell back to naive trial division for numbers exceeding `u128::MAX` (which are common in Elliptic Curve cryptography), causing indefinite hangs (DoS) on large inputs.
**Learning:** Security tools dealing with large numbers (like `BigUint`) must avoid algorithms with exponential time complexity (like trial division) as fallbacks.
**Prevention:** Implement and use efficient algorithms like Pollard's Rho for `BigUint` factorization to handle large composites gracefully.

## 2026-02-23 - Breaking Changes in Refactoring
**Vulnerability:** Moving utility functions (like `pollard_rho_factor`) from one module to another broke existing tests that relied on the old location, causing CI failure.
**Learning:** When refactoring security primitives, always check for test files that import specific functions, not just the main library code. `grep` is your friend.
**Prevention:** Run `cargo test` locally before submitting, even for "simple" refactors.

## 2026-02-25 - Unbounded Loop in Pollard's Rho DoS
**Vulnerability:** Pollard's Rho factorization algorithm for `u128` (up to 128-bit integers) lacked an iteration limit, causing infinite loops on hard-to-factor semiprimes.
**Learning:** Mathematical algorithms with probabilistic runtime (like Pollard's Rho) must always have an upper bound on iterations when exposed to user input to prevent Algorithmic Complexity DoS.
**Prevention:** Always add a safety loop counter or timeout to `while` loops in cryptographic/mathematical primitives.

## 2025-03-05 - Missing BSGS limit in DH implementation allows DoS
**Vulnerability:** The Baby-step Giant-step (BSGS) implementation in `src/crypto/dh.rs` lacked a safety limit for its iteration count (`m`), which dynamically depended on the `order` parameter provided by the user. By passing an extremely large `order` (e.g., $2^{64}$), an attacker could exhaust memory and CPU due to allocating an excessive number of elements in a `HashMap`.
**Learning:** In cryptography implementations, parameters defining loop bounds or dynamic allocations (such as hash maps in BSGS or similar time-memory tradeoff algorithms) must always be bounded by a hardcoded maximum (like `MAX_BSGS_ITERATIONS`), even if they rely on "expected" logical relationships like the order of a group.
**Prevention:** Always bound dynamically computed capacities and iterations to a safe maximum using constants like `const MAX_ITERATIONS = 1 << 22;` before performing memory allocation or large loops.

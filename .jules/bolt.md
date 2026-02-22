## 2026-02-11 - BigUint Overhead
**Learning:** Using `BigUint` for modular arithmetic on numbers that fit in native types (`u64`, `u128`) introduces massive overhead (~28x slower for primality test).
**Action:** Always prefer native integer arithmetic (`u128` for intermediate calculations) when inputs are small enough, especially in hot loops like primality testing.

## 2026-02-11 - modpow optimization
**Learning:** Generic `modpow` implementations using `BigUint` are costly even for small numbers.
**Action:** Always check if modulus fits in `u64` and dispatch to a `u128`-based fast path. This yielded ~4.8x speedup for `modpow`.

## 2026-02-12 - Factorization Algorithms
**Learning:** Trial division is exponentially slow for large inputs ($O(\sqrt{n})$), making it unusable for 64-bit semi-primes.
**Action:** Use Pollard's Rho ($O(n^{1/4})$) for composite numbers. This reduced factorization time for a 64-bit semi-prime from 22s to 0.035s (~600x speedup).

## 2026-02-14 - Branch Prediction vs Allocation
**Learning:** A naive double-and-add loop for 128-bit modular multiplication (to avoid BigUint allocation) was 20% SLOWER on random inputs due to branch misprediction, despite being 20% faster on constant inputs. BigUint's word-based operations are more predictor-friendly.
**Action:** When optimizing tight loops with random data, prefer branchless algorithms or vectorized operations over simple loops, even if it means keeping allocations.

## 2026-02-15 - Native BigUint Roots
**Learning:** `num-bigint`'s native `nth_root` and `sqrt` methods are significantly faster (~2x for `nth_root`, ~7x for `sqrt`) than a custom Newton's method implementation.
**Action:** Always prefer library implementations for big integer roots over custom arithmetic loops.

## 2026-02-18 - Montgomery Multiplication for u128
**Learning:** Native `u128` arithmetic with manual Montgomery reduction is significantly faster than `BigUint` for modular exponentiation and Pollard's Rho, even without unstable intrinsics like `widening_mul`.
**Action:** Use `Montgomery` struct for heavy modular arithmetic on `u128` inputs to avoid heap allocation overhead.

## 2026-02-19 - Kasiski Examination Complexity
**Learning:** Naively iterating all pairs of occurrences for Kasiski examination results in $O(N^2)$ complexity, causing massive slowdowns on large texts (e.g., 3s for 3.5k chars). Adjacent differences are sufficient and reduce complexity to $O(N)$.
**Action:** Always check loop complexity for pattern matching algorithms. Prefer linear scan (adjacent elements) over quadratic (all pairs) when statistical properties (like GCD) allow.

## 2026-02-19 - Montgomery Optimization for u128 modpow
**Learning:** `BigUint` modpow for `u128` values (specifically > `u64::MAX`) is ~6.9x slower than a manual Montgomery implementation on `u128`.
**Action:** When working with fixed-width large integers (like `u128`), use specialized modular arithmetic structs (like `Montgomery`) instead of generic `BigUint` to avoid allocation overhead.

## 2026-02-20 - Montgomery Initialization Optimization
**Learning:** Initializing `Montgomery` struct with `BigUint` to compute $R^2 \pmod m$ incurs significant allocation overhead (~50% of runtime). Replacing it with an iterative bitwise doubling loop (`u128`) yields ~2x speedup despite potential branch mispredictions.
**Action:** Avoid `BigUint` even for one-off initializations in hot paths if a simple iterative algorithm exists.

## 2026-02-21 - Pollard's Rho Optimization for u128
**Learning:** Generic `BigUint` implementation of Pollard's Rho is ~300x slower per iteration than `u128` implementation. Additionally, arbitrary iteration limits in `BigUint` implementation can cause failures for factorable numbers (e.g., 120-bit semiprimes).
**Action:** Dispatch factorization of numbers fitting in `u128` to optimized native implementations to enable solving larger instances and improve performance.

## 2026-02-21 - HashMap Overhead in Frequency Analysis
**Learning:** `HashMap<char, usize>` introduces significant overhead for simple frequency analysis on alphabetic input ($O(n \cdot H)$), compared to direct array indexing ($O(n)$).
**Action:** Always prefer `[usize; 26]` over `HashMap` for hot-path character counting when the domain is small and known (e.g., A-Z), yielding ~5x speedup.

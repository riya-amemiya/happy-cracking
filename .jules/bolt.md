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

## 2026-02-16 - Montgomery Multiplication vs Bitwise Reduction
**Learning:** A naive bitwise modular reduction loop for u128 was SLOWER than BigUint (due to branch misprediction). Montgomery multiplication (which is word-based and branchless) yielded a 20x speedup over BigUint.
**Action:** For modular arithmetic with fixed modulus (like Pollard's Rho), always prefer Montgomery multiplication over custom bitwise reduction loops.

## 2026-02-16 - CI Configuration
**Learning:** The CI pipeline runs on the stable Rust channel, which does not support unstable `rustfmt` options. Using them causes lint check failures.
**Action:** When configuring `rustfmt.toml`, only use options available in the stable channel or verify locally with `rustfmt --check` using the stable toolchain.

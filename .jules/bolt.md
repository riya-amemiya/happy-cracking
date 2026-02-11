## 2024-05-22 - Trial Division Optimization
**Learning:** Simple trial division (step 2) is significantly slower than wheel factorization (2, 3) for large composite numbers.
**Action:** Always implement at least wheel factorization (2, 3) or (2, 3, 5) for integer factorization routines to reduce search space by ~33% or more.

## 2024-05-25 - BigUint Overhead
**Learning:** Using `BigUint` for modular arithmetic on numbers that fit in native types (`u64`, `u128`) introduces massive overhead (~28x slower for primality test).
**Action:** Always prefer native integer arithmetic (`u128` for intermediate calculations) when inputs are small enough, especially in hot loops like primality testing.

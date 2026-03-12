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

## 2026-02-23 - Montgomery Multiplication for u64
**Learning:** Native division/remainder (`%`) for `u64` is significantly slower than Montgomery multiplication using `u128` intermediates. Implementing `Montgomery64` yielded ~8.7x speedup for `miller_rabin_u64`.
**Action:** Use specialized Montgomery arithmetic for `u64` modular exponentiation when modulus is odd, avoiding expensive hardware division in hot loops.

## 2026-02-24 - Array vs HashMap for Byte Frequencies
**Learning:** Computing frequencies of 8-bit bytes using `HashMap<u8, usize>` incurs unnecessary allocation and hashing overhead compared to a fixed-size `[usize; 256]` array.
**Action:** Always prefer `[usize; 256]` over `HashMap` for byte-level frequency analysis in hot paths, avoiding $O(N \cdot H)$ operations in favor of $O(N)$.

## 2026-03-02 - Binary Encoding Formatting Overhead
**Learning:** Using `format!("{:08b}", b)` inside an iterator loop to construct strings for binary encoding introduces massive overhead (multiple allocations, dynamic dispatch, formatting parsing). Similarly, using intermediate `Vec<String>` and `.join(" ")` allocates many small strings.
**Action:** When encoding fixed-width binary representations, pre-allocate a single string, compute bits manually into a small byte array `[u8; 8]`, and use `push_str` with `unsafe { std::str::from_utf8_unchecked }`. This yields ~22x speedup.

## 2026-03-02 - ASCII Substitution Overhead
**Learning:** `chars().map().collect()` incurs high overhead for ASCII-only transformations (like Atbash, ROT, Affine) due to Unicode boundary decoding and allocations. Direct byte mutation avoids this.
**Action:** When performing simple character substitutions that are guaranteed to map ASCII to ASCII, convert to a byte vector `let mut bytes = input.as_bytes().to_vec();`, mutate the bytes in place, and reconstruct the string safely using `unsafe { String::from_utf8_unchecked(bytes) }`.

## 2026-03-03 - format! Overhead for Hex Dump Output
**Learning:** Using `format!("{:02x}", b)` inside hot loops for hex dumping incurs significant overhead (formatting macros, dynamic allocations, dynamic dispatch). Replacing it with manual array lookup (`b"0123456789abcdef"`) and appending raw bytes using `unsafe { std::str::from_utf8_unchecked(...) }` yields a ~10x speedup.
**Action:** Always avoid `format!` macros inside tight loops when generating simple repeating structures (like hex bytes or offsets). Instead, allocate a `String` with `with_capacity` and precompute small byte blocks manually before appending.

## 2026-03-07 - Fixed-width Binary Encoding Formatting Overhead
**Learning:** Using `format!("{:05b}", code)` inside a loop and collecting them via `Vec<String>::join` causes massive overhead due to repeated allocations and formatting logic.
**Action:** Always precompute string representations of fixed-width binary chunks in a lookup table when possible (e.g., `["00000", "00001", ...]`), pre-allocate the final string `String::with_capacity(...)`, and push slices directly. This optimization yielded a ~10x speedup for Baudot encoding.

## 2026-03-09 - Formatting and join Overheads for Simple Strings
**Learning:** Using `.map(|c| format!("{}={}", c, c as u32)).collect::<Vec<_>>().join(" ")` for simple repeating character transformations incurs massive overhead due to formatting macros (`format!`), intermediate `Vec` allocations, and intermediate `String` allocations.
**Action:** When constructing strings from many small formatted parts (like character and integer combinations), manually compute the numeric parts into a small byte buffer and append directly to a pre-allocated `String` using `push` and `push_str`. This yields ~20x speedup.

## 2026-03-10 - A1Z26 Formatting Overhead
**Learning:** Using `format!()` and `.to_string()` with dynamic intermediate vectors (`Vec<String>`) inside a loop for `a1z26::encode` generates massive overhead.
**Action:** Replace multiple `.join()` loops by keeping state (`in_number_seq`) to selectively insert separators into a single, pre-allocated `String`. This eliminated intermediate vectors entirely and resulted in a 7.4x performance speedup.

## 2026-03-12 - Dynamic Array Initialization Overhead in Hot Loops
**Learning:** Rebuilding small cryptographic lookup tables (e.g., CRC32 lookup tables) on every function call incurs severe performance penalties in hot paths. Even if the array is small (256 elements), the overhead of dynamic computation and allocation dramatically outweighs the operation time, adding >5x to the runtime.
**Action:** Always precompute fixed lookup tables dynamically at runtime only once using `std::sync::LazyLock` (or `once_cell`), avoiding overhead entirely and making algorithms pure $O(n)$ where $n$ is the data length.

## 2026-03-12 - Polybius format! and Vec overhead
**Learning:** Using `format!("{}{}", row, col)` inside an iterator loop to construct strings for polybius encoding, and `.collect::<Vec<_>>().join(" ")` introduces massive overhead. Similarly, creating temporary vectors like `Vec<usize>` inside decryption loops causes unnecessary allocations.
**Action:** Always pre-allocate a single byte array (`Vec<u8>` or `String::with_capacity()`), compute characters manually, and reconstruct the string safely avoiding `format!` macro overhead. Also avoid generating intermediate vectors in parsing loops when token bytes can be read directly.

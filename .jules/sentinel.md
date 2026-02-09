## 2026-02-09 - Trial Division Denial of Service
**Vulnerability:** Naive trial division for primality testing on 128-bit numbers allows for indefinite hanging (DoS) when processing large primes.
**Learning:** Even constrained integer types like u128 are large enough to make O(sqrt(N)) algorithms impractical. Security tools must use efficient algorithms (like Miller-Rabin) to prevent self-DoS.
**Prevention:** Use probabilistic primality tests for large numbers and limit trial division to small factors.

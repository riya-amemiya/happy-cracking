#[cfg(test)]
mod tests {
    use happy_cracking::crypto::railfence;

    #[test]
    fn test_railfence_encrypt_dos_prevention() {
        // Attempt to encrypt a small string with a huge rail count.
        // Without protection, this might allocate huge memory and crash/hang.

        let input = "Hello, World!";
        let rails = 100_000_000; // 100 million rails

        let start = std::time::Instant::now();
        let result = railfence::encrypt(input, rails);
        let duration = start.elapsed();

        // If it runs successfully without OOM, we want to see what happens.
        // If we implement clamping, result should be "Hello, World!" (identity).
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Hello, World!");

        // Ensure it runs quickly (DoS check)
        assert!(
            duration.as_millis() < 100,
            "Encrypt took too long: {:?}",
            duration
        );
    }

    #[test]
    fn test_railfence_decrypt_dos_prevention() {
        // Attempt to decrypt a small string with a huge rail count.

        let input = "Hello, World!";
        let rails = 100_000_000; // 100 million rails

        let start = std::time::Instant::now();
        let result = railfence::decrypt(input, rails);
        let duration = start.elapsed();

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Hello, World!");

        // Ensure it runs quickly (DoS check)
        assert!(
            duration.as_millis() < 100,
            "Decrypt took too long: {:?}",
            duration
        );
    }
}

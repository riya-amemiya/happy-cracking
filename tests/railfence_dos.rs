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

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Hello, World!");

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

        assert!(
            duration.as_millis() < 100,
            "Decrypt took too long: {:?}",
            duration
        );
    }

    #[test]
    fn test_railfence_bruteforce_rejects_excessive_max_rails() {
        // Attempt to bruteforce with a huge max-rails on empty input.
        // Without a cap, the loop `2..=max_rails` still runs even though
        // decrypt short-circuits, causing CPU / stdout exhaustion.

        let start = std::time::Instant::now();
        let result = railfence::run(railfence::RailFenceAction::Bruteforce {
            input: String::new(),
            max_rails: 100_000_000,
        });
        let duration = start.elapsed();

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Denial of Service"),
            "expected DoS rejection"
        );
        assert!(
            duration.as_millis() < 100,
            "Bruteforce rejection took too long: {:?}",
            duration
        );
    }
}

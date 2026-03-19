use happy_cracking::crypto::columnar;
use std::time::Instant;

#[test]
fn test_columnar_dos() {
    let key = "A".repeat(10_000_000);
    let start = Instant::now();
    let res = columnar::encrypt("A", &key);
    assert!(res.is_err(), "Expected error for massive key");
    assert!(
        res.unwrap_err()
            .to_string()
            .contains("exceeds maximum length")
    );
    assert!(start.elapsed().as_millis() < 100, "Encrypt took too long");
}

#[test]
fn test_columnar_dos_decrypt() {
    let key = "A".repeat(10_000_000);
    let start = Instant::now();
    let res = columnar::decrypt("A", &key);
    assert!(res.is_err(), "Expected error for massive key");
    assert!(
        res.unwrap_err()
            .to_string()
            .contains("exceeds maximum length")
    );
    assert!(start.elapsed().as_millis() < 100, "Decrypt took too long");
}

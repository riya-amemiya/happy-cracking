use happy_cracking::crypto::zipcrack;
use std::io::{Cursor, Write};
use zip::AesMode;
use zip::write::{FileOptions, ZipWriter};

fn make_encrypted_zip(password: &str, content: &str) -> Vec<u8> {
    let mut buf = Cursor::new(Vec::new());
    {
        let mut writer = ZipWriter::new(&mut buf);
        let options: FileOptions<'_, ()> = FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .with_aes_encryption(AesMode::Aes256, password);
        writer.start_file("flag.txt", options).unwrap();
        writer.write_all(content.as_bytes()).unwrap();
        writer.finish().unwrap();
    }
    buf.into_inner()
}

#[test]
fn dict_attack_recovers_password() {
    let bytes = make_encrypted_zip("s3cr3t", "flag{zip_cracked}");
    let words = ["wrong", "nope", "s3cr3t", "other"];
    let found = zipcrack::dict_attack(&bytes, &words);
    assert_eq!(found, Some("s3cr3t".to_string()));
}

#[test]
fn verify_password_accepts_correct() {
    let bytes = make_encrypted_zip("hunter2", "flag{zip_cracked}");
    assert!(zipcrack::verify_password(&bytes, "hunter2"));
}

#[test]
fn verify_password_rejects_wrong() {
    let bytes = make_encrypted_zip("hunter2", "flag{zip_cracked}");
    assert!(!zipcrack::verify_password(&bytes, "wrongpass"));
}

#[test]
fn brute_attack_recovers_short_numeric_password() {
    let bytes = make_encrypted_zip("042", "flag{zip_cracked}");
    let found = zipcrack::brute_attack(&bytes, "0123456789", 1, 3).unwrap();
    assert_eq!(found, Some("042".to_string()));
}

#[test]
fn brute_attack_not_found_returns_none() {
    let bytes = make_encrypted_zip("99", "flag{zip_cracked}");
    let found = zipcrack::brute_attack(&bytes, "abc", 1, 2).unwrap();
    assert_eq!(found, None);
}

#[test]
fn brute_attack_oversized_space_errors() {
    let bytes = make_encrypted_zip("xx", "flag{zip_cracked}");

    let charset: String = (0x20u8..0x7f).map(|b| b as char).collect();
    let result = zipcrack::brute_attack(&bytes, &charset, 1, 6);
    assert!(result.is_err());
}

#[test]
fn list_entries_reports_encrypted() {
    let bytes = make_encrypted_zip("pw", "flag{zip_cracked}");
    let entries = zipcrack::list_entries(&bytes).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "flag.txt");
    assert!(entries[0].encrypted);
}

#[test]
fn verify_password_streams_larger_member_without_failing() {
    // ~64 KiB is well under the 16 MiB cap; this exercises the sink path
    // (no in-memory decompressed buffer) on a bigger payload than the other tests.
    let content = "flag{zip_streamed_verify}\n".repeat(2048);
    let bytes = make_encrypted_zip("hunter2", &content);
    assert!(zipcrack::verify_password(&bytes, "hunter2"));
    assert!(!zipcrack::verify_password(&bytes, "wrongpass"));
}

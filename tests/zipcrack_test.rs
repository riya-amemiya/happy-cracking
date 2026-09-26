use happy_cracking::crypto::zipcrack;
use std::fs;
use std::io::{Cursor, Write};
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use zip::AesMode;
use zip::write::{FileOptions, ZipWriter};

fn scratch_file(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("zipcrack_{tag}_{}_{nanos}", std::process::id()))
}

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
fn brute_attack_rejects_max_len_above_cap() {
    let bytes = make_encrypted_zip("xx", "flag{zip_cracked}");
    let err = zipcrack::brute_attack(&bytes, "a", 1, zipcrack::MAX_BRUTE_LEN + 1).unwrap_err();
    assert!(
        err.to_string()
            .contains(&zipcrack::MAX_BRUTE_LEN.to_string())
    );
}

#[cfg(target_pointer_width = "64")]
#[test]
fn brute_attack_rejects_len_beyond_u32() {
    let bytes = make_encrypted_zip("xx", "flag{zip_cracked}");
    let len = 1usize << 32;
    let err = zipcrack::brute_attack(&bytes, "ab", len, len).unwrap_err();
    assert!(
        err.to_string()
            .contains(&zipcrack::MAX_BRUTE_LEN.to_string())
    );
}

#[test]
fn brute_attack_rejects_charset_line_break() {
    let bytes = make_encrypted_zip("a", "flag{zip_cracked}");
    let err = zipcrack::brute_attack(&bytes, "a\n", 1, 1).unwrap_err();
    assert!(err.to_string().contains("line break"));
}

#[test]
fn brute_attack_deduplicates_charset_like_wordgen() {
    let bytes = make_encrypted_zip("a", "flag{zip_cracked}");
    let found = zipcrack::brute_attack(&bytes, &"a".repeat(100), 1, 5).unwrap();
    assert_eq!(found, Some("a".to_string()));
}

#[test]
fn mask_attack_recovers_pattern_password() {
    let bytes = make_encrypted_zip("AB01Z", "flag{zip_cracked}");
    let found = zipcrack::mask_attack(&bytes, "AB?c?cZ", "01").unwrap();
    assert_eq!(found, Some("AB01Z".to_string()));
}

#[test]
fn mask_attack_supports_literal_question_mark() {
    let bytes = make_encrypted_zip("A?Bx", "flag{zip_cracked}");
    let found = zipcrack::mask_attack(&bytes, "A??B?c", "x").unwrap();
    assert_eq!(found, Some("A?Bx".to_string()));
}

#[test]
fn mask_attack_literal_mask_has_one_candidate() {
    let bytes = make_encrypted_zip("fixed", "flag{zip_cracked}");
    let found = zipcrack::mask_attack(&bytes, "fixed", "ab").unwrap();
    assert_eq!(found, Some("fixed".to_string()));
}

#[test]
fn mask_attack_not_found_returns_none() {
    let bytes = make_encrypted_zip("nope", "flag{zip_cracked}");
    let found = zipcrack::mask_attack(&bytes, "AB?c", "01").unwrap();
    assert_eq!(found, None);
}

#[test]
fn mask_attack_rejects_unknown_token() {
    let bytes = make_encrypted_zip("ab", "flag{zip_cracked}");
    let err = zipcrack::mask_attack(&bytes, "?x", "ab").unwrap_err();
    assert!(err.to_string().contains("Unknown mask token"));
}

#[test]
fn mask_attack_rejects_length_above_cap() {
    let bytes = make_encrypted_zip("a", "flag{zip_cracked}");
    let mask = "a".repeat(zipcrack::MAX_BRUTE_LEN + 1);
    let err = zipcrack::mask_attack(&bytes, &mask, "a").unwrap_err();
    assert!(
        err.to_string()
            .contains(&zipcrack::MAX_BRUTE_LEN.to_string())
    );
}

#[test]
fn mask_attack_rejects_oversized_space() {
    let bytes = make_encrypted_zip("0123456789", "flag{zip_cracked}");
    let err = zipcrack::mask_attack(&bytes, &"?c".repeat(10), "0123456789").unwrap_err();
    assert!(err.to_string().contains("exceeds the limit"));
}

#[test]
fn random_attack_recovers_single_charset_password() {
    let bytes = make_encrypted_zip("zzzz", "flag{zip_cracked}");
    let found = zipcrack::random_attack(&bytes, "z", 4, 8).unwrap();
    assert_eq!(found, Some("zzzz".to_string()));
}

#[test]
fn random_attack_misses_when_charset_cannot_produce_password() {
    let bytes = make_encrypted_zip("b", "flag{zip_cracked}");
    let found = zipcrack::random_attack(&bytes, "a", 1, 5).unwrap();
    assert_eq!(found, None);
}

#[test]
fn random_attack_rejects_zero_count_and_oversized_count() {
    let bytes = make_encrypted_zip("a", "flag{zip_cracked}");
    let zero = zipcrack::random_attack(&bytes, "a", 1, 0).unwrap_err();
    assert!(zero.to_string().contains("count"));
    let oversized = zipcrack::random_attack(&bytes, "ab", 4, 1_000_000_001).unwrap_err();
    assert!(oversized.to_string().contains("exceeds the limit"));
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

#[test]
fn read_zipcrack_bytes_with_limit_rejects_oversized_file() {
    let path = scratch_file("oversize");
    fs::write(&path, vec![b'a'; 32]).unwrap();
    let err = zipcrack::read_zipcrack_bytes_with_limit(&path, 16).unwrap_err();
    let _ = fs::remove_file(&path);
    assert!(err.to_string().contains("Denial of Service"));
}

#[test]
fn read_zipcrack_bytes_with_limit_accepts_file_at_limit() {
    let path = scratch_file("at_limit");
    let data = b"PK\x03\x04tiny";
    fs::write(&path, data).unwrap();
    let got = zipcrack::read_zipcrack_bytes_with_limit(&path, data.len()).unwrap();
    let _ = fs::remove_file(&path);
    assert_eq!(got, data);
}

#[test]
fn dict_run_reads_zip_and_wordlist_under_limit() {
    let zip_path = scratch_file("dict_zip");
    let wl_path = scratch_file("dict_wl");
    let bytes = make_encrypted_zip("s3cr3t", "flag{zip_cracked}");
    fs::write(&zip_path, &bytes).unwrap();
    fs::write(&wl_path, "wrong\ns3cr3t\n").unwrap();
    zipcrack::run(zipcrack::ZipcrackAction::Dict {
        file: zip_path.clone(),
        wordlist: wl_path.clone(),
    })
    .unwrap();
    let _ = fs::remove_file(&zip_path);
    let _ = fs::remove_file(&wl_path);
}

fn run_zipcrack(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_happy-cracking"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn zipcrack_mask_cli_recovers_wordgen_pattern() {
    let path = scratch_file("cli_mask");
    fs::write(&path, make_encrypted_zip("AB01Z", "flag{zip_cracked}")).unwrap();
    let output = run_zipcrack(&[
        "zipcrack",
        "mask",
        "--file",
        path.to_str().unwrap(),
        "--mask",
        "AB?c?cZ",
        "--charset",
        "01",
    ]);
    let _ = fs::remove_file(&path);
    assert!(output.status.success(), "{:?}", output.stderr);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "Found password: AB01Z\n"
    );
}

#[test]
fn zipcrack_brute_cli_recovers_enumerated_password() {
    let path = scratch_file("cli_brute");
    fs::write(&path, make_encrypted_zip("042", "flag{zip_cracked}")).unwrap();
    let output = run_zipcrack(&[
        "zipcrack",
        "brute",
        "--file",
        path.to_str().unwrap(),
        "--charset",
        "0123456789",
        "--min-len",
        "3",
        "--max-len",
        "3",
    ]);
    let _ = fs::remove_file(&path);
    assert!(output.status.success(), "{:?}", output.stderr);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "Found password: 042\n"
    );
}

#[test]
fn zipcrack_random_cli_recovers_single_charset_password() {
    let path = scratch_file("cli_random");
    fs::write(&path, make_encrypted_zip("zzzz", "flag{zip_cracked}")).unwrap();
    let output = run_zipcrack(&[
        "zipcrack",
        "random",
        "--file",
        path.to_str().unwrap(),
        "--length",
        "4",
        "--count",
        "2",
        "--charset",
        "z",
    ]);
    let _ = fs::remove_file(&path);
    assert!(output.status.success(), "{:?}", output.stderr);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "Found password: zzzz\n"
    );
}

#[cfg(unix)]
#[test]
fn read_zipcrack_bytes_with_limit_bounds_device_without_eof() {
    let err = zipcrack::read_zipcrack_bytes_with_limit(std::path::Path::new("/dev/zero"), 64)
        .unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
}

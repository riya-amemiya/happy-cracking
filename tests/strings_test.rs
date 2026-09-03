use happy_cracking::crypto::strings::{self, StringEncoding, StringsAction};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn scratch_file(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("strings_{tag}_{}_{nanos}", std::process::id()))
}

#[test]
fn test_extract_flag_from_noise() {
    let mut data = vec![0x00, 0x01, 0x02, 0xFF, 0xFE];
    data.extend_from_slice(b"flag{strings_are_useful}");
    data.extend_from_slice(&[0x00, 0x80, 0x90]);
    let found = strings::extract_ascii(&data, 4).unwrap();
    assert!(found.iter().any(|s| s == "flag{strings_are_useful}"));
}

#[test]
fn test_min_len_filtering() {
    let mut data = vec![0x00];
    data.extend_from_slice(b"abc");
    data.push(0x00);
    data.extend_from_slice(b"abcd");
    data.push(0x00);
    let found = strings::extract_ascii(&data, 4).unwrap();
    assert!(found.iter().any(|s| s == "abcd"));
    assert!(!found.iter().any(|s| s == "abc"));
}

#[test]
fn test_empty_input() {
    let found = strings::extract_ascii(&[], 4).unwrap();
    assert!(found.is_empty());
}

#[test]
fn test_utf16le_extraction() {
    let data = [0x48, 0x00, 0x65, 0x00, 0x6C, 0x00, 0x6C, 0x00, 0x6F, 0x00];
    let found = strings::extract_utf16le(&data, 4).unwrap();
    assert!(found.iter().any(|s| s == "Hello"));
}

#[test]
fn test_utf16le_flag() {
    let mut data = vec![0xFF, 0xFE];
    for &b in b"flag{utf16}" {
        data.push(b);
        data.push(0x00);
    }
    let found = strings::extract_utf16le(&data, 4).unwrap();
    assert!(found.iter().any(|s| s == "flag{utf16}"));
}

#[test]
fn test_ascii_min_len_zero_errors() {
    assert!(strings::extract_ascii(b"hello", 0).is_err());
}

#[test]
fn test_utf16le_min_len_zero_errors() {
    assert!(strings::extract_utf16le(b"hello", 0).is_err());
}

#[test]
fn read_strings_bytes_with_limit_rejects_oversized_file() {
    let path = scratch_file("oversize");
    fs::write(&path, vec![0x41u8; 32]).unwrap();
    let err = strings::read_strings_bytes_with_limit(&path, 16).unwrap_err();
    let _ = fs::remove_file(&path);
    assert!(err.to_string().contains("Denial of Service"));
}

#[test]
fn read_strings_bytes_with_limit_accepts_file_at_limit() {
    let path = scratch_file("at_limit");
    let data = b"flag{bounded_read}";
    fs::write(&path, data).unwrap();
    let got = strings::read_strings_bytes_with_limit(&path, data.len()).unwrap();
    let _ = fs::remove_file(&path);
    assert_eq!(got, data);
}

#[test]
fn decode_strings_hex_with_limit_rejects_oversized_dump() {
    let err = strings::decode_strings_hex_with_limit("41424344", 2).unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
}

#[test]
fn decode_strings_hex_with_limit_accepts_dump_at_limit() {
    let data = strings::decode_strings_hex_with_limit("4142", 2).unwrap();
    assert_eq!(data, b"AB");
}

#[test]
fn extract_run_reads_file_under_limit() {
    let path = scratch_file("run");
    fs::write(&path, b"xx\x00flag{from_file}\x00").unwrap();
    let action = StringsAction::Extract {
        input: None,
        file: Some(path.clone()),
        min_len: 4,
        encoding: StringEncoding::Ascii,
    };
    strings::run(action).unwrap();
    let _ = fs::remove_file(&path);
}

#[cfg(unix)]
#[test]
fn read_strings_bytes_with_limit_bounds_device_without_eof() {
    let err =
        strings::read_strings_bytes_with_limit(std::path::Path::new("/dev/zero"), 64).unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
}

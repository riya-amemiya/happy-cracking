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
fn extract_ascii_from_hex_matches_decode_then_extract() {
    let mut data = vec![0x00, 0x01, 0x02, 0xFF, 0xFE];
    data.extend_from_slice(b"flag{strings_are_useful}");
    data.extend_from_slice(&[0x00, 0x80, 0x90]);
    let hx = hex::encode(&data);
    let from_hex = strings::extract_ascii_from_hex(&hx, 4).unwrap();
    let decoded = strings::decode_strings_hex_with_limit(&hx, strings::MAX_STRINGS_BYTES).unwrap();
    let from_bytes = strings::extract_ascii(&decoded, 4).unwrap();
    assert_eq!(from_hex, from_bytes);
    assert!(from_hex.iter().any(|s| s == "flag{strings_are_useful}"));
}

#[test]
fn extract_ascii_from_hex_accepts_uppercase_and_empty() {
    let found = strings::extract_ascii_from_hex("48656C6C6F00576F726C64", 4).unwrap();
    assert_eq!(found, vec!["Hello".to_string(), "World".to_string()]);
    assert!(strings::extract_ascii_from_hex("", 4).unwrap().is_empty());
    assert!(
        strings::extract_ascii_from_hex("   ", 4)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn extract_ascii_from_hex_rejects_invalid_and_odd_length() {
    let err = strings::extract_ascii_from_hex("zz", 4).unwrap_err();
    assert!(err.to_string().contains("Failed to decode input as hex"));
    let err = strings::extract_ascii_from_hex("414", 4).unwrap_err();
    assert!(err.to_string().contains("Failed to decode input as hex"));
}

#[test]
fn extract_ascii_from_hex_min_len_zero_validates_hex_first() {
    let err = strings::extract_ascii_from_hex("zz", 0).unwrap_err();
    assert!(err.to_string().contains("Failed to decode input as hex"));
    let err = strings::extract_ascii_from_hex("4142", 0).unwrap_err();
    assert!(err.to_string().contains("min_len must be at least 1"));
}

#[test]
fn extract_ascii_from_hex_with_limit_rejects_oversized_dump() {
    let err = strings::extract_ascii_from_hex_with_limit("41424344", 4, 2).unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
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

#[test]
fn extract_run_reads_hex_under_limit() {
    let action = StringsAction::Extract {
        input: Some("00666c61677b6865785f636c697d00".to_string()),
        file: None,
        min_len: 4,
        encoding: StringEncoding::Ascii,
    };
    strings::run(action).unwrap();
}

#[test]
fn extract_utf16le_from_hex_matches_decode_then_extract() {
    let mut data = vec![0xFF, 0xFE];
    for &b in b"flag{utf16}" {
        data.push(b);
        data.push(0x00);
    }
    data.extend_from_slice(&[0x00, 0x80, 0x90]);
    let hx = hex::encode(&data);
    let from_hex = strings::extract_utf16le_from_hex(&hx, 4).unwrap();
    let decoded = strings::decode_strings_hex_with_limit(&hx, strings::MAX_STRINGS_BYTES).unwrap();
    let from_bytes = strings::extract_utf16le(&decoded, 4).unwrap();
    assert_eq!(from_hex, from_bytes);
    assert!(from_hex.iter().any(|s| s == "flag{utf16}"));
}

#[test]
fn extract_utf16le_from_hex_finds_odd_offset_and_uppercase() {
    let data = [
        0x00, b'H', 0x00, b'e', 0x00, b'l', 0x00, b'l', 0x00, b'o', 0x00,
    ];
    let hx = hex::encode(data).to_ascii_uppercase();
    let found = strings::extract_utf16le_from_hex(&hx, 4).unwrap();
    assert_eq!(found, vec!["Hello".to_string()]);
    assert!(strings::extract_utf16le_from_hex("", 4).unwrap().is_empty());
}

#[test]
fn extract_utf16le_from_hex_rejects_invalid_and_odd_length() {
    let err = strings::extract_utf16le_from_hex("zz", 4).unwrap_err();
    assert!(err.to_string().contains("Failed to decode input as hex"));
    let err = strings::extract_utf16le_from_hex("414", 4).unwrap_err();
    assert!(err.to_string().contains("Failed to decode input as hex"));
}

#[test]
fn extract_utf16le_from_hex_min_len_zero_validates_hex_first() {
    let err = strings::extract_utf16le_from_hex("zz", 0).unwrap_err();
    assert!(err.to_string().contains("Failed to decode input as hex"));
    let err = strings::extract_utf16le_from_hex("48006500", 0).unwrap_err();
    assert!(err.to_string().contains("min_len must be at least 1"));
}

#[test]
fn extract_utf16le_from_hex_with_limit_rejects_oversized_dump() {
    let err = strings::extract_utf16le_from_hex_with_limit("41004200", 4, 2).unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
}

#[test]
fn extract_run_reads_utf16le_hex_under_limit() {
    let action = StringsAction::Extract {
        input: Some("66006c00610067007b00750074006600310036007d00".to_string()),
        file: None,
        min_len: 4,
        encoding: StringEncoding::Utf16le,
    };
    strings::run(action).unwrap();
}

#[cfg(unix)]
#[test]
fn read_strings_bytes_with_limit_bounds_device_without_eof() {
    let err =
        strings::read_strings_bytes_with_limit(std::path::Path::new("/dev/zero"), 64).unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
}

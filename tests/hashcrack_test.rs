use happy_cracking::crypto::hashcrack::{
    HashAlgo, MAX_BRUTE_LEN, SaltPosition, brute_force, compute_hash, find_in_candidates,
    lookup_in_pairs, lookup_in_table_file, lookup_in_table_file_with_limit, parse_table_line,
    read_wordlist_buf_with_limit,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn scratch_wordlist(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("hashcrack_wl_{tag}_{}_{nanos}", std::process::id()))
}

fn candidates() -> Vec<String> {
    ["apple", "banana", "hello", "secret", "password"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

#[test]
fn test_compute_hash_md5() {
    assert_eq!(
        compute_hash(HashAlgo::Md5, "hello"),
        "5d41402abc4b2a76b9719d911017c592"
    );
}

#[test]
fn test_compute_hash_ntlm() {
    assert_eq!(
        compute_hash(HashAlgo::Ntlm, "password"),
        "8846f7eaee8fb117ad06bdd830b7586c"
    );
}

#[test]
fn test_dict_recovers_md5() {
    let target = "5d41402abc4b2a76b9719d911017c592";
    let found = find_in_candidates(
        target,
        HashAlgo::Md5,
        None,
        SaltPosition::Suffix,
        &candidates(),
    );
    assert_eq!(found, Some("hello".to_string()));
}

#[test]
fn test_dict_recovers_ntlm() {
    let target = compute_hash(HashAlgo::Ntlm, "password");
    let found = find_in_candidates(
        &target,
        HashAlgo::Ntlm,
        None,
        SaltPosition::Suffix,
        &candidates(),
    );
    assert_eq!(found, Some("password".to_string()));
}

#[test]
fn test_dict_case_insensitive_target() {
    let target = "5D41402ABC4B2A76B9719D911017C592";
    let found = find_in_candidates(
        target,
        HashAlgo::Md5,
        None,
        SaltPosition::Suffix,
        &candidates(),
    );
    assert_eq!(found, Some("hello".to_string()));
}

#[test]
fn test_dict_with_salt_prefix() {
    let target = compute_hash(HashAlgo::Sha256, "s4ltyhello");
    let found = find_in_candidates(
        &target,
        HashAlgo::Sha256,
        Some("s4lty"),
        SaltPosition::Prefix,
        &candidates(),
    );
    assert_eq!(found, Some("hello".to_string()));
}

#[test]
fn test_dict_accepts_str_slices() {
    let target = "5d41402abc4b2a76b9719d911017c592";
    let pool = ["apple", "hello", "secret"];
    let found = find_in_candidates(target, HashAlgo::Md5, None, SaltPosition::Suffix, &pool);
    assert_eq!(found, Some("hello".to_string()));
}

#[test]
fn test_dict_cli_wordlist_skips_blank_and_strips_crlf() {
    use std::io::Write;
    use std::process::Command;

    let mut path = std::env::temp_dir();
    path.push(format!("hashcrack_wl_{}.txt", std::process::id()));
    {
        let mut file = std::fs::File::create(&path).unwrap();
        write!(file, "nope\r\n\nhello\r\n").unwrap();
    }
    let out = Command::new(env!("CARGO_BIN_EXE_happy-cracking"))
        .args([
            "hashcrack",
            "dict",
            "5d41402abc4b2a76b9719d911017c592",
            "-w",
        ])
        .arg(&path)
        .args(["--algo", "md5"])
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&path);
    assert!(out.status.success(), "stderr {:?}", out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Found: hello"),
        "unexpected stdout: {stdout}"
    );
}

#[test]
fn test_dict_not_found_returns_none() {
    let target = compute_hash(HashAlgo::Sha256, "not-in-the-list");
    let found = find_in_candidates(
        &target,
        HashAlgo::Sha256,
        None,
        SaltPosition::Suffix,
        &candidates(),
    );
    assert_eq!(found, None);
}

#[test]
fn test_dict_invalid_hex_target_returns_none() {
    // Byte-compare path hex-decodes the target once; garbage hex cannot match.
    let found = find_in_candidates(
        "not-a-hex-digest!!!!",
        HashAlgo::Md5,
        None,
        SaltPosition::Suffix,
        &candidates(),
    );
    assert_eq!(found, None);
}

#[test]
fn test_ctf_flag_sha256() {
    let flag = "flag{cr4ck3d}";
    let target = compute_hash(HashAlgo::Sha256, flag);
    let pool = vec![
        "wrong".to_string(),
        "flag{cr4ck3d}".to_string(),
        "another".to_string(),
    ];
    let found = find_in_candidates(&target, HashAlgo::Sha256, None, SaltPosition::Suffix, &pool);
    assert_eq!(found, Some(flag.to_string()));
}

#[test]
fn test_brute_force_recovers_short_word() {
    let target = compute_hash(HashAlgo::Md5, "cat");
    let found = brute_force(
        &target,
        HashAlgo::Md5,
        "abcdefghijklmnopqrstuvwxyz",
        1,
        3,
        None,
        SaltPosition::Suffix,
    )
    .unwrap();
    assert_eq!(found, Some("cat".to_string()));
}

#[test]
fn test_brute_force_with_salt() {
    let target = compute_hash(HashAlgo::Sha1, "abXX");
    let found = brute_force(
        &target,
        HashAlgo::Sha1,
        "ab",
        2,
        2,
        Some("XX"),
        SaltPosition::Suffix,
    )
    .unwrap();
    assert_eq!(found, Some("ab".to_string()));
}

#[test]
fn test_brute_force_not_found() {
    let target = compute_hash(HashAlgo::Md5, "zzzzz");
    let found = brute_force(
        &target,
        HashAlgo::Md5,
        "ab",
        1,
        2,
        None,
        SaltPosition::Suffix,
    )
    .unwrap();
    assert_eq!(found, None);
}

#[test]
fn test_brute_force_first_and_last_index() {
    // Stack path must emit MSD-first like the old Vec<char> collect (index 0 = "aaa").
    let target = compute_hash(HashAlgo::Md5, "aaa");
    let found = brute_force(
        &target,
        HashAlgo::Md5,
        "abc",
        3,
        3,
        None,
        SaltPosition::Suffix,
    )
    .unwrap();
    assert_eq!(found, Some("aaa".to_string()));

    let target = compute_hash(HashAlgo::Md5, "ccc");
    let found = brute_force(
        &target,
        HashAlgo::Md5,
        "abc",
        3,
        3,
        None,
        SaltPosition::Suffix,
    )
    .unwrap();
    assert_eq!(found, Some("ccc".to_string()));
}

#[test]
fn test_brute_force_unicode_charset() {
    // Non-ASCII charset skips the stack buffer and still enumerates correctly.
    let target = compute_hash(HashAlgo::Sha256, "βα");
    let found = brute_force(
        &target,
        HashAlgo::Sha256,
        "αβ",
        2,
        2,
        None,
        SaltPosition::Suffix,
    )
    .unwrap();
    assert_eq!(found, Some("βα".to_string()));
}

#[test]
fn test_brute_force_oversized_space_errors() {
    let charset =
        "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!@#$%^&*()-_=+[]{};:,.<>?";
    let result = brute_force(
        "0000000000000000000000000000000000000000",
        HashAlgo::Sha1,
        charset,
        1,
        6,
        None,
        SaltPosition::Suffix,
    );
    assert!(result.is_err());
}

#[test]
fn test_brute_force_rejects_max_len_above_cap() {
    let err = brute_force(
        "5d41402abc4b2a76b9719d911017c592",
        HashAlgo::Md5,
        "a",
        1,
        MAX_BRUTE_LEN + 1,
        None,
        SaltPosition::Suffix,
    )
    .unwrap_err();
    assert!(err.to_string().contains(&MAX_BRUTE_LEN.to_string()));
}

#[test]
fn test_brute_force_accepts_max_len_at_cap() {
    let target = compute_hash(HashAlgo::Md5, "a");
    let found = brute_force(
        &target,
        HashAlgo::Md5,
        "a",
        1,
        MAX_BRUTE_LEN,
        None,
        SaltPosition::Suffix,
    )
    .unwrap();
    assert_eq!(found, Some("a".to_string()));
}

#[cfg(target_pointer_width = "64")]
#[test]
fn test_brute_force_rejects_len_beyond_u32() {
    let len = 1usize << 32;
    let err = brute_force(
        "5d41402abc4b2a76b9719d911017c592",
        HashAlgo::Md5,
        "ab",
        len,
        len,
        None,
        SaltPosition::Suffix,
    )
    .unwrap_err();
    assert!(err.to_string().contains(&MAX_BRUTE_LEN.to_string()));
}

#[test]
fn test_brute_force_empty_charset_errors() {
    let result = brute_force(
        "5d41402abc4b2a76b9719d911017c592",
        HashAlgo::Md5,
        "",
        1,
        2,
        None,
        SaltPosition::Suffix,
    );
    assert!(result.is_err());
}

#[test]
fn test_brute_force_invalid_hex_still_validates_charset() {
    let result = brute_force(
        "not-hex!!!",
        HashAlgo::Md5,
        "",
        1,
        3,
        None,
        SaltPosition::Suffix,
    );
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Charset must not be empty")
    );
}

#[test]
fn test_brute_force_invalid_hex_still_validates_search_space() {
    let charset =
        "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!@#$%^&*()-_=+[]{};:,.<>?";
    let result = brute_force(
        "not-hex!!!",
        HashAlgo::Sha1,
        charset,
        1,
        6,
        None,
        SaltPosition::Suffix,
    );
    assert!(result.is_err());
}

#[test]
fn test_lookup_in_pairs_found() {
    let pairs = vec![
        ("5d41402abc4b2a76b9719d911017c592", "hello"),
        ("e10adc3949ba59abbe56e057f20f883e", "123456"),
    ];
    let found = lookup_in_pairs("5d41402abc4b2a76b9719d911017c592", pairs);
    assert_eq!(found, Some("hello".to_string()));
}

#[test]
fn test_lookup_in_pairs_case_insensitive() {
    let pairs = vec![("5D41402ABC4B2A76B9719D911017C592", "hello")];
    let found = lookup_in_pairs("5d41402abc4b2a76b9719d911017c592", pairs);
    assert_eq!(found, Some("hello".to_string()));
}

#[test]
fn test_lookup_in_pairs_not_found() {
    let pairs = vec![("aaaa", "x"), ("bbbb", "y")];
    let found = lookup_in_pairs("cccc", pairs);
    assert_eq!(found, None);
}

#[test]
fn test_parse_table_line_colon_and_whitespace() {
    assert_eq!(
        parse_table_line("5d41402abc4b2a76b9719d911017c592:hello"),
        Some((
            "5d41402abc4b2a76b9719d911017c592".to_string(),
            "hello".to_string()
        ))
    );
    assert_eq!(
        parse_table_line("5d41402abc4b2a76b9719d911017c592   hello"),
        Some((
            "5d41402abc4b2a76b9719d911017c592".to_string(),
            "hello".to_string()
        ))
    );
    assert_eq!(parse_table_line("   "), None);
}

#[test]
fn test_lookup_table_file_roundtrip() {
    use std::io::Write;
    let mut path = std::env::temp_dir();
    path.push(format!("hashcrack_table_{}.txt", std::process::id()));
    {
        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(file, "aaaa:nope").unwrap();
        writeln!(file, "5d41402abc4b2a76b9719d911017c592 hello").unwrap();
    }
    let target = "5d41402abc4b2a76b9719d911017c592";
    let found = lookup_in_table_file(target, &path).unwrap();
    let miss = lookup_in_table_file("ffffffffffffffffffffffffffffffff", &path).unwrap();
    let mixed_case = lookup_in_table_file("5D41402ABC4B2A76B9719D911017C592", &path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(found, Some("hello".to_string()));
    assert_eq!(miss, None);
    assert_eq!(mixed_case, Some("hello".to_string()));
}

#[test]
fn lookup_in_table_file_with_limit_rejects_oversized_file() {
    let path = scratch_wordlist("table_oversize");
    fs::write(&path, vec![b'a'; 32]).unwrap();
    let err = lookup_in_table_file_with_limit("deadbeef", &path, 16).unwrap_err();
    let _ = fs::remove_file(&path);
    assert!(err.to_string().contains("Denial of Service"));
}

#[test]
fn lookup_in_table_file_with_limit_accepts_file_at_limit() {
    let path = scratch_wordlist("table_at_limit");
    let data = "5d41402abc4b2a76b9719d911017c592 hello";
    fs::write(&path, data).unwrap();
    let found =
        lookup_in_table_file_with_limit("5d41402abc4b2a76b9719d911017c592", &path, data.len())
            .unwrap();
    let _ = fs::remove_file(&path);
    assert_eq!(found, Some("hello".to_string()));
}

#[cfg(unix)]
#[test]
fn lookup_in_table_file_with_limit_bounds_device_without_eof() {
    let err = lookup_in_table_file_with_limit("deadbeef", std::path::Path::new("/dev/zero"), 64)
        .unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
}

#[test]
fn read_wordlist_buf_with_limit_rejects_oversized_file() {
    let path = scratch_wordlist("oversize");
    fs::write(&path, vec![b'a'; 32]).unwrap();
    let err = read_wordlist_buf_with_limit(&path, 16).unwrap_err();
    let _ = fs::remove_file(&path);
    assert!(err.to_string().contains("Denial of Service"));
}

#[test]
fn read_wordlist_buf_with_limit_accepts_file_at_limit() {
    let path = scratch_wordlist("at_limit");
    fs::write(&path, "hello\nsecret\n").unwrap();
    let got = read_wordlist_buf_with_limit(&path, 13).unwrap();
    let _ = fs::remove_file(&path);
    assert_eq!(got, "hello\nsecret\n");
}

#[cfg(unix)]
#[test]
fn read_wordlist_buf_with_limit_bounds_device_without_eof() {
    let err = read_wordlist_buf_with_limit(std::path::Path::new("/dev/zero"), 64).unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
}

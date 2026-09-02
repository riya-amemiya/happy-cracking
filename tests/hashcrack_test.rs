use happy_cracking::crypto::hashcrack::{
    self, HashAlgo, SaltPosition, brute_force, compute_hash, find_in_candidates, lookup_in_pairs,
    parse_table_line,
};

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

    let content = std::fs::read_to_string(&path).unwrap();
    let pairs: Vec<(String, String)> = content
        .lines()
        .filter_map(hashcrack::parse_table_line)
        .collect();
    let found = lookup_in_pairs(target, pairs);
    std::fs::remove_file(&path).unwrap();
    assert_eq!(found, Some("hello".to_string()));
}

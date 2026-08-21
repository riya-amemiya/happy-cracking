use happy_cracking::crypto::hashcrack::{
    HashAlgo, apply_rule, builtin_rules, compute_hash, expand_mask, hybrid_attack, mask_attack,
    rule_attack,
};

#[test]
fn apply_rule_identity_lower_append() {
    assert_eq!(apply_rule("Password", ":"), "Password");
    assert_eq!(apply_rule("Password", "l"), "password");
    assert_eq!(apply_rule("Password", "u"), "PASSWORD");
    assert_eq!(apply_rule("password", "c"), "Password");
    assert_eq!(apply_rule("ab", "r"), "ba");
    assert_eq!(apply_rule("ab", "d"), "abab");
    assert_eq!(apply_rule("pass", "$1"), "pass1");
    assert_eq!(apply_rule("pass", "^!"), "!pass");
    assert_eq!(apply_rule("admin", "c$!"), "Admin!");
}

#[test]
fn rule_attack_finds_capitalized_word() {
    let plain = "Admin";
    let target = compute_hash(HashAlgo::Md5, plain);
    let words = vec!["admin".to_string()];
    let rules = vec!["c".to_string()];
    let found = rule_attack(&target, HashAlgo::Md5, &words, &rules);
    assert_eq!(found.as_deref(), Some("Admin"));
}

#[test]
fn builtin_rules_include_identity() {
    assert!(builtin_rules().contains(&":"));
}

#[test]
fn expand_mask_literals_and_classes() {
    let positions = expand_mask("a?d").unwrap();
    assert_eq!(positions.len(), 2);
    assert_eq!(positions[0], vec!['a']);
    assert_eq!(positions[1].len(), 10);
}

#[test]
fn mask_attack_recovers_short_password() {
    let plain = "ab1";
    let target = compute_hash(HashAlgo::Md5, plain);
    let found = mask_attack(&target, HashAlgo::Md5, "?l?l?d").unwrap();
    assert_eq!(found.as_deref(), Some("ab1"));
}

#[test]
fn mask_attack_literal_prefix() {
    let plain = "ab1";
    let target = compute_hash(HashAlgo::Md5, plain);
    let found = mask_attack(&target, HashAlgo::Md5, "ab?d").unwrap();
    assert_eq!(found.as_deref(), Some("ab1"));
}

#[test]
fn mask_attack_unicode_literal() {
    let plain = "café";
    let target = compute_hash(HashAlgo::Sha256, plain);
    let found = mask_attack(&target, HashAlgo::Sha256, "café").unwrap();
    assert_eq!(found.as_deref(), Some("café"));
}

#[test]
fn mask_attack_miss_returns_none() {
    let target = "0".repeat(32);
    let found = mask_attack(&target, HashAlgo::Md5, "?l?d").unwrap();
    assert!(found.is_none());
}

#[test]
fn hybrid_attack_numeric_suffix() {
    let plain = "pass12";
    let target = compute_hash(HashAlgo::Md5, plain);
    let words = vec!["pass".to_string(), "admin".to_string()];
    let found = hybrid_attack(&target, HashAlgo::Md5, &words, 2, 2, false).unwrap();
    assert_eq!(found.as_deref(), Some("pass12"));
}

#[test]
fn hybrid_attack_zero_padded_suffix() {
    let plain = "pass007";
    let target = compute_hash(HashAlgo::Sha256, plain);
    let words = vec!["pass".to_string()];
    let found = hybrid_attack(&target, HashAlgo::Sha256, &words, 3, 3, false).unwrap();
    assert_eq!(found.as_deref(), Some("pass007"));
}

#[test]
fn hybrid_attack_numeric_prefix() {
    let plain = "12admin";
    let target = compute_hash(HashAlgo::Md5, plain);
    let words = vec!["admin".to_string()];
    let found = hybrid_attack(&target, HashAlgo::Md5, &words, 2, 2, true).unwrap();
    assert_eq!(found.as_deref(), Some("12admin"));
}

#[test]
fn hybrid_attack_bare_word_when_min_digits_zero() {
    let plain = "admin";
    let target = compute_hash(HashAlgo::Md5, plain);
    let words = vec!["admin".to_string()];
    let found = hybrid_attack(&target, HashAlgo::Md5, &words, 0, 0, false).unwrap();
    assert_eq!(found.as_deref(), Some("admin"));
}

#[test]
fn mask_rejects_unknown_class() {
    assert!(expand_mask("?z").is_err());
}

#[test]
fn mask_attack_invalid_hex_still_validates_mask() {
    let err = mask_attack("not-hex!!!", HashAlgo::Md5, "?z").unwrap_err();
    assert!(err.to_string().contains("Unknown mask class"));
}

#[test]
fn hybrid_attack_invalid_hex_still_validates_digit_range() {
    let words = vec!["pass".to_string()];
    let err = hybrid_attack("not-hex!!!", HashAlgo::Md5, &words, 0, 7, false).unwrap_err();
    assert!(err.to_string().contains("--max-digits must be <= 6"));
}

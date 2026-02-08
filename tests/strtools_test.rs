use happy_cracking::crypto::strtools;

#[test]
fn test_reverse_basic() {
    assert_eq!(strtools::reverse("Hello"), "olleH");
}

#[test]
fn test_reverse_empty() {
    assert_eq!(strtools::reverse(""), "");
}

#[test]
fn test_reverse_palindrome() {
    assert_eq!(strtools::reverse("racecar"), "racecar");
}

#[test]
fn test_reverse_ctf_flag() {
    assert_eq!(strtools::reverse("}galf{ftc"), "ctf{flag}");
}

#[test]
fn test_ord_basic() {
    assert_eq!(strtools::ord("ABC"), "A=65 B=66 C=67");
}

#[test]
fn test_ord_empty() {
    assert_eq!(strtools::ord(""), "");
}

#[test]
fn test_ord_single() {
    assert_eq!(strtools::ord("A"), "A=65");
}

#[test]
fn test_ord_with_space() {
    assert_eq!(strtools::ord("A B"), "A=65  =32 B=66");
}

#[test]
fn test_chr_basic() {
    assert_eq!(strtools::chr("72 101 108 108 111").unwrap(), "Hello");
}

#[test]
fn test_chr_empty() {
    assert_eq!(strtools::chr("").unwrap(), "");
}

#[test]
fn test_chr_single() {
    assert_eq!(strtools::chr("65").unwrap(), "A");
}

#[test]
fn test_chr_invalid_number() {
    assert!(strtools::chr("abc").is_err());
}

#[test]
fn test_chr_invalid_codepoint() {
    // 0xD800 is an invalid Unicode scalar value (surrogate)
    assert!(strtools::chr("55296").is_err());
}

#[test]
fn test_roundtrip_ord_chr() {
    let original = "flag{test}";
    let ord_result = strtools::ord(original);
    let numbers: String = ord_result
        .split(' ')
        .filter_map(|s| s.split('=').nth(1))
        .collect::<Vec<_>>()
        .join(" ");
    assert_eq!(strtools::chr(&numbers).unwrap(), original);
}

use happy_cracking::crypto::atbash;

#[test]
fn test_transform_uppercase() {
    assert_eq!(atbash::transform("ABC"), "ZYX");
    assert_eq!(atbash::transform("XYZ"), "CBA");
}

#[test]
fn test_transform_lowercase() {
    assert_eq!(atbash::transform("abc"), "zyx");
}

#[test]
fn test_transform_mixed() {
    assert_eq!(atbash::transform("Hello"), "Svool");
}

#[test]
fn test_preserve_non_alpha() {
    assert_eq!(atbash::transform("Hello, World!"), "Svool, Dliow!");
}

#[test]
fn test_self_inverse() {
    let original = "Hello World";
    let transformed = atbash::transform(original);
    let back = atbash::transform(&transformed);
    assert_eq!(back, original);
}

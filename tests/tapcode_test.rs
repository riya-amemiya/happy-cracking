use happy_cracking::crypto::tapcode;

#[test]
fn test_encode_basic() {
    // A=11 -> ". ."
    assert_eq!(tapcode::encode("A"), ". .");
}

#[test]
fn test_encode_hello() {
    // H=23 -> ".. ...", E=15 -> ". .....", L=31 -> "... .", L=31, O=34 -> "... ...."
    assert_eq!(
        tapcode::encode("HELLO"),
        ".. ...   . .....   ... .   ... .   ... ...."
    );
}

#[test]
fn test_decode_basic() {
    assert_eq!(tapcode::decode(". .").unwrap(), "A");
}

#[test]
fn test_decode_hello() {
    assert_eq!(
        tapcode::decode(".. ...   . .....   ... .   ... .   ... ....").unwrap(),
        "HELLO"
    );
}

#[test]
fn test_encode_k_maps_to_c() {
    // K maps to C, C=13 -> ". ..."
    let enc_c = tapcode::encode("C");
    let enc_k = tapcode::encode("K");
    assert_eq!(enc_c, enc_k);
    assert_eq!(enc_c, ". ...");
}

#[test]
fn test_encode_with_space() {
    assert_eq!(
        tapcode::encode("HI THERE"),
        ".. ...   .. .... / .... ....   .. ...   . .....   .... ..   . ....."
    );
}

#[test]
fn test_decode_with_space() {
    assert_eq!(
        tapcode::decode(".. ...   .. .... / .... ....   .. ...   . .....   .... ..   . .....")
            .unwrap(),
        "HI THERE"
    );
}

#[test]
fn test_roundtrip() {
    let original = "HELLO WORLD";
    let encoded = tapcode::encode(original);
    let decoded = tapcode::decode(&encoded).unwrap();
    // K->C substitution doesn't affect this text
    assert_eq!(decoded, original);
}

#[test]
fn test_encode_z() {
    // Z=55 -> "..... ....."
    assert_eq!(tapcode::encode("Z"), "..... .....");
}

#[test]
fn test_encode_empty() {
    assert_eq!(tapcode::encode(""), "");
}

#[test]
fn test_decode_empty() {
    assert_eq!(tapcode::decode("").unwrap(), "");
}

#[test]
fn test_ctf_flag() {
    let encoded = tapcode::encode("FLAG");
    let decoded = tapcode::decode(&encoded).unwrap();
    assert_eq!(decoded, "FLAG");
}

#[test]
fn test_decode_invalid() {
    assert!(tapcode::decode("...... .").is_err());
}

#[test]
fn test_encode_lowercase() {
    assert_eq!(tapcode::encode("abc"), ". .   . ..   . ...");
}

#[test]
fn test_full_alphabet_roundtrip() {
    // K maps to C, so exclude K for exact roundtrip
    let alpha = "ABCDEFGHIJLMNOPQRSTUVWXYZ";
    let encoded = tapcode::encode(alpha);
    let decoded = tapcode::decode(&encoded).unwrap();
    assert_eq!(decoded, alpha);
}

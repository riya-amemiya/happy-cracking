use happy_cracking::crypto::xor::{
    self, XorAction, best_single_byte_key, crack_repeating_key, crib_drag, english_score, xor_bytes,
};

#[test]
fn english_score_prefers_plaintext() {
    let good = english_score(b"The quick brown fox jumps over the lazy dog");
    let bad = english_score(b"\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a");
    assert!(good > bad);
}

#[test]
fn best_single_byte_recovers_key() {
    let plain = b"Cooking MC's like a pound of bacon";
    let key = 0x58u8;
    let ct: Vec<u8> = plain.iter().map(|&b| b ^ key).collect();
    let (recovered, _, out) = best_single_byte_key(&ct);
    assert_eq!(recovered, key);
    assert_eq!(out, plain);
}

#[test]
fn crack_repeating_key_recovers_ice() {
    // Classic cryptopals-style repeating key "ICE"
    let plain = b"Burning 'em, if you ain't quick and nimble\nI go crazy when I hear a cymbal";
    let key = b"ICE";
    let ct = xor_bytes(plain, key);
    let cands = crack_repeating_key(&ct, 10, 5, Some(3));
    assert!(!cands.is_empty());
    assert_eq!(cands[0].key, key);
    assert_eq!(cands[0].plaintext, plain);
}

#[test]
fn crack_auto_length_finds_key() {
    let plain = b"This is a reasonably long english sentence used for xor key recovery testing purposes today.";
    let key = b"KEY!";
    let ct = xor_bytes(plain, key);
    let cands = crack_repeating_key(&ct, 12, 5, None);
    assert!(
        cands.iter().any(|c| c.key == key || c.plaintext == plain),
        "expected key recovery, top keys: {:?}",
        cands
            .iter()
            .take(3)
            .map(|c| String::from_utf8_lossy(&c.key).into_owned())
            .collect::<Vec<_>>()
    );
}

#[test]
fn crack_run_rejects_excessive_key_length() {
    let input = hex::encode(b"HELLO WORLD HELLO WORLD HELLO WORLD HELLO WORLD");
    let err = xor::run(XorAction::Crack {
        input,
        max_len: 40,
        top: 3,
        key_length: Some(xor::MAX_KEY_LENGTH + 1),
    })
    .unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
}

#[test]
fn crack_repeating_key_caps_excessive_max_len() {
    let plain = b"Burning 'em, if you ain't quick and nimble\nI go crazy when I hear a cymbal";
    let key = b"ICE";
    let ct = xor_bytes(plain, key);
    let cands = crack_repeating_key(&ct, usize::MAX, 5, Some(3));
    assert!(!cands.is_empty());
    assert_eq!(cands[0].key, key);
}

#[test]
fn crib_drag_finds_flag_offset() {
    let plain = b"prefix_flag{secret}_suffix";
    let key = b"AB";
    let ct = xor_bytes(plain, key);
    let hits = crib_drag(&ct, b"flag{");
    assert!(
        hits.iter().any(|h| h.offset == 7),
        "expected crib at offset 7, hits: {:?}",
        hits.iter().map(|h| h.offset).collect::<Vec<_>>()
    );
}

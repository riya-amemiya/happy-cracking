use happy_cracking::crypto::cipherid;

fn confidence_of(candidates: &[cipherid::Candidate], needle: &str) -> Option<f64> {
    candidates
        .iter()
        .find(|c| c.name.to_lowercase().contains(&needle.to_lowercase()))
        .map(|c| c.confidence)
}

fn top_name(candidates: &[cipherid::Candidate]) -> String {
    candidates
        .first()
        .map(|c| c.name.clone())
        .unwrap_or_default()
}

#[test]
fn test_detects_base64() {
    let candidates = cipherid::analyze("SGVsbG8gV29ybGQ=");
    assert!(top_name(&candidates).contains("Base64"));
    assert!(confidence_of(&candidates, "Base64").unwrap() >= 0.7);
}

#[test]
fn test_detects_hex() {
    let candidates = cipherid::analyze("48656c6c6f");
    assert!(top_name(&candidates).contains("Hex"));
    assert!(confidence_of(&candidates, "Hex").unwrap() >= 0.7);
}

#[test]
fn test_detects_all_numeric_a1z26() {
    let candidates = cipherid::analyze("8-5-12-12-15");
    assert!(top_name(&candidates).contains("A1Z26"));
    assert!(confidence_of(&candidates, "A1Z26").unwrap() >= 0.7);
}

#[test]
fn test_detects_morse() {
    let candidates = cipherid::analyze(".... . .-.. .-.. ---");
    assert!(top_name(&candidates).contains("Morse"));
    assert!(confidence_of(&candidates, "Morse").unwrap() >= 0.7);
}

#[test]
fn test_monoalphabetic_via_ioc() {
    let mono = "LWZDVWKHEHVWRIWLPHVLWZDVWKHZRUVWRIWLPHVLWZDVWKHDJHRIZLVGRPLWZDVWKHDJHRIIRROLVKQHVV";
    let candidates = cipherid::analyze(mono);
    let mono_conf = confidence_of(&candidates, "Monoalphabetic");
    let poly_conf = confidence_of(&candidates, "Polyalphabetic");
    assert!(mono_conf.is_some(), "expected a monoalphabetic candidate");

    assert!(poly_conf.is_none() || mono_conf.unwrap() >= poly_conf.unwrap());
}

#[test]
fn test_polyalphabetic_via_ioc() {
    let vig = "AXYRWMZIDVWMGJVZQXKMVNELLLGNSKKXQWXBEIUZXPSWVYITYIQWABKHQDMMOEUKLXSKGFJYGSNZWAFIUJMMOEUKLXWTQTLHXFGCMXXMVNELLLGVTHULQWMGUVGUYEAXAZXPSWVYILWEUFRHXPKXLMAXYRWMZIUVELGRQWHTJOPVWL";
    let candidates = cipherid::analyze(vig);
    let poly_conf = confidence_of(&candidates, "Polyalphabetic");
    let mono_conf = confidence_of(&candidates, "Monoalphabetic");
    assert!(poly_conf.is_some(), "expected a polyalphabetic candidate");

    assert!(mono_conf.is_none() || poly_conf.unwrap() >= mono_conf.unwrap());
}

#[test]
fn test_english_plaintext() {
    let candidates =
        cipherid::analyze("It was the best of times it was the worst of times it was the age");
    assert!(
        confidence_of(&candidates, "English plaintext").is_some(),
        "expected an English plaintext candidate"
    );
}

#[test]
fn test_empty_input() {
    let candidates = cipherid::analyze("");
    assert!(candidates.is_empty());
}

#[test]
fn test_whitespace_only_input() {
    let candidates = cipherid::analyze("   \t\n  ");
    assert!(candidates.is_empty());
}

#[test]
fn test_unicode_input_does_not_panic() {
    let candidates = cipherid::analyze("こんにちは🌸✨ flag{未来}");

    for w in candidates.windows(2) {
        assert!(w[0].confidence >= w[1].confidence);
    }
}

#[test]
fn test_flag_marker() {
    let candidates = cipherid::analyze("flag{this_is_a_ctf_flag}");
    assert!(
        candidates
            .iter()
            .any(|c| c.reason.contains("flag") && c.confidence >= 0.9)
    );
}

#[test]
fn test_results_sorted_descending() {
    let candidates = cipherid::analyze("48656c6c6f");
    for w in candidates.windows(2) {
        assert!(w[0].confidence >= w[1].confidence);
    }
}

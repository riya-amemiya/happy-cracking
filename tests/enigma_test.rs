use happy_cracking::crypto::enigma::{
    self, EnigmaAction, MAX_CRACK_HITS, MAX_CRACK_LETTERS, crack, decrypt, encrypt, transform,
};

fn crypt(input: &str) -> String {
    transform(input, "I II III", "B", "AAA", "AAA", "").unwrap()
}

#[test]
fn wikipedia_aaaaa_rings_a() {
    assert_eq!(crypt("AAAAA"), "BDZGO");
}

#[test]
fn wikipedia_aaaaa_rings_b() {
    assert_eq!(
        transform("AAAAA", "I II III", "B", "BBB", "AAA", "").unwrap(),
        "EWTYX"
    );
}

#[test]
fn encrypt_equals_decrypt() {
    let c = encrypt("HELLO", "I II III", "B", "AAA", "AAA", "").unwrap();
    let p = decrypt(&c, "I II III", "B", "AAA", "AAA", "").unwrap();
    assert_eq!(p, "HELLO");
}

#[test]
fn reciprocal_single_letter() {
    assert_eq!(crypt("A"), "B");
    assert_eq!(
        transform("B", "I II III", "B", "AAA", "AAA", "").unwrap(),
        "A"
    );
}

#[test]
fn preserves_non_letters_and_case() {
    assert_eq!(crypt("Aaa aa!"), "Bdz go!");
}

#[test]
fn empty_input() {
    assert_eq!(crypt(""), "");
}

#[test]
fn numeric_rings_and_positions() {
    assert_eq!(
        transform("AAAAA", "I II III", "B", "1 1 1", "1 1 1", "").unwrap(),
        "BDZGO"
    );
}

#[test]
fn compact_rotor_and_setting_forms() {
    assert_eq!(
        transform("AAAAA", "1,2,3", "B", "AAA", "AAA", "").unwrap(),
        "BDZGO"
    );
}

#[test]
fn plugboard_swaps_before_and_after() {
    let with = transform("AAAAA", "I II III", "B", "AAA", "AAA", "AB").unwrap();
    assert_ne!(with, "BDZGO");
    let back = transform(&with, "I II III", "B", "AAA", "AAA", "AB").unwrap();
    assert_eq!(back, "AAAAA");
}

#[test]
fn plugboard_compact_pairs() {
    let spaced = transform("FLAG", "III II I", "B", "AAA", "XYZ", "AB CD").unwrap();
    let packed = transform("FLAG", "III II I", "B", "AAA", "XYZ", "ABCD").unwrap();
    assert_eq!(spaced, packed);
}

#[test]
fn ctf_flag_roundtrip() {
    let settings = ("II IV I", "C", "A M Z", "QWE", "AT BS");
    let plain = "flag{ENIGMAWORKS}";
    let cipher = transform(
        plain, settings.0, settings.1, settings.2, settings.3, settings.4,
    )
    .unwrap();
    assert_ne!(cipher, plain);
    assert_eq!(
        transform(
            &cipher, settings.0, settings.1, settings.2, settings.3, settings.4,
        )
        .unwrap(),
        plain
    );
}

#[test]
fn right_notch_steps_middle() {
    let stream = transform("AA", "I II III", "B", "AAA", "AAU", "").unwrap();
    let from_notch = transform("A", "I II III", "B", "AAA", "AAV", "").unwrap();
    assert_eq!(stream.as_bytes()[1], from_notch.as_bytes()[0]);
}

#[test]
fn middle_notch_double_steps_left() {
    let stream = transform("AA", "I II III", "B", "AAA", "ADV", "").unwrap();
    let from_mid_notch = transform("A", "I II III", "B", "AAA", "AEW", "").unwrap();
    assert_eq!(stream.as_bytes()[1], from_mid_notch.as_bytes()[0]);
    assert_ne!(
        from_mid_notch,
        transform("A", "I II III", "B", "AAA", "AEX", "").unwrap()
    );
}

#[test]
fn rotor_vi_has_two_turnovers() {
    let at_z = transform("A", "I II VI", "B", "AAA", "AAZ", "").unwrap();
    let at_m = transform("A", "I II VI", "B", "AAA", "AAM", "").unwrap();
    assert_eq!(
        transform("AA", "I II VI", "B", "AAA", "AAY", "")
            .unwrap()
            .as_bytes()[1],
        at_z.as_bytes()[0]
    );
    assert_eq!(
        transform("AA", "I II VI", "B", "AAA", "AAL", "")
            .unwrap()
            .as_bytes()[1],
        at_m.as_bytes()[0]
    );
}

#[test]
fn duplicate_rotors_error() {
    assert!(transform("A", "I I II", "B", "AAA", "AAA", "").is_err());
}

#[test]
fn unknown_rotor_error() {
    assert!(transform("A", "I II IX", "B", "AAA", "AAA", "").is_err());
}

#[test]
fn unknown_reflector_error() {
    assert!(transform("A", "I II III", "D", "AAA", "AAA", "").is_err());
}

#[test]
fn bad_plugboard_errors() {
    assert!(transform("A", "I II III", "B", "AAA", "AAA", "AA").is_err());
    assert!(transform("A", "I II III", "B", "AAA", "AAA", "AB AC").is_err());
    assert!(transform("A", "I II III", "B", "AAA", "AAA", "A").is_err());
}

#[test]
fn two_rotor_list_error() {
    assert!(transform("A", "I II", "B", "AAA", "AAA", "").is_err());
}

#[test]
fn ring_out_of_range_error() {
    assert!(transform("A", "I II III", "B", "0 1 1", "AAA", "").is_err());
    assert!(transform("A", "I II III", "B", "27 1 1", "AAA", "").is_err());
}

#[test]
fn m4_beta_thin_b_at_a_matches_m3_b() {
    let m3 = transform("HELLOWORLD", "I II III", "B", "AAA", "QWE", "AB CD").unwrap();
    let m4 = transform(
        "HELLOWORLD",
        "BETA I II III",
        "B-THIN",
        "AAAA",
        "AQWE",
        "AB CD",
    )
    .unwrap();
    assert_eq!(m3, m4);
}

#[test]
fn m4_three_letter_settings_pad_greek_a() {
    let full = transform("SECRET", "BETA II IV I", "B-THIN", "AAAA", "ABCD", "").unwrap();
    let short = transform("SECRET", "BETA II IV I", "B", "AAA", "BCD", "").unwrap();
    assert_eq!(full, short);
}

#[test]
fn m4_roundtrip_and_gamma() {
    let c = transform("FLAGCTF", "GAMMA V III I", "C-THIN", "BCDF", "WXYZ", "AT").unwrap();
    assert_eq!(
        transform(&c, "GAMMA V III I", "C-THIN", "BCDF", "WXYZ", "AT").unwrap(),
        "FLAGCTF"
    );
}

#[test]
fn m4_needs_greek() {
    assert!(transform("A", "I II III IV", "B-THIN", "AAAA", "AAAA", "").is_err());
}

#[test]
fn crack_recovers_known_m3_settings() {
    let hits = crack(
        "BDZGO", "AAAAA", "I II III", "B", "AAA", "", "", false, false, false, None, 5,
    )
    .unwrap();
    assert!(
        hits.iter()
            .any(|h| h.rotors == "I II III" && h.reflector == "B" && h.position == "AAA")
    );
}

#[test]
fn crack_respects_fixed_position() {
    let hits = crack(
        "BDZGO", "AAAAA", "I II III", "B", "AAA", "AAA", "", false, false, false, None, 5,
    )
    .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].plaintext, "AAAAA");
}

#[test]
fn crack_rejects_wrong_crib() {
    let hits = crack(
        "BDZGO", "ZZZZZ", "I II III", "B", "AAA", "AAA", "", false, false, false, None, 5,
    )
    .unwrap();
    assert!(hits.is_empty());
}

#[test]
fn check_crack_letters_rejects_oversized() {
    let start = std::time::Instant::now();
    let err = enigma::check_crack_letters(MAX_CRACK_LETTERS + 1).unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
    assert!(
        start.elapsed().as_millis() < 100,
        "oversized ciphertext must be rejected before the search"
    );
}

#[test]
fn check_crack_letters_accepts_at_limit() {
    enigma::check_crack_letters(MAX_CRACK_LETTERS).unwrap();
    enigma::check_crack_letters(0).unwrap();
}

#[test]
fn check_crack_hits_rejects_oversized() {
    let err = enigma::check_crack_hits(MAX_CRACK_HITS + 1).unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
}

#[test]
fn check_crack_hits_accepts_at_limit() {
    enigma::check_crack_hits(MAX_CRACK_HITS).unwrap();
    enigma::check_crack_hits(0).unwrap();
}

#[test]
fn crack_rejects_oversized_ciphertext() {
    let start = std::time::Instant::now();
    let cipher = "A".repeat(MAX_CRACK_LETTERS + 1);
    let err = crack(
        &cipher, "A", "I II III", "B", "AAA", "", "", false, false, false, None, 5,
    )
    .unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
    assert!(
        start.elapsed().as_millis() < 100,
        "oversized ciphertext must be rejected before the search"
    );
}

#[test]
fn crack_rejects_oversized_hit_limit() {
    let err = crack(
        "BDZGO",
        "AAAAA",
        "I II III",
        "B",
        "AAA",
        "",
        "",
        false,
        false,
        false,
        None,
        MAX_CRACK_HITS + 1,
    )
    .unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
}

#[test]
fn crack_run_rejects_oversized_ciphertext() {
    let start = std::time::Instant::now();
    let err = enigma::run(EnigmaAction::Crack {
        input: "A".repeat(MAX_CRACK_LETTERS + 1),
        crib: "A".into(),
        rotors: "I II III".into(),
        reflector: "B".into(),
        rings: "AAA".into(),
        position: String::new(),
        plugboard: String::new(),
        navy: false,
        m4: false,
        search_rings: false,
        offset: None,
        max_hits: 5,
    })
    .unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
    assert!(
        start.elapsed().as_millis() < 100,
        "oversized ciphertext must be rejected before the search"
    );
}

#[test]
fn crack_run_rejects_oversized_hit_limit() {
    let err = enigma::run(EnigmaAction::Crack {
        input: "BDZGO".into(),
        crib: "AAAAA".into(),
        rotors: "I II III".into(),
        reflector: "B".into(),
        rings: "AAA".into(),
        position: String::new(),
        plugboard: String::new(),
        navy: false,
        m4: false,
        search_rings: false,
        offset: None,
        max_hits: MAX_CRACK_HITS + 1,
    })
    .unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
}

#[test]
fn crack_accepts_ciphertext_at_letter_cap() {
    let cipher = "A".repeat(MAX_CRACK_LETTERS);
    crack(
        &cipher, "A", "I II III", "B", "AAA", "AAA", "", false, false, false, None, 1,
    )
    .unwrap();
}

#[test]
fn u264_m4_decrypts() {
    let cipher = "NCZWVUSXPNYMINHZXMQXSFWXWLKJAHSHNMCOCCAKUQPMKCSMHKSEINJUSBLKIOSXCKUBHMLLXCSJUSRRDVKOHULXWCCBGVLIYXEOAHXRHKKFVDREWEZLXOBAFGYUJQUKGRTVUKAMEURBVEKSUHHVOYHABCJWMAKLFKLMYFVNRIZRVVRTKOFDANJMOLBGFFLEOPRGTFLVRHOWOPBEKVWMUQFMPWPARMFHAGKXIIBG";
    let plain = transform(
        cipher,
        "BETA II IV I",
        "B-THIN",
        "1 1 1 22",
        "VJNA",
        "AT BL DF GJ HM NW OP QY RZ VX",
    )
    .unwrap();
    assert!(plain.starts_with("VONVONJLOOKS"), "{plain}");
}

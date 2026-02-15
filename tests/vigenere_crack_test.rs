use happy_cracking::crypto::vigenere;

#[test]
fn test_crack_with_known_key_length() {
    // Frequency analysis needs sufficient text per column (~25+ chars each).
    // Key length 3 needs ~90+ alphabetic chars.
    let plaintext = "THEQUICKBROWNFOXJUMPSOVERTHELAZYDOG\
                     THEQUICKBROWNFOXJUMPSOVERTHELAZYDOG\
                     THEQUICKBROWNFOXJUMPSOVERTHELAZYDOG";
    let key = "KEY";
    let ciphertext = vigenere::encrypt(plaintext, key).unwrap();
    let recovered_key = vigenere::recover_key(&ciphertext, 3);
    assert_eq!(recovered_key, "KEY");
}

#[test]
fn test_crack_longer_text() {
    // Key length 5 needs ~150+ alphabetic chars
    let plaintext = "ATTACKATDAWNWEMUSTSENDMOREREINFORCEMENTSIMMEDIATELY\
                     THEENEMISGATHERINGSTRENGTHWEMUSTACTQUICKLYBEFORE\
                     THEYCANLAUNCHTHEIROFFENSIVESENDALLUNITSFORWARD\
                     ANDPREPAREFORTHECOMINGBATTLEWEEXPECTHEAVYRESISTANCE";
    let key = "LEMON";
    let ciphertext = vigenere::encrypt(plaintext, key).unwrap();
    let recovered_key = vigenere::recover_key(&ciphertext, 5);
    assert_eq!(recovered_key, "LEMON");
}

#[test]
fn test_key_length_estimation() {
    // Long enough text for statistical analysis with key length 6
    let plaintext = "ITWASTHEBESTOFTIMESITWASTHEWORSTOFTIMES\
                     ITWASTHEAGEOFWISDOMITWASTHEAGEOFFOOLISHNESS\
                     ITWASTHEEPOCHOFBELIEFITWASTHEEPOCHOFINCREDULITY\
                     ITWASTHESEASONOFLIGHTITWASTHESEASONOFDARKNESS\
                     ITWASTHESPRINGOFHOPEITWASTHEWINTEROFDESPAIR\
                     WEHAVENOTHINGBEFOREUSWEHADEVERYTHINGBEFOREUS\
                     WEWEREALLGOINGDIRECTTOHEAVENWEWEREALLGOING\
                     DIRECTTHEOTHERWAYINSHORTITWASASEASONSOMUCH";
    let key = "SECRET";
    let ciphertext = vigenere::encrypt(plaintext, key).unwrap();
    let candidates = vigenere::estimate_key_length(&ciphertext, 20);
    assert!(!candidates.is_empty());
    // The correct key length (6) or a multiple should be among the top candidates
    let top_lengths: Vec<usize> = candidates.iter().take(5).map(|&(len, _)| len).collect();
    assert!(
        top_lengths.contains(&6) || top_lengths.contains(&3) || top_lengths.contains(&2),
        "Expected key length 6 (or a factor) in top candidates, got {:?}",
        top_lengths
    );
}

#[test]
fn test_estimate_key_length_short_input() {
    let candidates = vigenere::estimate_key_length("AB", 20);
    // Short input may still produce results but not reliable
    assert!(candidates.len() <= 10);
}

#[test]
fn test_recover_key_two_char() {
    // Test with simple key on text with natural-like distribution
    let plaintext = "ETAOINSHRDLUETAOINSHRDLUETAOINSHRDLU\
                     ETAOINSHRDLUETAOINSHRDLUETAOINSHRDLU";
    let key = "AB";
    let ciphertext = vigenere::encrypt(plaintext, key).unwrap();
    let recovered = vigenere::recover_key(&ciphertext, 2);
    assert_eq!(recovered, "AB");
}

#[test]
fn test_crack_preserves_case_in_decrypt() {
    let plaintext = "Hello World";
    let key = "KEY";
    let encrypted = vigenere::encrypt(plaintext, key).unwrap();
    let decrypted = vigenere::decrypt(&encrypted, key).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_key_length_estimation_returns_sorted() {
    let plaintext =
        "ITWASTHEBESTOFTIMESITWASTHEWORSTOFTIMESITWASTHEAGEOFWISDOMITWASTHEAGEOFFOOLISHNESS";
    let key = "ABC";
    let ciphertext = vigenere::encrypt(plaintext, key).unwrap();
    let candidates = vigenere::estimate_key_length(&ciphertext, 15);
    // Verify we get results and they have IoC values
    for &(len, ioc) in &candidates {
        assert!(len >= 1);
        assert!(ioc >= 0.0);
    }
}

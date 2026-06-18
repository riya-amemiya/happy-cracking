use happy_cracking::crypto::filetype;

#[test]
fn test_png_detected() {
    let data = [
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D,
    ];
    let matches = filetype::identify(&data);
    assert!(matches.iter().any(|s| s.name == "PNG"));
    assert!(matches.iter().any(|s| s.offset == 0));
}

#[test]
fn test_elf_detected() {
    let data = [0x7F, 0x45, 0x4C, 0x46, 0x02, 0x01, 0x01, 0x00];
    let matches = filetype::identify(&data);
    assert!(matches.iter().any(|s| s.name == "ELF"));
}

#[test]
fn test_empty_no_matches() {
    let matches = filetype::identify(&[]);
    assert!(matches.is_empty());
}

#[test]
fn test_riff_webp_form_type() {
    let mut data = Vec::new();
    data.extend_from_slice(b"RIFF");
    data.extend_from_slice(&[0x10, 0x00, 0x00, 0x00]);
    data.extend_from_slice(b"WEBP");
    let matches = filetype::identify(&data);
    assert!(matches.iter().any(|s| s.name == "WEBP"));
    assert!(!matches.iter().any(|s| s.name == "WAV"));
}

#[test]
fn test_riff_wave_form_type() {
    let mut data = Vec::new();
    data.extend_from_slice(b"RIFF");
    data.extend_from_slice(&[0x24, 0x00, 0x00, 0x00]);
    data.extend_from_slice(b"WAVE");
    let matches = filetype::identify(&data);
    assert!(matches.iter().any(|s| s.name == "WAV"));
}

#[test]
fn test_tar_offset_257() {
    let mut data = vec![0u8; 262];
    data[257..262].copy_from_slice(b"ustar");
    let matches = filetype::identify(&data);
    let tar = matches.iter().find(|s| s.name == "TAR").unwrap();
    assert_eq!(tar.offset, 257);
}

#[test]
fn test_unknown_no_match() {
    let data = [0x12, 0x34, 0x56, 0x78];
    let matches = filetype::identify(&data);
    assert!(matches.is_empty());
}

#[test]
fn test_bad_hex_input_errors() {
    let action = filetype::FiletypeAction::Identify {
        input: Some("zzgg".to_string()),
        file: None,
    };
    assert!(filetype::run(action).is_err());
}

#[test]
fn test_no_input_errors() {
    let action = filetype::FiletypeAction::Identify {
        input: None,
        file: None,
    };
    assert!(filetype::run(action).is_err());
}

use happy_cracking::crypto::crc32_forge;

#[test]
fn test_crc32_compute_known_values() {
    assert_eq!(crc32_forge::crc32_compute(b""), 0x00000000);
    assert_eq!(crc32_forge::crc32_compute(b"hello"), 0x3610a686);
    assert_eq!(crc32_forge::crc32_compute(b"123456789"), 0xcbf43926);
}

#[test]
fn test_forge_deadbeef() {
    let data = b"Hello, World!";
    let target = 0xDEADBEEF_u32;
    let forge_bytes = crc32_forge::forge_crc32(data, target);
    let mut combined = data.to_vec();
    combined.extend_from_slice(&forge_bytes);
    assert_eq!(crc32_forge::crc32_compute(&combined), target);
}

#[test]
fn test_forge_zero_target() {
    let data = b"test data";
    let target = 0x00000000_u32;
    let forge_bytes = crc32_forge::forge_crc32(data, target);
    let mut combined = data.to_vec();
    combined.extend_from_slice(&forge_bytes);
    assert_eq!(crc32_forge::crc32_compute(&combined), target);
}

#[test]
fn test_forge_max_target() {
    let data = b"flag{crc32_forge}";
    let target = 0xFFFFFFFF_u32;
    let forge_bytes = crc32_forge::forge_crc32(data, target);
    let mut combined = data.to_vec();
    combined.extend_from_slice(&forge_bytes);
    assert_eq!(crc32_forge::crc32_compute(&combined), target);
}

#[test]
fn test_forge_empty_data() {
    let data = b"";
    let target = 0x12345678_u32;
    let forge_bytes = crc32_forge::forge_crc32(data, target);
    let mut combined = data.to_vec();
    combined.extend_from_slice(&forge_bytes);
    assert_eq!(crc32_forge::crc32_compute(&combined), target);
}

#[test]
fn test_forge_preserves_original_crc() {
    let data = b"hello";
    let original_crc = crc32_forge::crc32_compute(data);
    let forge_bytes = crc32_forge::forge_crc32(data, original_crc);
    let mut combined = data.to_vec();
    combined.extend_from_slice(&forge_bytes);
    assert_eq!(crc32_forge::crc32_compute(&combined), original_crc);
}

#[test]
fn test_forge_different_data_same_target() {
    let target = 0xCAFEBABE_u32;

    let data1 = b"Alice";
    let forge1 = crc32_forge::forge_crc32(data1, target);
    let mut combined1 = data1.to_vec();
    combined1.extend_from_slice(&forge1);

    let data2 = b"Bob";
    let forge2 = crc32_forge::forge_crc32(data2, target);
    let mut combined2 = data2.to_vec();
    combined2.extend_from_slice(&forge2);

    assert_eq!(crc32_forge::crc32_compute(&combined1), target);
    assert_eq!(crc32_forge::crc32_compute(&combined2), target);
}

#[test]
fn test_forge_ctf_scenario() {
    let flag = b"flag{crc32_is_not_secure}";
    let target = 0x13371337_u32;
    let forge_bytes = crc32_forge::forge_crc32(flag, target);
    let mut payload = flag.to_vec();
    payload.extend_from_slice(&forge_bytes);
    assert_eq!(crc32_forge::crc32_compute(&payload), target);
}

#[test]
fn test_forge_bytes_are_exactly_4() {
    let data = b"test";
    let target = 0xABCDABCD_u32;
    let forge_bytes = crc32_forge::forge_crc32(data, target);
    assert_eq!(forge_bytes.len(), 4);
}

#[test]
fn test_crc32_compute_single_byte() {
    // Single byte 'a' = 0x61
    let crc = crc32_forge::crc32_compute(b"a");
    assert_eq!(crc, 0xe8b7be43);
}

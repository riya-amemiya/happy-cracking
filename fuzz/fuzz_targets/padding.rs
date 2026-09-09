#![no_main]

use happy_cracking::crypto::padding;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() < 2 {
        return;
    }
    let block_size = (data[0] as usize % 255) + 1;
    let payload = &data[1..];
    if payload.len() > 65_536 {
        return;
    }
    let _ = padding::pkcs7_pad(payload, block_size);
    let _ = padding::pkcs7_unpad(payload, block_size);
    let _ = padding::zero_pad(payload, block_size);
    let _ = padding::zero_unpad(payload);
});

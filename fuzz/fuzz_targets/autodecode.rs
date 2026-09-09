#![no_main]

use happy_cracking::crypto::autodecode;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    if s.len() > 65_536 {
        return;
    }
    let _ = autodecode::detect_and_decode(s);
    // Keep the tree tiny so aggressive mode cannot dominate a CFL time slice.
    let _ = autodecode::decode_tree(s, 3, 16);
});

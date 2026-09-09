#![no_main]

use happy_cracking::crypto::hexdump;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    if s.len() > 65_536 {
        return;
    }
    let _ = hexdump::reverse(s);
});

#![no_main]

use happy_cracking::crypto::jwt;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    if s.len() > 65_536 {
        return;
    }
    if let Ok(parts) = jwt::decode(s) {
        let _ = jwt::extract_algorithm(&parts.header);
        let _ = jwt::find_vulnerabilities(&parts.header);
        let _ = jwt::forge_none(&parts.payload);
    }
});

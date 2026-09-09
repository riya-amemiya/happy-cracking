#![no_main]

use happy_cracking::crypto::chain;
use libfuzzer_sys::fuzz_target;

const OPS: &[&str] = &[
    "base64-encode",
    "base64-decode",
    "base32-encode",
    "base32-decode",
    "hex-encode",
    "hex-decode",
    "url-encode",
    "url-decode",
    "binary-encode",
    "binary-decode",
    "rot13",
    "rot47",
    "reverse",
    "upper",
    "lower",
];

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }
    let n_ops = usize::from(data[0] % 8) + 1;
    let header_len = 1 + n_ops;
    if data.len() < header_len {
        return;
    }
    let mut ops = String::new();
    for i in 0..n_ops {
        if i > 0 {
            ops.push(',');
        }
        let idx = usize::from(data[1 + i]) % OPS.len();
        ops.push_str(OPS[idx]);
    }
    let payload = &data[header_len..];
    if payload.len() > 65_536 {
        return;
    }
    let Ok(s) = std::str::from_utf8(payload) else {
        return;
    };
    let _ = chain::chain(s, &ops);
});

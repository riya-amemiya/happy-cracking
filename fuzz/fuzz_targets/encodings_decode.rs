#![no_main]

use happy_cracking::crypto::{
    base32, base45, base58, base62, base64, base85, base91, binary, hex, quotedprintable, url,
    uuencode,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }
    let selector = data[0];
    let payload = &data[1..];
    if payload.len() > 65_536 {
        return;
    }
    let Ok(s) = std::str::from_utf8(payload) else {
        return;
    };
    match selector % 12 {
        0 => {
            let _ = hex::decode(s);
        }
        1 => {
            let _ = base64::decode(s);
        }
        2 => {
            let _ = base32::decode(s);
        }
        3 => {
            let _ = base58::decode(s);
        }
        4 => {
            let _ = base62::decode(s);
        }
        5 => {
            let _ = base85::decode(s);
        }
        6 => {
            let _ = base91::decode(s);
        }
        7 => {
            let _ = base45::decode(s);
        }
        8 => {
            let _ = binary::decode(s);
        }
        9 => {
            let _ = url::decode(s);
        }
        10 => {
            let _ = quotedprintable::decode(s);
        }
        _ => {
            let _ = uuencode::decode(s);
        }
    }
});

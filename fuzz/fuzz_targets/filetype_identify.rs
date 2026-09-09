#![no_main]

use happy_cracking::crypto::filetype;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let slice = if data.len() > 65_536 {
        &data[..65_536]
    } else {
        data
    };
    let _ = filetype::identify(slice);
});

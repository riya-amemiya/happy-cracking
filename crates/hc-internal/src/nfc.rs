//! Convert macOS decomposed (UTF-8-MAC / NFD) filename bytes to NFC.
//!
//! Git's `core.precomposeunicode` uses iconv's UTF-8-MAC mapping; Unicode NFC
//! is the safe equivalent for typical filename data.

#[cfg(target_os = "macos")]
mod imp {
    use unicode_normalization::UnicodeNormalization;

    pub fn precomposed(raw: &[u8]) -> Option<Vec<u8>> {
        if !raw.iter().any(|&b| b >= 0x80) {
            return None;
        }
        let s = std::str::from_utf8(raw).ok()?;
        Some(s.nfc().collect::<String>().into_bytes())
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub fn precomposed(_raw: &[u8]) -> Option<Vec<u8>> {
        None
    }
}

pub fn precomposed(raw: &[u8]) -> Option<Vec<u8>> {
    imp::precomposed(raw)
}

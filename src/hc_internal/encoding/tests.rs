use std::borrow::Cow;
use std::io::Read;

use super::*;

fn decode_with(encoding: &'static Encoding, input: &[u8]) -> Vec<u8> {
    let opts = DecodeOptions {
        encoding: Some(encoding),
        bom_sniffing: false,
    };
    decode_all(input, &opts).into_owned()
}

fn decode_chunked(
    encoding: &'static Encoding,
    input: &[u8],
    chunk: usize,
    out_size: usize,
) -> Vec<u8> {
    let mut decoder = encoding.new_decoder_without_bom_handling();
    let mut out = Vec::new();
    let mut buffer = vec![0u8; out_size];
    let mut pieces = input.chunks(chunk).peekable();
    loop {
        let piece = pieces.next().unwrap_or(&[]);
        let last = pieces.peek().is_none();
        let mut read = 0;
        loop {
            let (result, r, w, _) = decoder.decode_to_utf8(&piece[read..], &mut buffer, last);
            read += r;
            out.extend_from_slice(&buffer[..w]);
            if result == CoderResult::InputEmpty {
                break;
            }
        }
        if last {
            return out;
        }
    }
}

#[test]
fn labels_resolve_with_trimming_and_case() {
    assert_eq!(Encoding::for_label(b"utf-8"), Some(UTF_8));
    assert_eq!(Encoding::for_label(b"  UTF8\t\n"), Some(UTF_8));
    assert_eq!(Encoding::for_label(b"Shift_JIS"), Some(SHIFT_JIS));
    assert_eq!(Encoding::for_label(b"latin1"), Some(WINDOWS_1252));
    assert_eq!(Encoding::for_label(b"utf-16"), Some(UTF_16LE));
    assert_eq!(Encoding::for_label(b"x-user-defined"), Some(X_USER_DEFINED));
    assert_eq!(Encoding::for_label(b"cseucpkdfmtjapanese"), Some(EUC_JP));
    assert_eq!(Encoding::for_label(b"utf 8"), None);
    assert_eq!(Encoding::for_label(b"\x0butf-8"), None);
    assert_eq!(Encoding::for_label(b""), None);
    assert_eq!(Encoding::for_label(b"cseucpkdfmtjapanese1"), None);
    assert_eq!(Encoding::for_label(b"iso-2022-kr"), Some(REPLACEMENT));
    assert_eq!(Encoding::for_label_no_replacement(b"iso-2022-kr"), None);
    assert_eq!(Encoding::for_label_no_replacement(b"replacement"), None);
    assert_eq!(Encoding::for_label_no_replacement(b"gbk"), Some(GBK));
    assert_eq!(GBK.name(), "GBK");
    assert_eq!(ISO_8859_8_I.name(), "ISO-8859-8-I");
}

#[test]
fn bom_detection() {
    assert_eq!(Encoding::for_bom(b"\xEF\xBB\xBFx"), Some((UTF_8, 3)));
    assert_eq!(Encoding::for_bom(b"\xFF\xFE"), Some((UTF_16LE, 2)));
    assert_eq!(Encoding::for_bom(b"\xFE\xFFab"), Some((UTF_16BE, 2)));
    assert_eq!(Encoding::for_bom(b"\xEF\xBB"), None);
    let auto = DecodeOptions::default();
    assert!(!needs_transcoding(b"abc", &auto));
    assert!(needs_transcoding(b"\xFF\xFE", &auto));
    assert!(needs_transcoding(b"\xEF\xBB\xBF", &auto));
    let none = DecodeOptions {
        encoding: None,
        bom_sniffing: false,
    };
    assert!(!needs_transcoding(b"\xFF\xFEa\x00", &none));
    let explicit = DecodeOptions {
        encoding: Some(SHIFT_JIS),
        bom_sniffing: true,
    };
    assert!(needs_transcoding(b"abc", &explicit));
}

#[test]
fn decode_all_borrows_without_transcoding() {
    let auto = DecodeOptions::default();
    assert!(matches!(
        decode_all(b"plain", &auto),
        Cow::Borrowed(b"plain")
    ));
    assert!(matches!(
        decode_all(b"\xEF\xBB\xBFflag{x}", &auto),
        Cow::Borrowed(b"flag{x}")
    ));
    let latin = DecodeOptions {
        encoding: Some(WINDOWS_1252),
        bom_sniffing: true,
    };
    assert!(matches!(
        decode_all(b"ascii only", &latin),
        Cow::Borrowed(_)
    ));
    assert_eq!(
        decode_all(b"caf\xE9", &latin).as_ref(),
        "caf\u{e9}".as_bytes()
    );
}

#[test]
fn bom_sniffing_matches_ripgrep() {
    let auto = DecodeOptions::default();
    assert_eq!(decode_all(b"\xFF\xFEf\x00l\x00", &auto).as_ref(), b"fl");
    assert_eq!(decode_all(b"\xFE\xFF\x00f\x00l", &auto).as_ref(), b"fl");
    assert_eq!(decode_all(b"\xFF\xFE", &auto).as_ref(), b"");
    assert_eq!(
        decode_all(b"\xFF\xFEA", &auto).as_ref(),
        "\u{FFFD}".as_bytes()
    );
    assert_eq!(decode_all(b"\xFF\xFE\xFF\xFEA\x00", &auto).as_ref(), b"A");
    assert_eq!(
        decode_all(b"\xEF\xBB\xBF\xEF\xBB\xBFA", &auto).as_ref(),
        b"\xEF\xBB\xBFA"
    );
    assert_eq!(decode_all(b"\xEF\xBB", &auto).as_ref(), b"\xEF\xBB");
    let sjis = DecodeOptions {
        encoding: Some(SHIFT_JIS),
        bom_sniffing: true,
    };
    assert_eq!(decode_all(b"\xFF\xFEA\x00", &sjis).as_ref(), b"A");
    assert_eq!(
        decode_all(b"\xEF\xBB\xBF\x82\xA0", &sjis).as_ref(),
        "\u{3042}".as_bytes()
    );
    let utf8 = DecodeOptions {
        encoding: Some(UTF_8),
        bom_sniffing: true,
    };
    assert_eq!(
        decode_all(b"\xEF\xBB\xBF\xEF\xBB\xBFA\xFF", &utf8).as_ref(),
        "A\u{FFFD}".as_bytes()
    );
    let disabled = DecodeOptions {
        encoding: None,
        bom_sniffing: false,
    };
    assert_eq!(
        decode_all(b"\xFF\xFEA\x00", &disabled).as_ref(),
        b"\xFF\xFEA\x00"
    );
    let utf16_no_sniff = DecodeOptions {
        encoding: Some(UTF_16LE),
        bom_sniffing: false,
    };
    assert_eq!(decode_all(b"\xFF\xFEA\x00", &utf16_no_sniff).as_ref(), b"A");
}

#[test]
fn utf8_maximal_subparts() {
    let cases: [(&[u8], &str); 8] = [
        (b"\xE0\x80", "\u{FFFD}\u{FFFD}"),
        (b"\xF0\x90\x80A", "\u{FFFD}A"),
        (b"\xC0\xAF", "\u{FFFD}\u{FFFD}"),
        (b"\xED\xA0\x80", "\u{FFFD}\u{FFFD}\u{FFFD}"),
        (b"\xF4\x90\x80\x80", "\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}"),
        (b"a\xE3\x81", "a\u{FFFD}"),
        (b"\xE3\x81\x82", "\u{3042}"),
        (b"\xF0\x9F\x98\x80\xFF", "\u{1F600}\u{FFFD}"),
    ];
    for (input, expected) in cases {
        assert_eq!(decode_with(UTF_8, input), expected.as_bytes(), "{input:x?}");
        assert_eq!(
            decode_chunked(UTF_8, input, 1, 8),
            expected.as_bytes(),
            "{input:x?}"
        );
    }
}

#[test]
fn utf16_errors() {
    let cases: [(&[u8], &str); 7] = [
        (b"a\x00b", "a\u{FFFD}"),
        (b"\x3D\xD8\xA9", "\u{FFFD}"),
        (b"\x3D\xD8\xA9\xDC\x03\x26", "\u{1F4A9}\u{2603}"),
        (b"\xA9\xDC\x03\x26", "\u{FFFD}\u{2603}"),
        (b"\x3D\xD8\x03\x26", "\u{FFFD}\u{2603}"),
        (b"\x3D\xD8\x3D\xD8\xA9\xDC", "\u{FFFD}\u{1F4A9}"),
        (b"f\x00l\x00a\x00g\x00{\x00x\x00}\x00\n\x00", "flag{x}\n"),
    ];
    for (input, expected) in cases {
        assert_eq!(
            decode_with(UTF_16LE, input),
            expected.as_bytes(),
            "{input:x?}"
        );
        assert_eq!(
            decode_chunked(UTF_16LE, input, 1, 8),
            expected.as_bytes(),
            "{input:x?}"
        );
        let swapped: Vec<u8> = input
            .chunks(2)
            .flat_map(|pair| pair.iter().rev().copied().collect::<Vec<u8>>())
            .collect();
        if input.len() % 2 == 0 {
            assert_eq!(
                decode_with(UTF_16BE, &swapped),
                expected.as_bytes(),
                "{input:x?}"
            );
        }
    }
}

#[test]
fn legacy_multibyte_errors() {
    let cases: [(&'static Encoding, &[u8], &str); 16] = [
        (SHIFT_JIS, b"\x82\xA0\x82\n", "\u{3042}\u{FFFD}\n"),
        (SHIFT_JIS, b"\x82A", "\u{FFFD}A"),
        (SHIFT_JIS, b"\x82\xFF", "\u{FFFD}"),
        (SHIFT_JIS, b"\x80\xA1\xF0\x40", "\u{80}\u{FF61}\u{E000}"),
        (
            EUC_JP,
            b"\xA4\xA2\x8E\xB1\x8F\xB0\xA1",
            "\u{3042}\u{FF71}\u{4E02}",
        ),
        (EUC_JP, b"\x8F\xA1A", "\u{FFFD}A"),
        (EUC_KR, b"\xB0\xA1\xB0\n", "\u{AC00}\u{FFFD}\n"),
        (BIG5, b"\x88\x62\x88\x64", "\u{CA}\u{304}\u{CA}\u{30C}"),
        (BIG5, b"\xA4\x40\x80", "\u{4E00}\u{FFFD}"),
        (GBK, b"\x80\x81\x40", "\u{20AC}\u{4E02}"),
        (GB18030, b"\x81\x30\x81\x30", "\u{80}"),
        (GB18030, b"\x81\x30\x81A", "\u{FFFD}0\u{4E04}"),
        (GB18030, b"\x90\x30\x81\x30", "\u{10000}"),
        (GB18030, b"\x84\x31\xA5\x30\x81", "\u{FFFD}\u{FFFD}"),
        (ISO_2022_JP, b"\x1b$B$\"\x1b(Bok", "\u{3042}ok"),
        (ISO_2022_JP, b"\x1b(B\x1b(B", "\u{FFFD}"),
    ];
    for (encoding, input, expected) in cases {
        let label = encoding.name();
        assert_eq!(
            decode_with(encoding, input),
            expected.as_bytes(),
            "{label} {input:x?}"
        );
        for chunk in 1..4 {
            assert_eq!(
                decode_chunked(encoding, input, chunk, 8),
                expected.as_bytes(),
                "{label} {input:x?}"
            );
        }
    }
}

#[test]
fn trailing_lead_follows_ripgrep_chunking() {
    let sjis = DecodeOptions {
        encoding: Some(SHIFT_JIS),
        bom_sniffing: true,
    };
    let replacement = "\u{FFFD}".as_bytes();
    assert_eq!(decode_all(b"\x82", &sjis).as_ref(), replacement);
    assert_eq!(decode_all(b"A\x82", &sjis).as_ref(), b"A");
    assert_eq!(decode_all(b"AB\x82", &sjis).as_ref(), b"AB");
    assert_eq!(
        decode_all(b"ABC\x82", &sjis).as_ref(),
        "ABC\u{FFFD}".as_bytes()
    );
    assert_eq!(decode_all(b"ABCD\x82", &sjis).as_ref(), b"ABCD");
    assert_eq!(decode_all(b"\xEF\xBB\xBF\x82", &sjis).as_ref(), replacement);
    let mut long = vec![b'a'; 3 + 8192];
    long.push(0x82);
    let decoded = decode_all(&long, &sjis);
    assert!(decoded.ends_with("a\u{FFFD}".as_bytes()));
    long.insert(0, b'a');
    assert!(decode_all(&long, &sjis).ends_with(b"aa"));
    for encoding in [EUC_KR, BIG5] {
        let opts = DecodeOptions {
            encoding: Some(encoding),
            bom_sniffing: true,
        };
        assert_eq!(decode_all(b"AB\xB0", &opts).as_ref(), b"AB");
        assert_eq!(
            decode_all(b"ABC\xB0", &opts).as_ref(),
            "ABC\u{FFFD}".as_bytes()
        );
    }
    let gbk = DecodeOptions {
        encoding: Some(GBK),
        bom_sniffing: true,
    };
    assert_eq!(
        decode_all(b"AB\x81", &gbk).as_ref(),
        "AB\u{FFFD}".as_bytes()
    );
    assert_eq!(
        decode_chunked(SHIFT_JIS, b"AB\x82", 2, 8),
        "AB\u{FFFD}".as_bytes()
    );
}

#[test]
fn single_byte_tables() {
    assert_eq!(
        decode_with(WINDOWS_1252, b"\x80\x81\xFF"),
        "\u{20AC}\u{81}\u{FF}".as_bytes()
    );
    assert_eq!(decode_with(WINDOWS_874, b"\xDB"), "\u{FFFD}".as_bytes());
    assert_eq!(
        decode_with(X_USER_DEFINED, b"a\x80\xFF"),
        "a\u{F780}\u{F7FF}".as_bytes()
    );
    assert_eq!(decode_with(KOI8_R, b"\xC1"), "\u{430}".as_bytes());
    assert_eq!(decode_with(REPLACEMENT, b"abc"), "\u{FFFD}".as_bytes());
    assert_eq!(decode_with(REPLACEMENT, b""), b"");
}

#[test]
fn reader_matches_decode_all() {
    let inputs: [&[u8]; 4] = [
        b"\xFF\xFEf\x00l\x00a\x00g\x00\n\x00\x3D\xD8",
        b"\xEF\xBB\xBFplain\xFF",
        b"no bom at all",
        b"\xFE\xFF",
    ];
    let options = [
        DecodeOptions::default(),
        DecodeOptions {
            encoding: Some(SHIFT_JIS),
            bom_sniffing: true,
        },
        DecodeOptions {
            encoding: Some(UTF_16BE),
            bom_sniffing: false,
        },
    ];
    for input in inputs {
        for opts in &options {
            let expected = decode_all(input, opts).into_owned();
            for size in [1, 3, 64] {
                let mut reader = DecodeReader::new(input, *opts);
                let mut out = Vec::new();
                let mut buf = vec![0u8; size];
                loop {
                    let n = reader.read(&mut buf).unwrap();
                    if n == 0 {
                        break;
                    }
                    out.extend_from_slice(&buf[..n]);
                }
                assert_eq!(out, expected, "{input:x?} {opts:?} {size}");
            }
        }
    }
}

#[test]
fn max_buffer_length_is_sufficient() {
    let input: Vec<u8> = (0..=255u8).cycle().take(4096).collect();
    for (_, encoding) in super::data::LABELS {
        let mut decoder = encoding.new_decoder_without_bom_handling();
        let mut out = vec![0u8; decoder.max_utf8_buffer_length(input.len()).unwrap()];
        let (result, read, _, _) = decoder.decode_to_utf8(&input, &mut out, true);
        assert_eq!(result, CoderResult::InputEmpty, "{}", encoding.name());
        assert_eq!(read, input.len());
    }
}

fn random_input(seed: &mut u64, len: usize) -> Vec<u8> {
    let alphabet = b"\x1b$(BJ@I\x0e\x0f\n A0\x80\x81\x82\x8e\x8f\xa0\xa1\xa4\xb0\xc8\xd8\xdc\xdf\xe0\xed\xef\xf0\xf4\xfe\xff\x30\x39\x40\x7e\x7f\x00";
    (0..len)
        .map(|_| {
            *seed ^= *seed << 13;
            *seed ^= *seed >> 7;
            *seed ^= *seed << 17;
            alphabet[(*seed % alphabet.len() as u64) as usize]
        })
        .collect()
}

#[test]
fn streaming_matches_one_shot() {
    let mut seed = 0x1234_5678_9ABC_DEF1u64;
    for (_, encoding) in super::data::LABELS {
        for round in 0..40 {
            let input = random_input(&mut seed, 1 + round * 7);
            let expected = decode_chunked(encoding, &input, input.len(), 4 * input.len() + 16);
            for chunk in [1, 2, 3, 5, 8] {
                for out_size in [8, 9, 13] {
                    assert_eq!(
                        decode_chunked(encoding, &input, chunk, out_size),
                        expected,
                        "{} {input:x?} {chunk} {out_size}",
                        encoding.name()
                    );
                }
            }
            let mut decoder = encoding.new_decoder_without_bom_handling();
            for piece in input.chunks(3) {
                let mut out = vec![0u8; decoder.max_utf8_buffer_length(piece.len()).unwrap()];
                let (result, read, _, _) = decoder.decode_to_utf8(piece, &mut out, false);
                assert_eq!(result, CoderResult::InputEmpty, "{}", encoding.name());
                assert_eq!(read, piece.len());
            }
            let mut out = vec![0u8; decoder.max_utf8_buffer_length(0).unwrap()];
            let (result, _, _, _) = decoder.decode_to_utf8(&[], &mut out, true);
            assert_eq!(result, CoderResult::InputEmpty, "{}", encoding.name());
        }
    }
}

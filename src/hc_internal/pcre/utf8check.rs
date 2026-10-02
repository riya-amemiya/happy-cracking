use super::error::BAD_UTF_OFFSET;

fn err(n: i32) -> i32 {
    -(n + 2)
}

pub fn check(hay: &[u8], start: usize, max_lookbehind: u32) -> Option<i32> {
    let mut s = start;
    if s < hay.len() && hay[s] & 0xc0 == 0x80 {
        return Some(if start > 0 { BAD_UTF_OFFSET } else { err(20) });
    }
    let mut i = max_lookbehind;
    while i > 0 && s > 0 {
        s -= 1;
        while s > 0 && hay[s] & 0xc0 == 0x80 {
            s -= 1;
        }
        i -= 1;
    }
    if std::str::from_utf8(&hay[s..]).is_ok() {
        return None;
    }
    validate(&hay[s..])
}

fn validate(s: &[u8]) -> Option<i32> {
    let mut p = 0usize;
    let len = s.len();
    while p < len {
        let c = s[p];
        p += 1;
        if c < 0x80 {
            continue;
        }
        if c < 0xc0 {
            return Some(err(20));
        }
        if c >= 0xfe {
            return Some(err(21));
        }
        let ab: usize = match c & 0x3f {
            0x00..=0x1f => 1,
            0x20..=0x2f => 2,
            0x30..=0x37 => 3,
            0x38..=0x3b => 4,
            _ => 5,
        };
        let remaining = len - p;
        if remaining < ab {
            return Some(err((ab - remaining) as i32));
        }
        let d = s[p];
        if d & 0xc0 != 0x80 {
            return Some(err(6));
        }
        match ab {
            1 => {
                if c & 0x3e == 0 {
                    return Some(err(15));
                }
            }
            2 => {
                if s[p + 1] & 0xc0 != 0x80 {
                    return Some(err(7));
                }
                if c == 0xe0 && d & 0x20 == 0 {
                    return Some(err(16));
                }
                if c == 0xed && d >= 0xa0 {
                    return Some(err(14));
                }
            }
            3 => {
                if s[p + 1] & 0xc0 != 0x80 {
                    return Some(err(7));
                }
                if s[p + 2] & 0xc0 != 0x80 {
                    return Some(err(8));
                }
                if c == 0xf0 && d & 0x30 == 0 {
                    return Some(err(17));
                }
                if c > 0xf4 || (c == 0xf4 && d > 0x8f) {
                    return Some(err(13));
                }
            }
            4 => {
                for k in 1..4 {
                    if s[p + k] & 0xc0 != 0x80 {
                        return Some(err(6 + k as i32));
                    }
                }
                if c == 0xf8 && d & 0x38 == 0 {
                    return Some(err(18));
                }
            }
            _ => {
                for k in 1..5 {
                    if s[p + k] & 0xc0 != 0x80 {
                        return Some(err(6 + k as i32));
                    }
                }
                if c == 0xfc && d & 0x3c == 0 {
                    return Some(err(19));
                }
            }
        }
        if ab > 3 {
            return Some(err(if ab == 4 { 11 } else { 12 }));
        }
        p += ab;
    }
    None
}

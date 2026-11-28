use std::time::{Duration, SystemTime, UNIX_EPOCH};

const TS_MIN: i64 = -377_705_116_800;
const TS_MAX: i64 = 253_402_207_199;
const NS: i128 = 1_000_000_000;
const UNIT_NANOS: [i128; 6] = [1, 1_000, 1_000_000, NS, 60 * NS, 3600 * NS];
const CALENDAR_LIMITS: [u64; 4] = [7_304_484, 1_043_497, 239_976, 19_998];
const HOUR: usize = 5;
const SECOND: usize = 3;
const MINUTE: usize = 4;

#[derive(Default)]
struct Units {
    values: [u64; 10],
    fraction: Option<u32>,
    min: Option<usize>,
    any: bool,
    negative: bool,
}

impl Units {
    fn set(&mut self, unit: usize, value: u64) -> Option<()> {
        if self.min.is_some_and(|m| m <= unit) {
            return None;
        }
        self.min = Some(unit);
        self.values[unit] = value;
        self.any |= value != 0;
        Some(())
    }

    fn set_fraction(&mut self, fraction: u32) -> Option<()> {
        if self.min.is_some_and(|m| m > HOUR || m == 0) {
            return None;
        }
        self.fraction = Some(fraction);
        self.any |= fraction != 0;
        Some(())
    }

    fn span(&self) -> Option<Span> {
        self.min?;
        for (i, limit) in CALENDAR_LIMITS.iter().enumerate() {
            if self.values[6 + i] > *limit {
                return None;
            }
        }
        let mut nanos: i128 = 0;
        for (unit, scale) in UNIT_NANOS.iter().enumerate() {
            let v = self.values[unit];
            if v > i64::MAX as u64 && !(self.negative && v == 1 << 63) {
                return None;
            }
            nanos += i128::from(v) * scale;
        }
        if let (Some(f), Some(min)) = (self.fraction, self.min) {
            nanos += i128::from(f) * UNIT_NANOS[min] / NS;
        }
        let sign: i64 = if !self.any {
            0
        } else if self.negative {
            -1
        } else {
            1
        };
        let v = |i: usize| self.values[i] as i64;
        Some(Span {
            months: sign * (v(9) * 12 + v(8)),
            days: sign * (v(7) * 7 + v(6)),
            nanos: i128::from(sign) * nanos,
        })
    }
}

struct Span {
    months: i64,
    days: i64,
    nanos: i128,
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'\x0C')
}

fn skip_ws(mut input: &[u8]) -> &[u8] {
    while input.first().is_some_and(|&b| is_ws(b)) {
        input = &input[1..];
    }
    input
}

fn u64_prefix(bytes: &[u8]) -> Result<(Option<u64>, &[u8]), ()> {
    let mut count = 0;
    let mut n: u64 = 0;
    while count <= 20 {
        let Some(&b) = bytes.get(count) else { break };
        if !b.is_ascii_digit() {
            break;
        }
        count += 1;
        n = n
            .checked_mul(10)
            .and_then(|n| n.checked_add(u64::from(b - b'0')))
            .ok_or(())?;
    }
    Ok(((count > 0).then_some(n), &bytes[count..]))
}

fn fraction(input: &[u8]) -> Result<(Option<u32>, &[u8]), ()> {
    if !matches!(input.first(), Some(b'.' | b',')) {
        return Ok((None, input));
    }
    let rest = &input[1..];
    let len = rest
        .iter()
        .take(9)
        .take_while(|b| b.is_ascii_digit())
        .count();
    if len == 0 {
        return Err(());
    }
    let mut n: u32 = 0;
    for &b in &rest[..len] {
        n = n * 10 + u32::from(b - b'0');
    }
    for _ in len..9 {
        n *= 10;
    }
    Ok((Some(n), &rest[len..]))
}

const LABELS: &[(&str, usize)] = &[
    ("milliseconds", 2),
    ("microseconds", 1),
    ("nanoseconds", 0),
    ("millisecond", 2),
    ("microsecond", 1),
    ("nanosecond", 0),
    ("seconds", 3),
    ("minutes", 4),
    ("\u{b5}secs", 1),
    ("second", 3),
    ("months", 8),
    ("minute", 4),
    ("millis", 2),
    ("micros", 1),
    ("\u{b5}sec", 1),
    ("years", 9),
    ("weeks", 7),
    ("usecs", 1),
    ("nsecs", 0),
    ("nanos", 0),
    ("msecs", 2),
    ("month", 8),
    ("milli", 2),
    ("micro", 1),
    ("hours", 5),
    ("year", 9),
    ("week", 7),
    ("usec", 1),
    ("secs", 3),
    ("nsec", 0),
    ("nano", 0),
    ("msec", 2),
    ("mins", 4),
    ("hour", 5),
    ("days", 6),
    ("\u{b5}s", 1),
    ("yrs", 9),
    ("wks", 7),
    ("sec", 3),
    ("mos", 8),
    ("min", 4),
    ("hrs", 5),
    ("day", 6),
    ("yr", 9),
    ("wk", 7),
    ("us", 1),
    ("ns", 0),
    ("ms", 2),
    ("mo", 8),
    ("hr", 5),
    ("y", 9),
    ("w", 7),
    ("s", 3),
    ("m", 4),
    ("h", 5),
    ("d", 6),
];

fn designator(input: &[u8]) -> Option<(usize, &[u8])> {
    LABELS.iter().find_map(|(label, unit)| {
        input
            .starts_with(label.as_bytes())
            .then(|| (*unit, &input[label.len()..]))
    })
}

fn friendly(input: &[u8]) -> Option<Span> {
    let had_prefix_sign = matches!(input.first(), Some(b'+' | b'-'));
    let mut units = Units {
        negative: input.first() == Some(&b'-'),
        ..Units::default()
    };
    let mut input = if had_prefix_sign { &input[1..] } else { input };
    let (first, rest) = u64_prefix(input).ok()?;
    let mut value = first?;
    input = rest;
    let mut after_comma = true;
    loop {
        if let Some(tail) = input.strip_prefix(b":") {
            let (minute, rest) = u64_prefix(tail).ok()?;
            let rest = rest.strip_prefix(b":")?;
            let (second, rest) = u64_prefix(rest).ok()?;
            let (frac, rest) = fraction(rest).ok()?;
            if units.min.is_some_and(|m| m <= HOUR) {
                return None;
            }
            units.set(HOUR, value)?;
            units.set(MINUTE, minute?)?;
            units.set(SECOND, second?)?;
            if let Some(f) = frac {
                units.set_fraction(f)?;
            }
            input = rest;
            break;
        }
        let (frac, rest) = fraction(input).ok()?;
        let (unit, rest) = designator(skip_ws(rest))?;
        input = rest;
        if input.first() == Some(&b',') {
            if !input.get(1).is_some_and(|&b| is_ws(b)) {
                return None;
            }
            input = &input[2..];
            after_comma = false;
        }
        units.set(unit, value)?;
        if let Some(f) = frac {
            units.set_fraction(f)?;
            break;
        }
        let (next, rest) = u64_prefix(skip_ws(input)).ok()?;
        match next {
            None => break,
            Some(v) => {
                value = v;
                input = rest;
                after_comma = true;
            }
        }
    }
    if !after_comma {
        return None;
    }
    if input.first().is_some_and(|&b| is_ws(b)) {
        input = skip_ws(&input[1..]);
        if let Some(tail) = input.strip_prefix(b"ago") {
            if had_prefix_sign {
                return None;
            }
            units.negative = true;
            input = tail;
        }
    }
    input.is_empty().then(|| units.span())?
}

fn iso(input: &[u8]) -> Option<Span> {
    let mut units = Units::default();
    let mut input = match input.first() {
        Some(b'+') => &input[1..],
        Some(b'-') => {
            units.negative = true;
            &input[1..]
        }
        _ => input,
    };
    input = input
        .strip_prefix(b"P")
        .or_else(|| input.strip_prefix(b"p"))?;
    loop {
        let (value, rest) = u64_prefix(input).ok()?;
        let Some(value) = value else { break };
        let unit = match rest.first()? {
            b'Y' | b'y' => 9,
            b'M' | b'm' => 8,
            b'W' | b'w' => 7,
            b'D' | b'd' => 6,
            _ => return None,
        };
        units.set(unit, value)?;
        input = &rest[1..];
    }
    if let Some(tail) = input
        .strip_prefix(b"T")
        .or_else(|| input.strip_prefix(b"t"))
    {
        input = tail;
        loop {
            let (value, rest) = u64_prefix(input).ok()?;
            let Some(value) = value else { break };
            let (frac, rest) = fraction(rest).ok()?;
            let unit = match rest.first()? {
                b'H' | b'h' => HOUR,
                b'M' | b'm' => MINUTE,
                b'S' | b's' => SECOND,
                _ => return None,
            };
            units.set(unit, value)?;
            input = &rest[1..];
            if let Some(f) = frac {
                units.set_fraction(f)?;
                break;
            }
        }
        if units.min.is_none_or(|m| m > HOUR) {
            return None;
        }
    }
    input.is_empty().then(|| units.span())?
}

fn parse_span(input: &[u8]) -> Option<Span> {
    let first = match input {
        [b'+' | b'-', next, ..] => *next,
        [b'+' | b'-'] | [] => return None,
        [first, ..] => *first,
    };
    if first == b'P' || first == b'p' {
        iso(input)
    } else {
        friendly(input)
    }
}

#[derive(Clone, Copy)]
struct Civil {
    year: i64,
    month: i64,
    day: i64,
    secs: i64,
    nanos: i64,
}

enum Offset {
    Zulu,
    Numeric(i64),
}

struct Temporal {
    civil: Civil,
    has_time: bool,
    offset: Option<Offset>,
}

fn digits(input: &[u8], n: usize, lo: i64, hi: i64) -> Option<(i64, &[u8])> {
    if input.len() < n || !input[..n].iter().all(u8::is_ascii_digit) {
        return None;
    }
    let v = input[..n]
        .iter()
        .fold(0i64, |acc, &b| acc * 10 + i64::from(b - b'0'));
    (lo..=hi).contains(&v).then_some((v, &input[n..]))
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn date_separator(input: &[u8], extended: bool) -> Option<&[u8]> {
    if extended {
        input.strip_prefix(b"-")
    } else {
        (!input.starts_with(b"-")).then_some(input)
    }
}

fn date_spec(input: &[u8]) -> Option<(Civil, &[u8])> {
    let (year, input) = match input.first() {
        Some(&s @ (b'+' | b'-')) => {
            let (y, rest) = digits(&input[1..], 6, 0, 9999)?;
            if y == 0 && s == b'-' {
                return None;
            }
            (if s == b'-' { -y } else { y }, rest)
        }
        _ => digits(input, 4, 0, 9999)?,
    };
    let extended = input.starts_with(b"-");
    let (month, input) = digits(date_separator(input, extended)?, 2, 1, 12)?;
    let (day, input) = digits(date_separator(input, extended)?, 2, 1, 31)?;
    (day <= days_in_month(year, month)).then_some((
        Civil {
            year,
            month,
            day,
            secs: 0,
            nanos: 0,
        },
        input,
    ))
}

fn time_separator(input: &[u8], extended: bool) -> (bool, &[u8]) {
    if extended {
        match input.strip_prefix(b":") {
            Some(tail) => (true, tail),
            None => (false, input),
        }
    } else {
        (
            input.len() >= 2 && input[..2].iter().all(u8::is_ascii_digit),
            input,
        )
    }
}

fn time_spec(input: &[u8]) -> Option<(i64, i64, &[u8])> {
    let (hour, input) = digits(input, 2, 0, 23)?;
    let extended = input.starts_with(b":");
    let (more, input) = time_separator(input, extended);
    if !more {
        return Some((hour * 3600, 0, input));
    }
    let (minute, input) = digits(input, 2, 0, 59)?;
    let (more, input) = time_separator(input, extended);
    if !more {
        return Some((hour * 3600 + minute * 60, 0, input));
    }
    let (second, input) = digits(input, 2, 0, 60)?;
    let (frac, input) = fraction(input).ok()?;
    Some((
        hour * 3600 + minute * 60 + second.min(59),
        i64::from(frac.unwrap_or(0)),
        input,
    ))
}

fn offset(input: &[u8], zulu: bool, subminute: bool) -> Option<(Offset, &[u8])> {
    if let [b'Z' | b'z', rest @ ..] = input {
        return zulu.then_some((Offset::Zulu, rest));
    }
    let negative = match input.first()? {
        b'+' => false,
        b'-' => true,
        _ => return None,
    };
    let (hours, input) = digits(&input[1..], 2, 0, 25)?;
    let extended = input.starts_with(b":");
    let mut secs = hours * 3600;
    let (more, mut input) = time_separator(input, extended);
    if more {
        let (minutes, rest) = digits(input, 2, 0, 59)?;
        secs += minutes * 60;
        input = rest;
        if subminute {
            let (more, rest) = time_separator(input, extended);
            input = rest;
            if more {
                let (seconds, rest) = digits(input, 2, 0, 59)?;
                let (frac, rest) = fraction(rest).ok()?;
                secs += seconds + i64::from(frac.is_some_and(|f| f >= 500_000_000));
                input = rest;
            }
        } else if input.starts_with(b":") {
            return None;
        }
    }
    Some((Offset::Numeric(if negative { -secs } else { secs }), input))
}

fn tz_name(input: &[u8]) -> Option<&[u8]> {
    if !matches!(input.first()?, b'_' | b'.' | b'A'..=b'Z' | b'a'..=b'z') {
        return None;
    }
    let len = 1 + input[1..]
        .iter()
        .take_while(
            |b| matches!(b, b'_' | b'.' | b'+' | b'-' | b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z'),
        )
        .count();
    Some(&input[len..])
}

fn annotation_value(input: &[u8]) -> Option<&[u8]> {
    let len = input
        .iter()
        .take_while(|b| b.is_ascii_alphanumeric())
        .count();
    (len > 0).then(|| &input[len..])
}

fn annotations(input: &[u8]) -> Option<&[u8]> {
    let mut input = input;
    if let Some(tail) = input.strip_prefix(b"[") {
        let tail = tail.strip_prefix(b"!").unwrap_or(tail);
        if tail.starts_with(b"+") || tail.starts_with(b"-") {
            let (_, rest) = offset(tail, false, false)?;
            input = rest.strip_prefix(b"]")?;
        } else {
            let mut rest = tz_name(tail)?;
            if !rest.starts_with(b"=") {
                while let Some(t) = rest.strip_prefix(b"/") {
                    rest = tz_name(t)?;
                }
                input = rest.strip_prefix(b"]")?;
            }
        }
    }
    while let Some(tail) = input.strip_prefix(b"[") {
        let (critical, tail) = match tail.strip_prefix(b"!") {
            Some(t) => (true, t),
            None => (false, tail),
        };
        if !matches!(tail.first()?, b'_' | b'a'..=b'z') {
            return None;
        }
        let key = 1 + tail[1..]
            .iter()
            .take_while(|b| matches!(b, b'_' | b'-' | b'0'..=b'9' | b'a'..=b'z'))
            .count();
        let mut rest = annotation_value(tail[key..].strip_prefix(b"=")?)?;
        while let Some(t) = rest.strip_prefix(b"-") {
            rest = annotation_value(t)?;
        }
        input = rest.strip_prefix(b"]")?;
        if critical {
            return None;
        }
    }
    Some(input)
}

fn temporal(input: &[u8]) -> Option<Temporal> {
    let (mut civil, input) = date_spec(input)?;
    let (has_time, off, input) = match input.first() {
        Some(b' ' | b'T' | b't') => {
            let (secs, nanos, rest) = time_spec(&input[1..])?;
            civil.secs = secs;
            civil.nanos = nanos;
            match rest.first() {
                Some(b'Z' | b'z' | b'+' | b'-') => {
                    let (o, rest) = offset(rest, true, true)?;
                    (true, Some(o), rest)
                }
                _ => (true, None, rest),
            }
        }
        _ => (false, None, input),
    };
    let rest = if input.first() == Some(&b'[') {
        annotations(input)?
    } else {
        input
    };
    rest.is_empty().then_some(Temporal {
        civil,
        has_time,
        offset: off,
    })
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

fn naive_secs(c: &Civil) -> i64 {
    days_from_civil(c.year, c.month, c.day) * 86_400 + c.secs
}

#[cfg(all(
    unix,
    any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly"
    )
))]
mod local {
    use std::ffi::{c_char, c_int, c_long};

    #[repr(C)]
    struct Tm {
        sec: c_int,
        min: c_int,
        hour: c_int,
        mday: c_int,
        mon: c_int,
        year: c_int,
        wday: c_int,
        yday: c_int,
        isdst: c_int,
        gmtoff: c_long,
        zone: *const c_char,
    }

    unsafe extern "C" {
        fn tzset();
        fn localtime_r(t: *const c_long, out: *mut Tm) -> *mut Tm;
    }

    pub(super) fn offset(ts: i64) -> i64 {
        let mut tm = Tm {
            sec: 0,
            min: 0,
            hour: 0,
            mday: 0,
            mon: 0,
            year: 0,
            wday: 0,
            yday: 0,
            isdst: 0,
            gmtoff: 0,
            zone: std::ptr::null(),
        };
        let t = ts as c_long;
        let ok = unsafe {
            tzset();
            !localtime_r(&raw const t, &raw mut tm).is_null()
        };
        if ok { tm.gmtoff as i64 } else { 0 }
    }
}

#[cfg(not(all(
    unix,
    any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly"
    )
)))]
mod local {
    pub(super) fn offset(_ts: i64) -> i64 {
        0
    }
}

fn local_to_ts(c: &Civil, later: bool) -> i64 {
    let naive = naive_secs(c);
    let before = local::offset(naive - 86_400);
    let after = local::offset(naive + 86_400);
    if before == after {
        return naive - before;
    }
    let first = naive - before;
    let second = naive - after;
    let first_ok = local::offset(first) == before;
    let second_ok = local::offset(second) == after;
    if second_ok && (later || !first_ok) {
        second
    } else {
        first
    }
}

fn to_system(secs: i64, nanos: i64) -> Option<SystemTime> {
    if !(TS_MIN..=TS_MAX).contains(&secs) {
        return None;
    }
    let total = i128::from(secs) * NS + i128::from(nanos);
    let d = Duration::new(
        u64::try_from(total.unsigned_abs() / 1_000_000_000).ok()?,
        (total.unsigned_abs() % 1_000_000_000) as u32,
    );
    if total >= 0 {
        UNIX_EPOCH.checked_add(d)
    } else {
        UNIX_EPOCH.checked_sub(d)
    }
}

fn split_nanos(total: i128) -> Option<(i64, i64)> {
    Some((
        i64::try_from(total.div_euclid(NS)).ok()?,
        total.rem_euclid(NS) as i64,
    ))
}

fn now_minus(span: &Span, now: SystemTime) -> Option<SystemTime> {
    let since = match now.duration_since(UNIX_EPOCH) {
        Ok(d) => i128::try_from(d.as_nanos()).ok()?,
        Err(e) => -i128::try_from(e.duration().as_nanos()).ok()?,
    };
    if span.months == 0 && span.days == 0 {
        let (secs, nanos) = split_nanos(since - span.nanos)?;
        return to_system(secs, nanos);
    }
    let (now_secs, now_nanos) = split_nanos(since)?;
    let local = now_secs + local::offset(now_secs);
    let (y, m, d) = civil_from_days(local.div_euclid(86_400));
    let total_months = y * 12 + (m - 1) - span.months;
    let year = total_months.div_euclid(12);
    let month = total_months.rem_euclid(12) + 1;
    if !(-9999..=9999).contains(&year) {
        return None;
    }
    let day = d.min(days_in_month(year, month));
    let (year, month, day) = civil_from_days(days_from_civil(year, month, day) - span.days);
    if !(-9999..=9999).contains(&year) {
        return None;
    }
    let civil = Civil {
        year,
        month,
        day,
        secs: local.rem_euclid(86_400),
        nanos: now_nanos,
    };
    let base = local_to_ts(&civil, false);
    let (secs, nanos) = split_nanos(i128::from(base) * NS + i128::from(now_nanos) - span.nanos)?;
    to_system(secs, nanos)
}

pub(super) fn parse(text: &str, now: SystemTime) -> Option<SystemTime> {
    let bytes = text.as_bytes();
    if let Some(span) = parse_span(bytes) {
        return now_minus(&span, now);
    }
    if let Some(t) = temporal(bytes) {
        match (&t.offset, t.has_time) {
            (Some(Offset::Zulu), true) => {
                return to_system(naive_secs(&t.civil), t.civil.nanos);
            }
            (Some(Offset::Numeric(o)), true) => {
                return to_system(naive_secs(&t.civil) - o, t.civil.nanos);
            }
            (Some(Offset::Zulu), false) => {}
            _ => return to_system(local_to_ts(&t.civil, true), t.civil.nanos),
        }
    }
    let secs: u64 = text.strip_prefix('@')?.parse().ok()?;
    UNIX_EPOCH.checked_add(Duration::from_secs(secs))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(t: SystemTime) -> i64 {
        match t.duration_since(UNIX_EPOCH) {
            Ok(d) => d.as_secs() as i64,
            Err(e) => -(e.duration().as_secs() as i64),
        }
    }

    #[test]
    fn spans_follow_jiff_grammar() {
        let now = UNIX_EPOCH + Duration::from_secs(1_707_723_412);
        let base = 1_707_723_412;
        assert_eq!(secs(parse("1min", now).unwrap()), base - 60);
        assert_eq!(secs(parse("10h", now).unwrap()), base - 36_000);
        assert_eq!(secs(parse("35 mins", now).unwrap()), base - 2100);
        assert_eq!(secs(parse("1.5h", now).unwrap()), base - 5400);
        assert_eq!(secs(parse("01:02:03", now).unwrap()), base - 3723);
        assert_eq!(secs(parse("PT1H30M", now).unwrap()), base - 5400);
        assert_eq!(secs(parse("1h ago", now).unwrap()), base + 3600);
        assert_eq!(secs(parse("-1h", now).unwrap()), base + 3600);
        assert_eq!(secs(parse("1h, 2m", now).unwrap()), base - 3720);
        assert_eq!(secs(parse("1h ", now).unwrap()), base - 3600);
        assert!(parse("-1h ago", now).is_none());
        assert!(parse("1m 1h", now).is_none());
        assert!(parse("1h,", now).is_none());
        assert!(parse("1d1.5d", now).is_none());
        assert!(parse("1.5d", now).is_none());
        assert!(parse("P", now).is_none());
        assert!(parse("PT", now).is_none());
        assert!(parse("P1DT", now).is_none());
        assert!(parse("1x", now).is_none());
        assert!(parse("", now).is_none());
        assert!(parse("-", now).is_none());
        assert!(parse("19999y", now).is_none());
        assert!(parse("1d", now).is_some());
        assert!(parse("2weeks", now).is_some());
        assert!(parse("P1Y2M3W4DT5H6M7.5S", now).is_some());
        assert!(parse("3 months", now).is_some());
        assert!(parse("1\u{b5}s", now).is_some());
        assert!(parse("1000000000000000000000ns", now).is_none());
    }

    #[test]
    fn timestamps_and_datetimes() {
        let now = SystemTime::now();
        assert_eq!(
            secs(parse("2024-02-12T07:36:52+00:00", now).unwrap()),
            1_707_723_412
        );
        assert_eq!(
            secs(parse("2024-02-12T07:36:52Z", now).unwrap()),
            1_707_723_412
        );
        assert_eq!(
            secs(parse("20240212T083652+0100", now).unwrap()),
            1_707_723_412
        );
        assert_eq!(
            secs(parse("2024-02-12 07:36:52.5-00:00[UTC][u-ca=iso8601]", now).unwrap()),
            1_707_723_412
        );
        assert_eq!(secs(parse("@1707723412", now).unwrap()), 1_707_723_412);
        assert!(parse("1707723412", now).is_none());
        assert!(parse("2024-02-12Z", now).is_none());
        assert!(parse("2024-02-30", now).is_none());
        assert!(parse("2024-0212", now).is_none());
        assert!(parse("2024-02-12T25", now).is_none());
        assert!(parse("2024-02-12T10:00:00+26", now).is_none());
        assert!(parse("2024-02-12[!u-ca=iso8601]", now).is_none());
        assert!(parse("2024-02-12[0x]", now).is_none());
        assert!(parse("-000000-01-01", now).is_none());
        let local = parse("2010-10-10 10:10:10", now).unwrap();
        let date_only = parse("2010-10-10", now).unwrap();
        assert_eq!(secs(local) - secs(date_only), 36_610);
        assert!(parse("+002010-10-10T10", now).is_some());
        assert!(parse("2010-10-10T10:10:60.123456789", now).is_some());
        assert!(parse("2010-10-10T10:10:10.1234567890", now).is_none());
    }

    #[test]
    fn calendar_helpers() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(days_from_civil(2000, 2, 29)), (2000, 2, 29));
        assert_eq!(days_in_month(1900, 2), 28);
        assert_eq!(days_in_month(2000, 2), 29);
        assert!(to_system(TS_MAX + 1, 0).is_none());
        assert!(to_system(-1, 0).is_some());
    }
}

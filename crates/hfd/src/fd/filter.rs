use std::sync::OnceLock;
use std::time::SystemTime;

use anyhow::anyhow;
use regex::Regex;

use super::bin;
use super::cli::FileType;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SizeFilter {
    Max(u64),
    Min(u64),
    Equals(u64),
}

impl SizeFilter {
    pub(super) fn from_string(s: &str) -> anyhow::Result<Self> {
        Self::parse_opt(s).ok_or_else(|| {
            anyhow!(
                "'{s}' is not a valid size constraint. See '{} --help'.",
                bin()
            )
        })
    }

    fn parse_opt(s: &str) -> Option<Self> {
        static PATTERN: OnceLock<Regex> = OnceLock::new();
        let captures = PATTERN
            .get_or_init(|| Regex::new(r"(?i)^([+-]?)(\d+)(b|[kmgt]i?b?)$").unwrap())
            .captures(s)?;
        let quantity = captures.get(2)?.as_str().parse::<u64>().ok()?;
        let unit = captures.get(3)?.as_str().to_lowercase();
        let multiplier: u64 = match unit.as_bytes() {
            [b'k', b'i', ..] => 1 << 10,
            [b'k', ..] => 1000,
            [b'm', b'i', ..] => 1 << 20,
            [b'm', ..] => 1_000_000,
            [b'g', b'i', ..] => 1 << 30,
            [b'g', ..] => 1_000_000_000,
            [b't', b'i', ..] => 1 << 40,
            [b't', ..] => 1_000_000_000_000,
            _ => 1,
        };
        let size = quantity.wrapping_mul(multiplier);
        match captures.get(1)?.as_str() {
            "+" => Some(SizeFilter::Min(size)),
            "-" => Some(SizeFilter::Max(size)),
            _ => Some(SizeFilter::Equals(size)),
        }
    }

    pub(super) fn is_within(self, size: u64) -> bool {
        match self {
            SizeFilter::Max(limit) => size <= limit,
            SizeFilter::Min(limit) => size >= limit,
            SizeFilter::Equals(limit) => size == limit,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum TimeFilter {
    Before(SystemTime),
    After(SystemTime),
}

impl TimeFilter {
    pub(super) fn before(s: &str) -> Option<TimeFilter> {
        super::when::parse(s, SystemTime::now()).map(TimeFilter::Before)
    }

    pub(super) fn after(s: &str) -> Option<TimeFilter> {
        super::when::parse(s, SystemTime::now()).map(TimeFilter::After)
    }

    pub(super) fn applies_to(&self, t: SystemTime) -> bool {
        match self {
            TimeFilter::Before(limit) => t < *limit,
            TimeFilter::After(limit) => t > *limit,
        }
    }
}

#[cfg(unix)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Check {
    Equal(u32),
    NotEq(u32),
    Ignore,
}

#[cfg(unix)]
impl Check {
    fn check(self, v: u32) -> bool {
        match self {
            Check::Equal(x) => v == x,
            Check::NotEq(x) => v != x,
            Check::Ignore => true,
        }
    }

    fn parse(
        s: Option<&str>,
        lookup: impl Fn(&str) -> anyhow::Result<u32>,
    ) -> anyhow::Result<Self> {
        let (s, equality) = match s {
            Some("") | None => return Ok(Check::Ignore),
            Some(s) => match s.strip_prefix('!') {
                Some(rest) => (rest, false),
                None => (s, true),
            },
        };
        lookup(s).map(|x| {
            if equality {
                Check::Equal(x)
            } else {
                Check::NotEq(x)
            }
        })
    }
}

#[cfg(unix)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OwnerFilter {
    uid: Check,
    gid: Check,
}

#[cfg(unix)]
mod ids {
    use std::ffi::{CString, c_char};

    #[repr(C)]
    struct Head {
        name: *mut c_char,
        passwd: *mut c_char,
        id: u32,
    }

    unsafe extern "C" {
        fn getpwnam(name: *const c_char) -> *const Head;
        fn getgrnam(name: *const c_char) -> *const Head;
    }

    fn lookup(name: &str, f: unsafe extern "C" fn(*const c_char) -> *const Head) -> Option<u32> {
        let name = CString::new(name).ok()?;
        unsafe { f(name.as_ptr()).as_ref().map(|h| h.id) }
    }

    pub(super) fn user(name: &str) -> Option<u32> {
        lookup(name, getpwnam)
    }

    pub(super) fn group(name: &str) -> Option<u32> {
        lookup(name, getgrnam)
    }
}

#[cfg(unix)]
impl OwnerFilter {
    const IGNORE: Self = OwnerFilter {
        uid: Check::Ignore,
        gid: Check::Ignore,
    };

    pub(super) fn from_string(input: &str) -> anyhow::Result<Self> {
        let mut it = input.split(':');
        let (fst, snd) = (it.next(), it.next());
        if it.next().is_some() {
            return Err(anyhow!(
                "more than one ':' present in owner string '{input}'. See '{} --help'.",
                bin()
            ));
        }
        let uid = Check::parse(fst, |s| {
            s.parse()
                .ok()
                .or_else(|| ids::user(s))
                .ok_or_else(|| anyhow!("'{s}' is not a recognized user name"))
        })?;
        let gid = Check::parse(snd, |s| {
            s.parse()
                .ok()
                .or_else(|| ids::group(s))
                .ok_or_else(|| anyhow!("'{s}' is not a recognized group name"))
        })?;
        Ok(OwnerFilter { uid, gid })
    }

    pub(super) fn filter_ignore(self) -> Option<Self> {
        (self != Self::IGNORE).then_some(self)
    }

    pub(super) fn matches(self, md: &std::fs::Metadata) -> bool {
        use std::os::unix::fs::MetadataExt;
        self.uid.check(md.uid()) && self.gid.check(md.gid())
    }
}

#[derive(Default, Clone, Copy)]
pub(super) struct FileTypes(u16);

impl FileTypes {
    pub(super) fn set(&mut self, t: FileType) {
        self.0 |= 1 << t as u16;
    }

    pub(super) fn has(self, t: FileType) -> bool {
        self.0 & (1 << t as u16) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_filters_parse_like_fd() {
        for (text, expected) in [
            ("+1b", SizeFilter::Min(1)),
            ("-1b", SizeFilter::Max(1)),
            ("+1k", SizeFilter::Min(1000)),
            ("+1kb", SizeFilter::Min(1000)),
            ("-100k", SizeFilter::Max(100_000)),
            ("+1KB", SizeFilter::Min(1000)),
            ("+1ki", SizeFilter::Min(1024)),
            ("+1KiB", SizeFilter::Min(1024)),
            ("-1Mi", SizeFilter::Max(1_048_576)),
            ("+1g", SizeFilter::Min(1_000_000_000)),
            ("+1GI", SizeFilter::Min(1_073_741_824)),
            ("+1t", SizeFilter::Min(1_000_000_000_000)),
            ("-1TIB", SizeFilter::Max(1_099_511_627_776)),
            ("10b", SizeFilter::Equals(10)),
        ] {
            assert_eq!(SizeFilter::from_string(text).unwrap(), expected, "{text}");
        }
        for bad in [
            "+g", "+18", "$10M", "badval", "9999", "+50a", "-10v", "+1Mv", "+1bib", "+1bb",
        ] {
            assert!(SizeFilter::from_string(bad).is_err(), "{bad}");
        }
        assert!(SizeFilter::Max(1000).is_within(1000));
        assert!(!SizeFilter::Max(1000).is_within(1001));
        assert!(SizeFilter::Min(1000).is_within(1000));
        assert!(SizeFilter::Equals(3).is_within(3));
    }

    #[cfg(unix)]
    #[test]
    fn owner_filters_parse_like_fd() {
        assert_eq!(OwnerFilter::from_string("").unwrap(), OwnerFilter::IGNORE);
        assert_eq!(OwnerFilter::from_string(":").unwrap(), OwnerFilter::IGNORE);
        assert_eq!(
            OwnerFilter::from_string("9:3").unwrap(),
            OwnerFilter {
                uid: Check::Equal(9),
                gid: Check::Equal(3)
            }
        );
        assert_eq!(
            OwnerFilter::from_string("!4:!3").unwrap(),
            OwnerFilter {
                uid: Check::NotEq(4),
                gid: Check::NotEq(3)
            }
        );
        assert_eq!(
            OwnerFilter::from_string("5:").unwrap(),
            OwnerFilter {
                uid: Check::Equal(5),
                gid: Check::Ignore
            }
        );
        assert!(OwnerFilter::from_string("3:5:").is_err());
        assert!(OwnerFilter::from_string("::").is_err());
        assert!(OwnerFilter::from_string("hfd-no-such-user-zz").is_err());
        assert!(OwnerFilter::from_string(":hfd-no-such-group-zz").is_err());
        assert!(OwnerFilter::from_string("root").is_ok());
        assert!(OwnerFilter::IGNORE.filter_ignore().is_none());
        assert!(
            OwnerFilter::from_string("0")
                .unwrap()
                .filter_ignore()
                .is_some()
        );
    }

    #[test]
    fn time_filters_compare() {
        let now = SystemTime::now();
        assert!(TimeFilter::after("1min").unwrap().applies_to(now));
        assert!(!TimeFilter::before("1min").unwrap().applies_to(now));
        assert!(TimeFilter::after("nonsense").is_none());
    }
}

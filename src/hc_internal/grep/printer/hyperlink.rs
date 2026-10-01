use std::cell::RefCell;
use std::io;
use std::path::Path;
use std::sync::Arc;

use memchr::memchr;

use super::termcolor::{HyperlinkSpec, WriteColor};
use super::util::DecimalFormatter;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HyperlinkConfig(Arc<HyperlinkConfigInner>);

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct HyperlinkConfigInner {
    env: HyperlinkEnvironment,
    format: HyperlinkFormat,
}

impl HyperlinkConfig {
    #[must_use]
    pub fn new(env: HyperlinkEnvironment, format: HyperlinkFormat) -> HyperlinkConfig {
        HyperlinkConfig(Arc::new(HyperlinkConfigInner { env, format }))
    }

    pub(crate) fn environment(&self) -> &HyperlinkEnvironment {
        &self.0.env
    }

    pub(crate) fn format(&self) -> &HyperlinkFormat {
        &self.0.format
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HyperlinkFormat {
    parts: Vec<Part>,
    is_line_dependent: bool,
}

impl HyperlinkFormat {
    #[must_use]
    pub fn empty() -> HyperlinkFormat {
        HyperlinkFormat::default()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    pub(crate) fn is_line_dependent(&self) -> bool {
        self.is_line_dependent
    }
}

impl std::str::FromStr for HyperlinkFormat {
    type Err = HyperlinkFormatError;

    fn from_str(s: &str) -> Result<HyperlinkFormat, HyperlinkFormatError> {
        enum State {
            Verbatim,
            VerbatimCloseVariable,
            OpenVariable,
            InVariable,
        }
        let err = |kind| HyperlinkFormatError { kind };
        let mut builder = FormatBuilder::new();
        let input = HyperlinkAlias::find(s).map_or(s, |alias| alias.format());
        let mut name = String::new();
        let mut state = State::Verbatim;
        for ch in input.chars() {
            state = match state {
                State::Verbatim => {
                    if ch == '{' {
                        State::OpenVariable
                    } else if ch == '}' {
                        State::VerbatimCloseVariable
                    } else {
                        builder.append_char(ch);
                        State::Verbatim
                    }
                }
                State::VerbatimCloseVariable => {
                    if ch == '}' {
                        builder.append_char('}');
                        State::Verbatim
                    } else {
                        return Err(err(HyperlinkFormatErrorKind::InvalidCloseVariable));
                    }
                }
                State::OpenVariable => {
                    if ch == '{' {
                        builder.append_char('{');
                        State::Verbatim
                    } else {
                        name.clear();
                        if ch == '}' {
                            builder.append_var(&name)?;
                            State::Verbatim
                        } else {
                            name.push(ch);
                            State::InVariable
                        }
                    }
                }
                State::InVariable => {
                    if ch == '}' {
                        builder.append_var(&name)?;
                        State::Verbatim
                    } else {
                        name.push(ch);
                        State::InVariable
                    }
                }
            };
        }
        match state {
            State::Verbatim => builder.build(),
            State::VerbatimCloseVariable => {
                Err(err(HyperlinkFormatErrorKind::InvalidCloseVariable))
            }
            State::OpenVariable | State::InVariable => {
                Err(err(HyperlinkFormatErrorKind::UnclosedVariable))
            }
        }
    }
}

impl std::fmt::Display for HyperlinkFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for part in &self.parts {
            part.fmt(f)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct HyperlinkAlias {
    name: &'static str,
    format: &'static str,
    display_priority: Option<i16>,
}

impl HyperlinkAlias {
    #[must_use]
    pub const fn name(&self) -> &str {
        self.name
    }

    #[must_use]
    pub const fn display_priority(&self) -> Option<i16> {
        self.display_priority
    }

    const fn format(&self) -> &'static str {
        self.format
    }

    fn find(name: &str) -> Option<&HyperlinkAlias> {
        HYPERLINK_PATTERN_ALIASES
            .binary_search_by_key(&name, |alias| alias.name())
            .map(|i| &HYPERLINK_PATTERN_ALIASES[i])
            .ok()
    }
}

const fn alias(name: &'static str, format: &'static str) -> HyperlinkAlias {
    HyperlinkAlias {
        name,
        format,
        display_priority: None,
    }
}

const fn prioritized_alias(
    priority: i16,
    name: &'static str,
    format: &'static str,
) -> HyperlinkAlias {
    HyperlinkAlias {
        name,
        format,
        display_priority: Some(priority),
    }
}

#[cfg(not(windows))]
const DEFAULT_FORMAT: &str = "file://{host}{path}";
#[cfg(windows)]
const DEFAULT_FORMAT: &str = "file://{path}";

const HYPERLINK_PATTERN_ALIASES: &[HyperlinkAlias] = &[
    alias("cursor", "cursor://file{path}:{line}:{column}"),
    prioritized_alias(0, "default", DEFAULT_FORMAT),
    alias("file", "file://{host}{path}"),
    alias("grep+", "grep+://{path}:{line}"),
    alias("kitty", "file://{host}{path}#{line}"),
    alias(
        "macvim",
        "mvim://open?url=file://{path}&line={line}&column={column}",
    ),
    prioritized_alias(1, "none", ""),
    alias(
        "textmate",
        "txmt://open?url=file://{path}&line={line}&column={column}",
    ),
    alias("vscode", "vscode://file{path}:{line}:{column}"),
    alias(
        "vscode-insiders",
        "vscode-insiders://file{path}:{line}:{column}",
    ),
    alias("vscodium", "vscodium://file{path}:{line}:{column}"),
];

#[must_use]
pub fn hyperlink_aliases() -> Vec<HyperlinkAlias> {
    HYPERLINK_PATTERN_ALIASES.to_vec()
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HyperlinkEnvironment {
    host: Option<String>,
    wsl_prefix: Option<String>,
}

impl HyperlinkEnvironment {
    #[must_use]
    pub fn new() -> HyperlinkEnvironment {
        HyperlinkEnvironment::default()
    }

    pub fn host(&mut self, host: Option<String>) -> &mut HyperlinkEnvironment {
        self.host = host;
        self
    }

    pub fn wsl_prefix(&mut self, wsl_prefix: Option<String>) -> &mut HyperlinkEnvironment {
        self.wsl_prefix = wsl_prefix;
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HyperlinkFormatError {
    kind: HyperlinkFormatErrorKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum HyperlinkFormatErrorKind {
    NoVariables,
    NoPathVariable,
    NoLineVariable,
    InvalidVariable(String),
    InvalidScheme,
    InvalidCloseVariable,
    UnclosedVariable,
}

impl std::error::Error for HyperlinkFormatError {}

impl std::fmt::Display for HyperlinkFormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind {
            HyperlinkFormatErrorKind::NoVariables => {
                let mut aliases = hyperlink_aliases();
                aliases.sort_by_key(|alias| alias.display_priority().unwrap_or(i16::MAX));
                let names: Vec<&str> = aliases.iter().map(HyperlinkAlias::name).collect();
                write!(
                    f,
                    "at least a {{path}} variable is required in a hyperlink format, or otherwise use a valid alias: {}",
                    names.join(", ")
                )
            }
            HyperlinkFormatErrorKind::NoPathVariable => {
                write!(f, "the {{path}} variable is required in a hyperlink format")
            }
            HyperlinkFormatErrorKind::NoLineVariable => write!(
                f,
                "the hyperlink format contains a {{column}} variable, but no {{line}} variable is present"
            ),
            HyperlinkFormatErrorKind::InvalidVariable(ref name) => write!(
                f,
                "invalid hyperlink format variable: '{name}', choose from: path, line, column, host, wslprefix"
            ),
            HyperlinkFormatErrorKind::InvalidScheme => write!(
                f,
                "the hyperlink format must start with a valid URL scheme, i.e., [0-9A-Za-z+-.]+:"
            ),
            HyperlinkFormatErrorKind::InvalidCloseVariable => write!(
                f,
                "unopened variable: found '}}' without a corresponding '{{' preceding it"
            ),
            HyperlinkFormatErrorKind::UnclosedVariable => write!(
                f,
                "unclosed variable: found '{{' without a corresponding '}}' following it"
            ),
        }
    }
}

#[derive(Debug)]
struct FormatBuilder {
    parts: Vec<Part>,
}

impl FormatBuilder {
    fn new() -> FormatBuilder {
        FormatBuilder { parts: vec![] }
    }

    fn append_slice(&mut self, text: &[u8]) -> &mut FormatBuilder {
        if let Some(Part::Text(contents)) = self.parts.last_mut() {
            contents.extend_from_slice(text);
        } else if !text.is_empty() {
            self.parts.push(Part::Text(text.to_vec()));
        }
        self
    }

    fn append_char(&mut self, ch: char) -> &mut FormatBuilder {
        self.append_slice(ch.encode_utf8(&mut [0; 4]).as_bytes())
    }

    fn append_var(&mut self, name: &str) -> Result<&mut FormatBuilder, HyperlinkFormatError> {
        let part = match name {
            "host" => Part::Host,
            "wslprefix" => Part::WslPrefix,
            "path" => Part::Path,
            "line" => Part::Line,
            "column" => Part::Column,
            unknown => {
                return Err(HyperlinkFormatError {
                    kind: HyperlinkFormatErrorKind::InvalidVariable(unknown.to_string()),
                });
            }
        };
        self.parts.push(part);
        Ok(self)
    }

    fn build(&self) -> Result<HyperlinkFormat, HyperlinkFormatError> {
        self.validate()?;
        Ok(HyperlinkFormat {
            parts: self.parts.clone(),
            is_line_dependent: self.parts.contains(&Part::Line),
        })
    }

    fn validate(&self) -> Result<(), HyperlinkFormatError> {
        let err = |kind| HyperlinkFormatError { kind };
        if self.parts.is_empty() {
            return Ok(());
        }
        if self.parts.iter().all(|p| matches!(*p, Part::Text(_))) {
            return Err(err(HyperlinkFormatErrorKind::NoVariables));
        }
        if !self.parts.contains(&Part::Path) {
            return Err(err(HyperlinkFormatErrorKind::NoPathVariable));
        }
        if self.parts.contains(&Part::Column) && !self.parts.contains(&Part::Line) {
            return Err(err(HyperlinkFormatErrorKind::NoLineVariable));
        }
        self.validate_scheme()
    }

    fn validate_scheme(&self) -> Result<(), HyperlinkFormatError> {
        let invalid = HyperlinkFormatError {
            kind: HyperlinkFormatErrorKind::InvalidScheme,
        };
        let Some(Part::Text(part)) = self.parts.first() else {
            return Err(invalid);
        };
        let Some(colon) = memchr(b':', part) else {
            return Err(invalid);
        };
        let scheme = &part[..colon];
        if scheme.is_empty()
            || !scheme
                .iter()
                .all(|&b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
        {
            return Err(invalid);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Part {
    Text(Vec<u8>),
    Host,
    WslPrefix,
    Path,
    Line,
    Column,
}

impl Part {
    fn interpolate_to(&self, env: &HyperlinkEnvironment, values: &Values<'_>, dest: &mut Vec<u8>) {
        match *self {
            Part::Text(ref text) => dest.extend_from_slice(text),
            Part::Host => dest.extend_from_slice(env.host.as_deref().unwrap_or("").as_bytes()),
            Part::WslPrefix => {
                dest.extend_from_slice(env.wsl_prefix.as_deref().unwrap_or("").as_bytes());
            }
            Part::Path => dest.extend_from_slice(&values.path.0),
            Part::Line => {
                dest.extend_from_slice(DecimalFormatter::new(values.line.unwrap_or(1)).as_bytes());
            }
            Part::Column => {
                dest.extend_from_slice(
                    DecimalFormatter::new(values.column.unwrap_or(1)).as_bytes(),
                );
            }
        }
    }
}

impl std::fmt::Display for Part {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Part::Text(text) => write!(f, "{}", String::from_utf8_lossy(text)),
            Part::Host => write!(f, "{{host}}"),
            Part::WslPrefix => write!(f, "{{wslprefix}}"),
            Part::Path => write!(f, "{{path}}"),
            Part::Line => write!(f, "{{line}}"),
            Part::Column => write!(f, "{{column}}"),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Values<'a> {
    path: &'a HyperlinkPath,
    line: Option<u64>,
    column: Option<u64>,
}

impl<'a> Values<'a> {
    pub(crate) fn new(path: &'a HyperlinkPath) -> Values<'a> {
        Values {
            path,
            line: None,
            column: None,
        }
    }

    pub(crate) fn line(mut self, line: Option<u64>) -> Values<'a> {
        self.line = line;
        self
    }

    pub(crate) fn column(mut self, column: Option<u64>) -> Values<'a> {
        self.column = column;
        self
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Interpolator {
    config: HyperlinkConfig,
    buf: RefCell<Vec<u8>>,
}

impl Interpolator {
    pub(crate) fn new(config: &HyperlinkConfig) -> Interpolator {
        Interpolator {
            config: config.clone(),
            buf: RefCell::new(vec![]),
        }
    }

    pub(crate) fn enabled<W: WriteColor>(&self, wtr: &W) -> bool {
        !self.config.format().is_empty() && wtr.supports_hyperlinks() && wtr.supports_color()
    }

    pub(crate) fn begin<W: WriteColor>(
        &self,
        values: &Values<'_>,
        mut wtr: W,
    ) -> io::Result<InterpolatorStatus> {
        if self.config.format().is_empty() || !wtr.supports_hyperlinks() || !wtr.supports_color() {
            return Ok(InterpolatorStatus::inactive());
        }
        let mut buf = self.buf.borrow_mut();
        buf.clear();
        for part in &self.config.format().parts {
            part.interpolate_to(self.config.environment(), values, &mut buf);
        }
        wtr.set_hyperlink(&HyperlinkSpec::open(&buf))?;
        Ok(InterpolatorStatus { active: true })
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct InterpolatorStatus {
    active: bool,
}

impl InterpolatorStatus {
    #[inline]
    pub(crate) fn inactive() -> InterpolatorStatus {
        InterpolatorStatus { active: false }
    }

    pub(crate) fn finish<W: WriteColor>(self, mut wtr: W) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }
        wtr.set_hyperlink(&HyperlinkSpec::close())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct HyperlinkPath(Vec<u8>);

impl HyperlinkPath {
    #[cfg(unix)]
    pub(crate) fn from_path(original_path: &Path) -> Option<HyperlinkPath> {
        use std::os::unix::ffi::OsStrExt;
        let path = original_path.canonicalize().ok()?;
        let bytes = path.as_os_str().as_bytes();
        if !bytes.starts_with(b"/") {
            return None;
        }
        Some(HyperlinkPath::encode(bytes))
    }

    #[cfg(windows)]
    pub(crate) fn from_path(original_path: &Path) -> Option<HyperlinkPath> {
        let path = std::path::absolute(original_path).ok()?;
        let mut string = path.to_str()?;
        if let Some(rest) = string.strip_prefix(r"\\?\") {
            string = rest;
            if string.starts_with(r"UNC\") {
                string = &string[3..];
            }
        } else if string.starts_with(r"\\") || string.starts_with("//") {
            string = &string[1..];
        }
        let with_slash = format!("/{string}");
        Some(HyperlinkPath::encode(with_slash.as_bytes()))
    }

    #[cfg(not(any(windows, unix)))]
    pub(crate) fn from_path(_original_path: &Path) -> Option<HyperlinkPath> {
        None
    }

    fn encode(input: &[u8]) -> HyperlinkPath {
        const HEX: &[u8] = b"0123456789ABCDEF";
        let mut result = Vec::with_capacity(input.len());
        for &byte in input {
            match byte {
                b'0'..=b'9'
                | b'A'..=b'Z'
                | b'a'..=b'z'
                | b'/'
                | b':'
                | b'-'
                | b'.'
                | b'_'
                | b'~'
                | 128.. => result.push(byte),
                b'\\' if cfg!(windows) => result.push(b'/'),
                _ => {
                    result.push(b'%');
                    result.push(HEX[usize::from(byte >> 4)]);
                    result.push(HEX[usize::from(byte & 0xF)]);
                }
            }
        }
        HyperlinkPath(result)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn build_format() {
        let format = FormatBuilder::new()
            .append_slice(b"foo://")
            .append_slice(b"bar-")
            .append_slice(b"baz")
            .append_var("path")
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(format.to_string(), "foo://bar-baz{path}");
        assert_eq!(format.parts[0], Part::Text(b"foo://bar-baz".to_vec()));
        assert!(!format.is_empty());
    }

    #[test]
    fn build_empty_format() {
        let format = FormatBuilder::new().build().unwrap();
        assert!(format.is_empty());
        assert_eq!(format, HyperlinkFormat::empty());
        assert_eq!(format, HyperlinkFormat::default());
    }

    #[test]
    fn handle_alias() {
        assert!(HyperlinkFormat::from_str("file").is_ok());
        assert!(HyperlinkFormat::from_str("none").is_ok());
        assert!(HyperlinkFormat::from_str("none").unwrap().is_empty());
    }

    #[test]
    fn parse_format() {
        let format = HyperlinkFormat::from_str("foo://{host}/bar/{path}:{line}:{column}").unwrap();
        assert_eq!(
            format.to_string(),
            "foo://{host}/bar/{path}:{line}:{column}"
        );
        assert_eq!(format.parts.len(), 8);
        assert!(format.parts.contains(&Part::Path));
        assert!(format.parts.contains(&Part::Line));
        assert!(format.parts.contains(&Part::Column));
    }

    #[test]
    fn parse_valid() {
        assert!(HyperlinkFormat::from_str("").unwrap().is_empty());
        assert_eq!(
            HyperlinkFormat::from_str("foo://{path}")
                .unwrap()
                .to_string(),
            "foo://{path}"
        );
        assert_eq!(
            HyperlinkFormat::from_str("foo://{path}/bar")
                .unwrap()
                .to_string(),
            "foo://{path}/bar"
        );
        for s in [
            "f://{path}",
            "f:{path}",
            "f-+.:{path}",
            "f42:{path}",
            "42:{path}",
            "+:{path}",
            "F42:{path}",
            "F42://foo{{bar}}{path}",
        ] {
            HyperlinkFormat::from_str(s).unwrap();
        }
    }

    #[test]
    fn parse_invalid() {
        use super::HyperlinkFormatErrorKind::*;
        let err = |kind| HyperlinkFormatError { kind };
        let cases = [
            ("foo://bar", NoVariables),
            ("foo://{line}", NoPathVariable),
            ("foo://{path", UnclosedVariable),
            ("foo://{path}:{column}", NoLineVariable),
            ("{path}", InvalidScheme),
            (":{path}", InvalidScheme),
            ("f*:{path}", InvalidScheme),
            ("foo://{bar}", InvalidVariable("bar".to_string())),
            ("foo://{}}bar}", InvalidVariable(String::new())),
            ("foo://{b}}ar}", InvalidVariable("b".to_string())),
            ("foo://{bar}}}", InvalidVariable("bar".to_string())),
            ("foo://{{bar}", InvalidCloseVariable),
            ("foo://{{{bar}", InvalidVariable("bar".to_string())),
            ("foo://{b{{ar}", InvalidVariable("b{{ar".to_string())),
            ("foo://{bar{{}", InvalidVariable("bar{{".to_string())),
        ];
        for (input, kind) in cases {
            assert_eq!(
                HyperlinkFormat::from_str(input).unwrap_err(),
                err(kind),
                "{input}"
            );
        }
    }

    #[test]
    #[cfg(windows)]
    fn convert_to_hyperlink_path() {
        let convert =
            |path| String::from_utf8(HyperlinkPath::from_path(Path::new(path)).unwrap().0).unwrap();
        assert_eq!(convert(r"C:\dir\file.txt"), "/C:/dir/file.txt");
        assert_eq!(
            convert(r"C:\foo\bar\..\other\baz.txt"),
            "/C:/foo/other/baz.txt"
        );
        assert_eq!(convert(r"\\server\dir\file.txt"), "//server/dir/file.txt");
        assert_eq!(
            convert(r"\\server\dir\foo\..\other\file.txt"),
            "//server/dir/other/file.txt"
        );
        assert_eq!(convert(r"\\?\C:\dir\file.txt"), "/C:/dir/file.txt");
        assert_eq!(
            convert(r"\\?\UNC\server\dir\file.txt"),
            "//server/dir/file.txt"
        );
    }

    #[test]
    fn aliases_are_sorted() {
        let aliases = hyperlink_aliases();
        let mut prev = aliases.first().expect("aliases should be non-empty").name();
        for alias in aliases.iter().skip(1) {
            let name = alias.name();
            assert!(
                name > prev,
                "'{prev}' should come before '{name}' in HYPERLINK_PATTERN_ALIASES"
            );
            prev = name;
        }
    }

    #[test]
    fn alias_names_are_reasonable() {
        for alias in hyperlink_aliases() {
            assert!(
                alias
                    .name()
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '+' || c == '-' || c == '.')
            );
        }
    }

    #[test]
    fn aliases_are_valid_formats() {
        for alias in hyperlink_aliases() {
            let (name, format) = (alias.name(), alias.format());
            assert!(
                format.parse::<HyperlinkFormat>().is_ok(),
                "invalid hyperlink alias '{name}': {format}"
            );
        }
    }

    #[test]
    fn encode_path() {
        assert_eq!(HyperlinkPath::encode(b"/a b/%c").0, b"/a%20b/%25c".to_vec());
    }
}

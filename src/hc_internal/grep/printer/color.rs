use std::fmt;
use std::str::FromStr;

use super::termcolor::{Color, ColorSpec, ParseColorError};

#[must_use]
pub fn default_color_specs() -> Vec<UserColorSpec> {
    vec![
        #[cfg(not(windows))]
        "path:fg:magenta".parse().unwrap(),
        #[cfg(windows)]
        "path:fg:cyan".parse().unwrap(),
        "line:fg:green".parse().unwrap(),
        "match:fg:red".parse().unwrap(),
        "match:style:bold".parse().unwrap(),
    ]
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ColorError {
    UnrecognizedOutType(String),
    UnrecognizedSpecType(String),
    UnrecognizedColor(String, String),
    UnrecognizedStyle(String),
    InvalidFormat(String),
}

impl std::error::Error for ColorError {}

impl ColorError {
    fn from_parse_error(err: &ParseColorError) -> ColorError {
        ColorError::UnrecognizedColor(err.invalid().to_string(), err.to_string())
    }
}

impl fmt::Display for ColorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            ColorError::UnrecognizedOutType(ref name) => write!(
                f,
                "unrecognized output type '{name}'. Choose from: path, line, column, match, highlight."
            ),
            ColorError::UnrecognizedSpecType(ref name) => write!(
                f,
                "unrecognized spec type '{name}'. Choose from: fg, bg, style, none."
            ),
            ColorError::UnrecognizedColor(_, ref msg) => write!(f, "{msg}"),
            ColorError::UnrecognizedStyle(ref name) => write!(
                f,
                "unrecognized style attribute '{name}'. Choose from: nobold, bold, nointense, intense, nounderline, underline, noitalic, italic."
            ),
            ColorError::InvalidFormat(ref original) => write!(
                f,
                "invalid color spec format: '{original}'. Valid format is '(path|line|column|match|highlight):(fg|bg|style):(value)'."
            ),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ColorSpecs {
    path: ColorSpec,
    line: ColorSpec,
    column: ColorSpec,
    matched: ColorSpec,
    highlight: ColorSpec,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserColorSpec {
    ty: OutType,
    value: SpecValue,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum SpecValue {
    None,
    Fg(Color),
    Bg(Color),
    Style(Style),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum OutType {
    Path,
    Line,
    Column,
    Match,
    Highlight,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum SpecType {
    Fg,
    Bg,
    Style,
    None,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Style {
    Bold,
    NoBold,
    Intense,
    NoIntense,
    Underline,
    NoUnderline,
    Italic,
    NoItalic,
}

impl ColorSpecs {
    #[must_use]
    pub fn new(specs: &[UserColorSpec]) -> ColorSpecs {
        let mut merged = ColorSpecs::default();
        for spec in specs {
            let target = match spec.ty {
                OutType::Path => &mut merged.path,
                OutType::Line => &mut merged.line,
                OutType::Column => &mut merged.column,
                OutType::Match => &mut merged.matched,
                OutType::Highlight => &mut merged.highlight,
            };
            spec.value.merge_into(target);
        }
        merged
    }

    #[cfg(test)]
    #[must_use]
    pub fn default_with_color() -> ColorSpecs {
        ColorSpecs::new(&default_color_specs())
    }

    #[must_use]
    pub fn path(&self) -> &ColorSpec {
        &self.path
    }

    #[must_use]
    pub fn line(&self) -> &ColorSpec {
        &self.line
    }

    #[must_use]
    pub fn column(&self) -> &ColorSpec {
        &self.column
    }

    #[must_use]
    pub fn matched(&self) -> &ColorSpec {
        &self.matched
    }

    #[must_use]
    pub fn highlight(&self) -> &ColorSpec {
        &self.highlight
    }
}

impl SpecValue {
    fn merge_into(&self, cspec: &mut ColorSpec) {
        match *self {
            SpecValue::None => cspec.clear(),
            SpecValue::Fg(color) => {
                cspec.set_fg(Some(color));
            }
            SpecValue::Bg(color) => {
                cspec.set_bg(Some(color));
            }
            SpecValue::Style(ref style) => {
                match *style {
                    Style::Bold => cspec.set_bold(true),
                    Style::NoBold => cspec.set_bold(false),
                    Style::Intense => cspec.set_intense(true),
                    Style::NoIntense => cspec.set_intense(false),
                    Style::Underline => cspec.set_underline(true),
                    Style::NoUnderline => cspec.set_underline(false),
                    Style::Italic => cspec.set_italic(true),
                    Style::NoItalic => cspec.set_italic(false),
                };
            }
        }
    }
}

impl FromStr for UserColorSpec {
    type Err = ColorError;

    fn from_str(s: &str) -> Result<UserColorSpec, ColorError> {
        let pieces: Vec<&str> = s.split(':').collect();
        if pieces.len() <= 1 || pieces.len() > 3 {
            return Err(ColorError::InvalidFormat(s.to_string()));
        }
        let otype: OutType = pieces[0].parse()?;
        let value = match pieces[1].parse()? {
            SpecType::None => SpecValue::None,
            spec_type => {
                if pieces.len() < 3 {
                    return Err(ColorError::InvalidFormat(s.to_string()));
                }
                match spec_type {
                    SpecType::Style => SpecValue::Style(pieces[2].parse()?),
                    SpecType::Fg => SpecValue::Fg(
                        pieces[2]
                            .parse()
                            .map_err(|e| ColorError::from_parse_error(&e))?,
                    ),
                    _ => SpecValue::Bg(
                        pieces[2]
                            .parse()
                            .map_err(|e| ColorError::from_parse_error(&e))?,
                    ),
                }
            }
        };
        Ok(UserColorSpec { ty: otype, value })
    }
}

impl FromStr for OutType {
    type Err = ColorError;

    fn from_str(s: &str) -> Result<OutType, ColorError> {
        match &*s.to_lowercase() {
            "path" => Ok(OutType::Path),
            "line" => Ok(OutType::Line),
            "column" => Ok(OutType::Column),
            "match" => Ok(OutType::Match),
            "highlight" => Ok(OutType::Highlight),
            _ => Err(ColorError::UnrecognizedOutType(s.to_string())),
        }
    }
}

impl FromStr for SpecType {
    type Err = ColorError;

    fn from_str(s: &str) -> Result<SpecType, ColorError> {
        match &*s.to_lowercase() {
            "fg" => Ok(SpecType::Fg),
            "bg" => Ok(SpecType::Bg),
            "style" => Ok(SpecType::Style),
            "none" => Ok(SpecType::None),
            _ => Err(ColorError::UnrecognizedSpecType(s.to_string())),
        }
    }
}

impl FromStr for Style {
    type Err = ColorError;

    fn from_str(s: &str) -> Result<Style, ColorError> {
        match &*s.to_lowercase() {
            "bold" => Ok(Style::Bold),
            "nobold" => Ok(Style::NoBold),
            "intense" => Ok(Style::Intense),
            "nointense" => Ok(Style::NoIntense),
            "underline" => Ok(Style::Underline),
            "nounderline" => Ok(Style::NoUnderline),
            "italic" => Ok(Style::Italic),
            "noitalic" => Ok(Style::NoItalic),
            _ => Err(ColorError::UnrecognizedStyle(s.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge() {
        let user_specs: &[UserColorSpec] = &[
            "match:fg:blue".parse().unwrap(),
            "match:none".parse().unwrap(),
            "match:style:bold".parse().unwrap(),
        ];
        let mut expect_matched = ColorSpec::new();
        expect_matched.set_bold(true);
        assert_eq!(
            ColorSpecs::new(user_specs),
            ColorSpecs {
                path: ColorSpec::default(),
                line: ColorSpec::default(),
                column: ColorSpec::default(),
                matched: expect_matched,
                highlight: ColorSpec::default(),
            }
        );
    }

    #[test]
    fn specs() {
        let spec: UserColorSpec = "path:fg:blue".parse().unwrap();
        assert_eq!(
            spec,
            UserColorSpec {
                ty: OutType::Path,
                value: SpecValue::Fg(Color::Blue),
            }
        );
        let spec: UserColorSpec = "path:bg:0x15".parse().unwrap();
        assert_eq!(
            spec,
            UserColorSpec {
                ty: OutType::Path,
                value: SpecValue::Bg(Color::Ansi256(0x15)),
            }
        );
        let spec: UserColorSpec = "line:style:bold".parse().unwrap();
        assert_eq!(
            spec,
            UserColorSpec {
                ty: OutType::Line,
                value: SpecValue::Style(Style::Bold),
            }
        );
        let spec: UserColorSpec = "match:none".parse().unwrap();
        assert_eq!(
            spec,
            UserColorSpec {
                ty: OutType::Match,
                value: SpecValue::None,
            }
        );
        let spec: UserColorSpec = "highlight:bg:255,64,0".parse().unwrap();
        assert_eq!(
            spec,
            UserColorSpec {
                ty: OutType::Highlight,
                value: SpecValue::Bg(Color::Rgb(255, 64, 0)),
            }
        );
    }

    #[test]
    fn spec_errors() {
        let err = "line:nonee".parse::<UserColorSpec>().unwrap_err();
        assert_eq!(err, ColorError::UnrecognizedSpecType("nonee".to_string()));
        let err = "".parse::<UserColorSpec>().unwrap_err();
        assert_eq!(err, ColorError::InvalidFormat(String::new()));
        let err = "foo".parse::<UserColorSpec>().unwrap_err();
        assert_eq!(err, ColorError::InvalidFormat("foo".to_string()));
        let err = "line:style:italicism".parse::<UserColorSpec>().unwrap_err();
        assert_eq!(err, ColorError::UnrecognizedStyle("italicism".to_string()));
        let err = "line:fg".parse::<UserColorSpec>().unwrap_err();
        assert_eq!(err, ColorError::InvalidFormat("line:fg".to_string()));
        let err = "line:fg:a:b".parse::<UserColorSpec>().unwrap_err();
        assert_eq!(err, ColorError::InvalidFormat("line:fg:a:b".to_string()));
        let err = "file:fg:red".parse::<UserColorSpec>().unwrap_err();
        assert_eq!(err, ColorError::UnrecognizedOutType("file".to_string()));
    }
}

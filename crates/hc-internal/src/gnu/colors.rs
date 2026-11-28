#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Colors {
    pub selected_match: String,
    pub context_match: String,
    pub filename: String,
    pub line_number: String,
    pub byte_offset: String,
    pub separator: String,
    pub selected_line: String,
    pub context_line: String,
    pub reverse: bool,
    pub no_erase: bool,
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            selected_match: "01;31".into(),
            context_match: "01;31".into(),
            filename: "35".into(),
            line_number: "32".into(),
            byte_offset: "32".into(),
            separator: "36".into(),
            selected_line: String::new(),
            context_line: String::new(),
            reverse: false,
            no_erase: false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    Default,
    Legacy,
    Colors,
}

impl Colors {
    #[must_use]
    pub fn from_env() -> (Self, Option<String>) {
        let colors = std::env::var("GREP_COLORS").ok();
        let color = std::env::var("GREP_COLOR").ok();
        Self::from_values(colors.as_deref(), color.as_deref())
    }

    #[must_use]
    pub fn from_values(
        grep_colors: Option<&str>,
        grep_color: Option<&str>,
    ) -> (Self, Option<String>) {
        let mut c = Self::default();
        let mut selected_src = Source::Default;
        let mut context_src = Source::Default;
        let legacy = grep_color
            .filter(|v| !v.is_empty() && v.bytes().all(|b| b == b';' || b.is_ascii_digit()));
        if let Some(v) = legacy {
            c.selected_match = v.to_string();
            c.context_match = v.to_string();
            selected_src = Source::Legacy;
            context_src = Source::Legacy;
        }
        if let Some(spec) = grep_colors.filter(|s| !s.is_empty()) {
            c.parse(spec, &mut selected_src, &mut context_src);
        }
        let warning = legacy
            .filter(|_| selected_src == Source::Legacy || context_src == Source::Legacy)
            .map(|v| format!("warning: GREP_COLOR='{v}' is deprecated; use GREP_COLORS='mt={v}'"));
        (c, warning)
    }

    fn parse(&mut self, spec: &str, selected_src: &mut Source, context_src: &mut Source) {
        let bytes = spec.as_bytes();
        let mut name_start = 0;
        let mut val_start: Option<usize> = None;
        let mut q = 0;
        loop {
            let ch = bytes.get(q).copied();
            match ch {
                None | Some(b':') => {
                    let name_end = val_start.map_or(q, |v| v - 1);
                    let name = &spec[name_start..name_end];
                    let val = val_start.map(|v| &spec[v..q]);
                    self.apply(name, val, selected_src, context_src);
                    if ch.is_none() {
                        return;
                    }
                    q += 1;
                    name_start = q;
                    val_start = None;
                }
                Some(b'=') => {
                    if q == name_start || val_start.is_some() {
                        return;
                    }
                    q += 1;
                    val_start = Some(q);
                }
                Some(b) => {
                    if val_start.is_some() && b != b';' && !b.is_ascii_digit() {
                        return;
                    }
                    q += 1;
                }
            }
        }
    }

    fn apply(
        &mut self,
        name: &str,
        val: Option<&str>,
        selected_src: &mut Source,
        context_src: &mut Source,
    ) {
        match (name, val) {
            ("mt", v) => {
                if let Some(v) = v {
                    self.selected_match = v.to_string();
                    *selected_src = Source::Colors;
                }
                self.context_match = self.selected_match.clone();
                *context_src = *selected_src;
            }
            ("ms", Some(v)) => {
                self.selected_match = v.to_string();
                *selected_src = Source::Colors;
            }
            ("mc", Some(v)) => {
                self.context_match = v.to_string();
                *context_src = Source::Colors;
            }
            ("fn", Some(v)) => self.filename = v.to_string(),
            ("ln", Some(v)) => self.line_number = v.to_string(),
            ("bn", Some(v)) => self.byte_offset = v.to_string(),
            ("se", Some(v)) => self.separator = v.to_string(),
            ("sl", Some(v)) => self.selected_line = v.to_string(),
            ("cx", Some(v)) => self.context_line = v.to_string(),
            ("rv", _) => self.reverse = true,
            ("ne", _) => self.no_erase = true,
            _ => {}
        }
    }

    pub fn start(&self, out: &mut Vec<u8>, cap: &str) {
        if !cap.is_empty() {
            out.extend_from_slice(b"\x1b[");
            out.extend_from_slice(cap.as_bytes());
            out.push(b'm');
            if !self.no_erase {
                out.extend_from_slice(b"\x1b[K");
            }
        }
    }

    pub fn end(&self, out: &mut Vec<u8>, cap: &str) {
        if !cap.is_empty() {
            out.extend_from_slice(b"\x1b[m");
            if !self.no_erase {
                out.extend_from_slice(b"\x1b[K");
            }
        }
    }
}

#[must_use]
pub fn term_allows_color(term: Option<&str>) -> bool {
    term.is_some_and(|t| !t.is_empty() && t != "dumb")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grep_colors_parsing() {
        let (c, w) = Colors::from_values(Some("mt=01;32:fn=:ne"), None);
        assert_eq!(c.selected_match, "01;32");
        assert_eq!(c.context_match, "01;32");
        assert_eq!(c.filename, "");
        assert!(c.no_erase);
        assert!(w.is_none());
        let (c, _) = Colors::from_values(Some("ms=1:bad=x:ln=33"), None);
        assert_eq!(c.selected_match, "1");
        assert_eq!(c.line_number, "32");
        let (c, w) = Colors::from_values(None, Some("1;32"));
        assert_eq!(c.selected_match, "1;32");
        assert_eq!(
            w.as_deref(),
            Some("warning: GREP_COLOR='1;32' is deprecated; use GREP_COLORS='mt=1;32'")
        );
        let (c, w) = Colors::from_values(Some("mt=7"), Some("1;32"));
        assert_eq!(c.context_match, "7");
        assert!(w.is_none());
        let (c, _) = Colors::from_values(None, Some("1x"));
        assert_eq!(c.selected_match, "01;31");
        let mut out = Vec::new();
        c.start(&mut out, "35");
        c.end(&mut out, "35");
        assert_eq!(out, b"\x1b[35m\x1b[K\x1b[m\x1b[K");
    }
}

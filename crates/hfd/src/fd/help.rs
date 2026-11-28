use std::env;
use std::io::{IsTerminal, Write};

fn display_width(text: &str) -> usize {
    let mut width = 0;
    let mut control = false;
    for ch in text.chars() {
        if ch.is_ascii_control() {
            control = true;
        } else if control && ch == 'm' {
            control = false;
            continue;
        }
        if !control {
            width += 1;
        }
    }
    width
}

fn words(line: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut in_ws = false;
    for (idx, ch) in line.char_indices() {
        let ws = ch == ' ';
        if in_ws && !ws {
            out.push(&line[start..idx]);
            start = idx;
        }
        in_ws = ws;
    }
    if start < line.len() {
        out.push(&line[start..]);
    }
    out
}

fn wrap(line: &str, hard_width: usize) -> String {
    wrap_words(words(line), hard_width)
}

fn wrap_words(mut words: Vec<&str>, hard_width: usize) -> String {
    let indentation = match words.first() {
        Some(w) if w.trim().is_empty() => *w,
        _ => "",
    };
    let mut first_word = true;
    let mut line_width = 0;
    let mut i = 0;
    while i < words.len() {
        let word = words[i];
        let trimmed = word.trim_end();
        let word_width = display_width(trimmed);
        let delta = word.len() - trimmed.len();
        if first_word && word_width > 0 {
            first_word = false;
        } else if hard_width < line_width + word_width {
            if i > 0 {
                words[i - 1] = words[i - 1].trim_end();
            }
            line_width = 0;
            words.insert(i, "\n");
            i += 1;
            words.insert(i, indentation);
            line_width += indentation.len();
            i += 1;
        }
        line_width += word_width + delta;
        i += 1;
    }
    words.concat()
}

fn column_to_byte(line: &str, column: usize) -> usize {
    let mut width = 0;
    let mut control = false;
    for (idx, ch) in line.char_indices() {
        if width == column && !control && !ch.is_ascii_control() {
            return idx;
        }
        if ch.is_ascii_control() {
            control = true;
        } else if control && ch == 'm' {
            control = false;
            continue;
        }
        if !control {
            width += 1;
        }
    }
    line.len()
}

fn strip(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut control = false;
    for ch in line.chars() {
        if ch.is_ascii_control() && ch != '\n' {
            control = true;
        } else if control {
            if ch == 'm' {
                control = false;
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn inline_help_column(plain: &str) -> Option<usize> {
    let lead = plain.chars().take_while(|&c| c == ' ').count();
    let gap = plain.get(lead..)?.find("  ")? + lead;
    let start = gap + plain[gap..].chars().take_while(|&c| c == ' ').count();
    (start < plain.len()).then_some(start)
}

fn continues(lines: &[&str], index: usize, col: usize) -> bool {
    let Some(next) = lines.get(index + 1).map(|l| strip(l)) else {
        return false;
    };
    let lead = next.chars().take_while(|&c| c == ' ').count();
    if next.is_empty() || lead < col {
        return false;
    }
    let after = lines.get(index + 2).map_or_else(String::new, |l| strip(l));
    !(next.trim().is_empty() && after.trim_start().starts_with("[default:"))
}

pub(super) fn layout(text: &str, width: usize) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out: Vec<String> = Vec::new();
    let mut in_args = false;
    let mut column: Option<usize> = None;
    for (index, &line) in lines.iter().enumerate() {
        let plain = strip(line);
        if !plain.starts_with(' ') && plain.ends_with(':') {
            in_args = plain == "Arguments:" || plain == "Options:";
            column = None;
            out.push(line.to_string());
            continue;
        }
        let col = if plain.trim().is_empty() {
            None
        } else if !plain.starts_with(' ') {
            (!plain.starts_with("Usage:")).then_some(0)
        } else if !in_args || plain.trim() == "Possible values:" {
            None
        } else if plain.starts_with("          - ") && !plain.starts_with("           ") {
            if display_width(line) > width && width > 12 {
                let split = column_to_byte(line, 12);
                let text = &line[split..];
                let colon = text.find(':').unwrap_or(text.len());
                let mut pieces = vec![&text[..colon]];
                pieces.extend(words(&text[colon..]));
                let wrapped = wrap_words(pieces, width - 12);
                out.push(format!(
                    "{}{}",
                    &line[..split],
                    wrapped.replace('\n', &format!("\n{}", " ".repeat(12)))
                ));
            } else {
                out.push(line.to_string());
            }
            continue;
        } else if (!plain.starts_with("   ") && plain.starts_with("  "))
            || plain.starts_with("      -")
        {
            let inline = inline_help_column(&plain);
            if inline.is_some() {
                column = inline;
            }
            inline
        } else {
            let lead = plain.chars().take_while(|&c| c == ' ').count();
            column.filter(|&c| lead >= c).or((lead >= 10).then_some(10))
        };
        match col {
            Some(c) if display_width(line) > width || continues(&lines, index, c) => {
                let split = column_to_byte(line, c);
                let wrapped = if continues(&lines, index, c) {
                    let mut text = wrap(&format!("{}\n", &line[split..]), width.saturating_sub(c));
                    text.pop();
                    text
                } else {
                    wrap(&line[split..], width.saturating_sub(c))
                };
                let indent = " ".repeat(c);
                out.push(format!(
                    "{}{}",
                    &line[..split],
                    wrapped.replace('\n', &format!("\n{indent}"))
                ));
            }
            _ => out.push(line.to_string()),
        }
    }
    out.join("\n")
}

fn use_color() -> bool {
    let var = |k: &str| env::var_os(k);
    if var("NO_COLOR").is_some_and(|v| !v.is_empty()) {
        return false;
    }
    if var("CLICOLOR_FORCE").is_some_and(|v| !v.is_empty()) {
        return true;
    }
    let clicolor = var("CLICOLOR").map(|v| v != "0");
    if clicolor == Some(false) {
        return false;
    }
    let term = if cfg!(windows) {
        var("TERM").is_none_or(|t| t != "dumb")
    } else {
        var("TERM").is_some_and(|t| t != "dumb")
    };
    std::io::stdout().is_terminal() && (term || clicolor == Some(true) || var("CI").is_some())
}

pub(super) fn print(err: &clap::Error, width: usize) {
    let rendered = err.render();
    let text = if use_color() {
        rendered.ansi().to_string()
    } else {
        rendered.to_string()
    };
    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(layout(&text, width).as_bytes());
    let _ = stdout.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_like_clap_textwrap() {
        assert_eq!(wrap("foo bar baz", 7), "foo bar\nbaz");
        assert_eq!(wrap("  ab cd ef", 6), "  ab\n  cd\n  ef");
        assert_eq!(wrap("  ab cd ef", 7), "  ab cd\n  ef");
        assert_eq!(wrap("abcdefgh ij", 4), "abcdefgh\nij");
        assert_eq!(words("foo   bar"), vec!["foo   ", "bar"]);
        assert_eq!(display_width("\x1b[1mab\x1b[0m"), 2);
        assert_eq!(column_to_byte("\x1b[1mab\x1b[0m cd", 3), 11);
        assert_eq!(strip("\x1b[1mab\x1b[0m"), "ab");
        assert_eq!(inline_help_column("  -a, --all  Help"), Some(13));
        assert_eq!(inline_help_column("  -a, --all"), None);
        assert_eq!(inline_help_column("      --long  Help"), Some(14));
        let text = "Options:\n  -t, --type <x>  one two three four five\n          six seven eight nine ten";
        assert_eq!(
            layout(text, 30),
            "Options:\n  -t, --type <x>  one two\n                  three four\n                  five\n          six seven eight nine\n          ten"
        );
    }
}

use std::fmt::Write;

use super::{Category, FlagDoc, rename, render_markup, rg_flags, short_var, version};

const TEMPLATE_SHORT: &str = include_str!("template.short.help");
const TEMPLATE_LONG: &str = include_str!("template.long.help");
const WIDTH: usize = 71;
const INDENT: &str = "        ";

pub(crate) fn help_short(prog: &str) -> String {
    short(prog, prog)
}

pub(crate) fn help_long(prog: &str) -> String {
    long(prog, prog)
}

pub(super) fn template(text: &str, prog: &str) -> String {
    text.replace("!!VERSION!!", &version::digits())
        .replace("!!PROG!!", prog)
}

pub(super) fn short(prog: &str, tool: &str) -> String {
    let rows: Vec<(Category, String, String)> = rg_flags()
        .map(|flag| (flag.cat, short_name(flag), rename(flag.doc, prog, tool)))
        .collect();
    let width = rows.iter().map(|row| row.1.len()).max().unwrap_or(0);
    Category::ALL
        .into_iter()
        .fold(template(TEMPLATE_SHORT, prog), |out, cat| {
            let body: Vec<String> = rows
                .iter()
                .filter(|row| row.0 == cat)
                .map(|(_, name, doc)| format!("  {name:width$}  {doc}"))
                .collect();
            out.replace(cat.placeholder(), &body.join("\n"))
        })
}

fn short_name(flag: &FlagDoc) -> String {
    let var = flag.var.map(short_var);
    if let Some(long) = flag.long {
        let short = flag.short.map_or_else(String::new, |s| format!("-{s}, "));
        let var = var.map_or_else(String::new, |v| format!("={v}"));
        format!("{short}--{long}{var}")
    } else {
        let short = flag.short.unwrap_or_default();
        let var = var.map_or_else(String::new, |v| format!(" {v}"));
        format!("-{short}{var}")
    }
}

pub(super) fn long(prog: &str, tool: &str) -> String {
    Category::ALL
        .into_iter()
        .fold(template(TEMPLATE_LONG, prog), |out, cat| {
            let body: Vec<String> = rg_flags()
                .filter(|flag| flag.cat == cat)
                .map(|flag| long_flag(flag, prog, tool))
                .collect();
            out.replace(cat.placeholder(), &body.join("\n\n"))
        })
}

fn long_flag(flag: &FlagDoc, prog: &str, tool: &str) -> String {
    let mut out = String::from("    ");
    if let Some(short) = flag.short {
        write!(out, "-{short}").unwrap();
        if let Some(var) = flag.var {
            write!(out, " {var}").unwrap();
        }
        if flag.long.is_some() {
            out.push_str(", ");
        }
    }
    if let Some(long) = flag.long {
        write!(out, "--{long}").unwrap();
        if let Some(var) = flag.var {
            write!(out, "={var}").unwrap();
        }
    }
    out.push('\n');

    let doc = rename(flag.long_doc.trim(), prog, tool);
    let doc = render_markup(&doc, "flag", |flag, out| {
        if let Some(short) = flag.short {
            write!(out, "-{short}/").unwrap();
        }
        write!(out, "--{}", flag.long.unwrap_or_default()).unwrap();
    });
    let doc = render_markup(&doc, "flag-negate", |flag, out| {
        write!(out, "--{}", flag.negated.unwrap_or_default()).unwrap();
    });
    let mut cleaned = remove_roff(&doc);
    if let Some(negated) = flag.negated
        && flag.is_switch()
    {
        write!(cleaned, "\n\nThis flag can be disabled with --{negated}.").unwrap();
    }
    for (i, paragraph) in cleaned.split("\n\n").enumerate() {
        if i > 0 {
            out.push_str("\n\n");
        }
        let text = if paragraph.lines().all(|line| line.starts_with("    ")) {
            indent(paragraph)
        } else {
            indent(&fill(&paragraph.replace('\n', " ")))
        };
        out.push_str(text.trim_end());
    }
    out
}

fn indent(text: &str) -> String {
    let lines: Vec<String> = text
        .lines()
        .map(|line| {
            if line.trim().is_empty() {
                line.to_string()
            } else {
                format!("{INDENT}{line}")
            }
        })
        .collect();
    lines.join("\n")
}

fn fill(text: &str) -> String {
    let mut words: Vec<(&str, &str)> = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let word_end = rest.find(' ').unwrap_or(rest.len());
        let space_end =
            word_end + rest[word_end..].len() - rest[word_end..].trim_start_matches(' ').len();
        words.push((&rest[..word_end], &rest[word_end..space_end]));
        rest = &rest[space_end..];
    }
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    let mut used = 0;
    let mut pending = "";
    for (i, (word, space)) in words.iter().enumerate() {
        let len = word.chars().count();
        if i > 0 && used + len > WIDTH {
            lines.push(std::mem::take(&mut line));
            used = 0;
        } else {
            line.push_str(pending);
        }
        line.push_str(word);
        used += len + space.len();
        pending = space;
    }
    lines.push(line);
    lines.join("\n")
}

fn remove_roff(doc: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    for line in doc.trim().lines() {
        assert!(!line.is_empty(), "roff should have no empty lines");
        if line.starts_with('.') {
            if line.starts_with(".IP ") {
                let label = line
                    .split(' ')
                    .nth(1)
                    .expect("first argument to .IP")
                    .replace(r"\(bu", "\u{2022}")
                    .replace(r"\fB", "")
                    .replace(r"\fP", ":");
                lines.push(label);
            } else if line.starts_with(".IB ") || line.starts_with(".BI ") {
                lines.push(line.split_whitespace().skip(1).collect());
            } else if line.starts_with(".sp") || line.starts_with(".PP") || line.starts_with(".TP")
            {
                lines.push(String::new());
            }
        } else if line.starts_with(r"\fB") && line.ends_with(r"\fP") {
            lines.push(format!("{}:", line.replace(r"\fB", "").replace(r"\fP", "")));
        } else {
            lines.push(line.to_string());
        }
    }
    lines.dedup_by(|a, b| a.is_empty() && b.is_empty());
    lines
        .join("\n")
        .replace(r"\fB", "")
        .replace(r"\fI", "")
        .replace(r"\fP", "")
        .replace(r"\-", "-")
        .replace(r"\\", r"\")
}

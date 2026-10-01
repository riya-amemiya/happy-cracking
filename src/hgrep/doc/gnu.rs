use super::version::{NAME, VERSION};
use super::{Category, FLAGS, FlagDoc, mentions, rename, short_var};

const TEMPLATE: &str = include_str!("template.gnu.help");
const RG_SHORTS_UNDER_GREP: &[&str] = &[".", "M", "N", "S", "g", "j", "p", "t"];
const COLUMN: usize = 28;

pub(crate) fn gnu_help(prog: &str) -> String {
    let rows: Vec<String> = Category::ALL
        .into_iter()
        .flat_map(|cat| FLAGS.iter().filter(move |flag| flag.cat == cat))
        .filter(|flag| flag.long.is_some_and(|long| !mentions(TEMPLATE, long)))
        .map(|flag| row(flag, prog))
        .collect();
    TEMPLATE
        .replace("!!PROG!!", prog)
        .replace("!!RIPGREP!!", &rows.join("\n"))
}

pub(crate) fn gnu_usage_hint(prog: &str) -> String {
    format!(
        "Usage: {prog} [OPTION]... PATTERNS [FILE]...\nTry '{prog} --help' for more information.\n"
    )
}

pub(crate) fn gnu_version(prog: &str) -> String {
    format!(
        "{prog} ({NAME}) {VERSION}\n\
         Copyright (c) 2026 Riya Amemiya\n\
         License MIT: <https://opensource.org/license/mit>.\n\
         You may use, copy, modify and distribute this software under that license.\n\
         It comes with no warranty, to the extent permitted by law.\n\
         \n\
         Written by Riya Amemiya; see\n\
         <https://github.com/riya-amemiya/happy-cracking>.\n\
         \n\
         {prog} -P uses a built-in PCRE2 compatible engine\n"
    )
}

fn row(flag: &FlagDoc, prog: &str) -> String {
    let short = flag
        .short
        .filter(|s| RG_SHORTS_UNDER_GREP.contains(s))
        .map_or_else(|| "    ".to_string(), |s| format!("-{s}, "));
    let var = flag
        .var
        .map_or_else(String::new, |v| format!("={}", short_var(v)));
    let name = format!("  {short}--{}{var}", flag.long.unwrap_or_default());
    let doc = describe(&rename(flag.doc, prog, prog));
    if name.len() + 2 <= COLUMN {
        format!("{name:COLUMN$}{doc}")
    } else if name.len() + 2 + doc.len() <= 79 {
        format!("{name}  {doc}")
    } else {
        format!("{name}\n{:COLUMN$}{doc}", "")
    }
}

fn describe(doc: &str) -> String {
    let doc = doc.strip_suffix('.').unwrap_or(doc);
    let letters: Vec<char> = doc
        .split(' ')
        .next()
        .unwrap_or_default()
        .chars()
        .filter(char::is_ascii_alphabetic)
        .collect();
    if letters.len() > 1 && letters.iter().all(char::is_ascii_uppercase) {
        return doc.to_string();
    }
    let mut chars = doc.chars();
    chars.next().map_or_else(String::new, |c| {
        c.to_ascii_lowercase().to_string() + chars.as_str()
    })
}

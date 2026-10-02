mod complete;
mod defs;
mod gnu;
mod help;
mod man;
mod version;

pub(crate) use complete::{complete_bash, complete_fish, complete_powershell, complete_zsh};
pub(crate) use gnu::{gnu_help, gnu_usage_hint, gnu_version};
pub(crate) use help::{help_long, help_short};
pub(crate) use man::man_page;
pub(crate) use version::{pcre2_version, version_long, version_short};

use defs::FLAGS;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Category {
    Input,
    Search,
    Filter,
    Output,
    OutputModes,
    Logging,
    OtherBehaviors,
}

impl Category {
    const ALL: [Category; 7] = [
        Category::Input,
        Category::Search,
        Category::Filter,
        Category::Output,
        Category::OutputModes,
        Category::Logging,
        Category::OtherBehaviors,
    ];

    fn placeholder(self) -> &'static str {
        match self {
            Category::Input => "!!input!!",
            Category::Search => "!!search!!",
            Category::Filter => "!!filter!!",
            Category::Output => "!!output!!",
            Category::OutputModes => "!!output-modes!!",
            Category::Logging => "!!logging!!",
            Category::OtherBehaviors => "!!other-behaviors!!",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Completion {
    Other,
    Filename,
    Executable,
    Filetype,
    Encoding,
}

struct FlagDoc {
    long: Option<&'static str>,
    short: Option<&'static str>,
    negated: Option<&'static str>,
    var: Option<&'static str>,
    cat: Category,
    doc: &'static str,
    long_doc: &'static str,
    choices: &'static [&'static str],
    complete: Completion,
}

impl FlagDoc {
    fn is_switch(&self) -> bool {
        self.var.is_none()
    }

    fn short_letter(&self) -> Option<&'static str> {
        self.short.filter(|s| s.len() == 1)
    }
}

fn rg_flags() -> impl Iterator<Item = &'static FlagDoc> {
    FLAGS.iter().filter(|flag| {
        flag.long
            .is_none_or(|long| !super::flags::grep_only_long_names().any(|name| name == long))
    })
}

fn lookup(name: &str) -> &'static FlagDoc {
    FLAGS
        .iter()
        .find(|flag| flag.long == Some(name))
        .unwrap_or_else(|| unreachable!("unknown flag {name} in docs"))
}

fn render_markup(doc: &str, tag: &str, mut replace: impl FnMut(&FlagDoc, &mut String)) -> String {
    let prefix = format!(r"\{tag}{{");
    let mut out = String::with_capacity(doc.len());
    let mut rest = doc;
    while let Some(at) = rest.find(&prefix) {
        out.push_str(&rest[..at]);
        let start = at + prefix.len();
        let end = start
            + rest[start..]
                .find('}')
                .unwrap_or_else(|| unreachable!("unclosed {prefix} in docs"));
        replace(lookup(&rest[start..end]), &mut out);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

fn rename(text: &str, prog: &str, tool: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        let rest = &text[at..];
        let hit = [("ripgrep", tool), ("rg", prog)]
            .into_iter()
            .find(|(word, _)| rest.starts_with(word) && is_word(text, at, word.len()));
        if let Some((word, name)) = hit {
            out.push_str(name);
            at += word.len();
        } else {
            let ch = rest.chars().next().unwrap_or_default();
            out.push(ch);
            at += ch.len_utf8();
        }
    }
    out
}

fn is_word(text: &str, at: usize, len: usize) -> bool {
    let before = &text[..at];
    let open = before
        .chars()
        .next_back()
        .is_none_or(|c| !(c.is_ascii_alphanumeric() || "_/.-".contains(c)))
        || before.ends_with(r"\fB")
        || before.ends_with(r"\fI");
    let close = text[at + len..]
        .chars()
        .next()
        .is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_'));
    open && close
}

fn short_var(var: &str) -> String {
    var.replace("SEPARATOR", "SEP")
        .replace("REPLACEMENT", "TEXT")
        .replace("NUM+SUFFIX?", "NUM")
}

fn mentions(text: &str, long: &str) -> bool {
    let needle = format!("--{long}");
    text.match_indices(&needle).any(|(at, _)| {
        text[at + needle.len()..]
            .chars()
            .next()
            .is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '-'))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hgrep::flags::{gnu_long_flag_names, grep_short_letters, long_flag_names};

    const UNDOCUMENTED_ALIASES: &[&str] = &[
        "colour",
        "fixed-regexp",
        "maxdepth",
        "passthrough",
        "silent",
    ];

    fn documented_longs() -> Vec<&'static str> {
        FLAGS.iter().filter_map(|flag| flag.long).collect()
    }

    fn documented_names() -> Vec<&'static str> {
        FLAGS
            .iter()
            .flat_map(|flag| flag.long.into_iter().chain(flag.negated))
            .collect()
    }

    fn rg_longs() -> Vec<&'static str> {
        rg_flags().filter_map(|flag| flag.long).collect()
    }

    fn rg_names() -> Vec<&'static str> {
        rg_flags()
            .flat_map(|flag| flag.long.into_iter().chain(flag.negated))
            .collect()
    }

    #[test]
    fn doc_table_matches_the_parser() {
        let parser = long_flag_names();
        let documented = documented_names();
        for name in &parser {
            assert!(
                documented.contains(name) || UNDOCUMENTED_ALIASES.contains(name),
                "--{name} has no documentation entry"
            );
        }
        for name in documented {
            assert!(
                parser.contains(&name),
                "--{name} is documented but not parsed"
            );
        }
    }

    #[test]
    fn help_and_man_document_every_flag() {
        let short = help_short("hg");
        let long = help_long("hg");
        let man = man_page("hg").replace(r"\-", "-");
        for name in rg_longs() {
            assert!(mentions(&short, name), "-h is missing --{name}");
        }
        for name in rg_names() {
            assert!(mentions(&long, name), "--help is missing --{name}");
            assert!(mentions(&man, name), "the man page is missing --{name}");
        }
    }

    #[test]
    fn completion_assets_carry_no_comments() {
        let zsh = complete_zsh("hg");
        let mut lines = zsh.lines();
        assert_eq!(lines.next(), Some("#compdef hg"));
        for line in lines
            .chain(complete_fish("hg").lines())
            .chain(complete_bash("hg").lines())
        {
            assert!(!line.trim_start().starts_with('#'), "comment line: {line}");
        }
    }

    #[test]
    fn rendered_docs_have_no_leftover_markup() {
        let outputs = [
            help_short("hg"),
            help_long("hrg"),
            man_page("hg"),
            gnu_help("hgrep"),
            complete_bash("hg"),
            complete_fish("hg"),
            complete_powershell("hg"),
            complete_zsh("hg"),
        ];
        for out in outputs {
            assert!(!out.contains("!!"), "unfilled placeholder in:\n{out}");
            assert!(!out.contains("!PROG!"), "unfilled program name in:\n{out}");
            assert!(!out.contains(r"\flag"), "unrendered flag markup in:\n{out}");
        }
        for out in [help_short("hg"), help_long("hg")] {
            assert!(!out.contains(r"\f"), "roff left in help:\n{out}");
            for line in out.lines() {
                assert!(line.chars().count() <= 79, "line too long: {line}");
            }
        }
    }

    #[test]
    fn gnu_help_lists_every_option() {
        let help = gnu_help("hgrep");
        for &name in documented_longs().iter().chain(gnu_long_flag_names()) {
            assert!(mentions(&help, name), "--help is missing --{name}");
        }
        for letter in grep_short_letters().into_iter().map(char::from) {
            let shown = if letter.is_ascii_digit() {
                help.contains("  -NUM ")
            } else {
                help.contains(&format!("-{letter},")) || help.contains(&format!("-{letter} "))
            };
            assert!(shown, "--help is missing -{letter}");
        }
        for line in help.lines() {
            assert!(line.len() <= 79, "line too long: {line}");
        }
    }

    #[test]
    fn completions_cover_every_flag() {
        let bash = complete_bash("hg");
        let fish = complete_fish("hg");
        let powershell = complete_powershell("hg");
        let zsh = complete_zsh("hg");
        for name in rg_names() {
            assert!(mentions(&bash, name), "bash is missing --{name}");
            assert!(
                fish.contains(&format!("-l {name} ")),
                "fish is missing --{name}"
            );
            assert!(
                powershell.contains(&format!("'--{name}'")),
                "powershell is missing --{name}"
            );
            assert!(mentions(&zsh, name), "zsh is missing --{name}");
        }
        assert!(bash.ends_with("complete -F _hg -o bashdefault -o default hg\n"));
        assert!(fish.contains("function __hg_contains_opt "));
        assert!(powershell.contains("-CommandName 'hg'"));
        assert!(zsh.contains("  _arguments -s -S : $args\n"));
        assert!(zsh.contains("compdef _hg hg\n"));
        assert!(zsh.contains("'(-i --ignore-case)--ignore-case[Case insensitive search.]'"));
        assert!(zsh.contains("'*-g+[Include or exclude file paths.]:glob'"));
        assert!(zsh.contains("'--sort=[Sort results in ascending order.]:sortby:(none path modified accessed created)'"));
        assert!(zsh.contains("'(-E --encoding)-E+[Specify the text encoding of files to search.]:encoding:_hg_encodings'"));
    }

    #[test]
    fn versions_and_usage_follow_each_layout() {
        let version = env!("CARGO_PKG_VERSION");
        assert_eq!(
            version_short("hg"),
            format!("hg {version} (happy-cracking)")
        );
        assert!(version_long("hg").starts_with(&format!(
            "hg {version} (happy-cracking)\n\nfeatures:+pcre2\n\n"
        )));
        assert!(pcre2_version().1);
        assert_eq!(
            gnu_usage_hint("hgrep"),
            "Usage: hgrep [OPTION]... PATTERNS [FILE]...\nTry 'hgrep --help' for more information.\n"
        );
        assert!(gnu_version("hgrep").starts_with(&format!("hgrep (happy-cracking) {version}\n")));
        assert!(gnu_help("hgrep").starts_with("Usage: hgrep [OPTION]... PATTERNS [FILE]...\n"));
    }

    #[test]
    fn rename_only_touches_the_tool_name() {
        assert_eq!(
            rename(
                r"ripgrep's \fBrg\fP and rg -e, but not .rgignore, ripgreprc, args or BurntSushi/ripgrep",
                "hg",
                "hrg"
            ),
            r"hrg's \fBhg\fP and hg -e, but not .rgignore, ripgreprc, args or BurntSushi/ripgrep"
        );
    }
}

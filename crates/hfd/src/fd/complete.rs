use std::env;
use std::fmt::Write as _;
use std::path::Path;

use clap::builder::{PossibleValue, StyledStr};
use clap::{Arg, ArgAction, Command, ValueEnum, ValueHint};

#[derive(Clone, Copy, PartialEq, Eq, Debug, ValueEnum)]
pub(super) enum Shell {
    Bash,
    Elvish,
    Fish,
    Powershell,
    Zsh,
}

impl Shell {
    pub(super) fn from_env() -> Option<Shell> {
        let Some(path) = env::var_os("SHELL") else {
            return cfg!(windows).then_some(Shell::Powershell);
        };
        match Path::new(&path).file_stem()?.to_str()? {
            "bash" => Some(Shell::Bash),
            "zsh" => Some(Shell::Zsh),
            "fish" => Some(Shell::Fish),
            "elvish" => Some(Shell::Elvish),
            "powershell" | "powershell_ise" => Some(Shell::Powershell),
            _ => None,
        }
    }
}

pub(super) fn generate(shell: Shell, mut cmd: Command, bin_name: &str) -> String {
    cmd = cmd.bin_name(bin_name.to_owned());
    cmd.build();
    match shell {
        Shell::Bash => bash(&cmd, bin_name),
        Shell::Elvish => elvish(&cmd, bin_name),
        Shell::Fish => fish(&cmd, bin_name),
        Shell::Powershell => powershell(&cmd, bin_name),
        Shell::Zsh => zsh(&cmd, bin_name),
    }
}

fn takes_values(arg: &Arg) -> bool {
    arg.get_num_args().is_some_and(|n| n.takes_values())
}

fn flags(cmd: &Command) -> impl Iterator<Item = &Arg> {
    cmd.get_arguments()
        .filter(|a| !takes_values(a) && !a.is_positional())
}

fn possible_values(arg: &Arg) -> Option<Vec<PossibleValue>> {
    if takes_values(arg) {
        arg.get_value_parser()
            .possible_values()
            .map(Iterator::collect)
    } else {
        None
    }
}

fn visible_names(values: &[PossibleValue], sep: &str) -> String {
    values
        .iter()
        .filter(|v| !v.is_hide_set())
        .map(PossibleValue::get_name)
        .collect::<Vec<_>>()
        .join(sep)
}

fn help_text(help: Option<&StyledStr>) -> String {
    help.map_or_else(String::new, ToString::to_string)
}

fn bash(cmd: &Command, name: &str) -> String {
    format!(
        "_{name}() {{
    local i cur prev opts cmd
    COMPREPLY=()
    if [[ \"${{BASH_VERSINFO[0]}}\" -ge 4 ]]; then
        cur=\"$2\"
    else
        cur=\"${{COMP_WORDS[COMP_CWORD]}}\"
    fi
    prev=\"$3\"
    cmd=\"\"
    opts=\"\"

    for i in \"${{COMP_WORDS[@]:0:COMP_CWORD}}\"
    do
        case \"${{cmd}},${{i}}\" in
            \",$1\")
                cmd=\"{cmd}\"
                ;;
            *)
                ;;
        esac
    done

    case \"${{cmd}}\" in
        {cmd})
            opts=\"{opts}\"
            if [[ ${{cur}} == -* || ${{COMP_CWORD}} -eq 1 ]] ; then
                COMPREPLY=( $(compgen -W \"${{opts}}\" -- \"${{cur}}\") )
                return 0
            fi
            case \"${{prev}}\" in{details}
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W \"${{opts}}\" -- \"${{cur}}\") )
            return 0
            ;;
    esac
}}

if [[ \"${{BASH_VERSINFO[0]}}\" -eq 4 && \"${{BASH_VERSINFO[1]}}\" -ge 4 || \"${{BASH_VERSINFO[0]}}\" -gt 4 ]]; then
    complete -F _{name} -o nosort -o bashdefault -o default {name}
else
    complete -F _{name} -o bashdefault -o default {name}
fi
",
        cmd = name.replace('-', "__"),
        opts = bash_options(cmd),
        details = bash_details(cmd),
    )
}

fn bash_options(cmd: &Command) -> String {
    let named: Vec<&Arg> = cmd.get_arguments().filter(|a| !a.is_positional()).collect();
    let shorts = named.iter().flat_map(|a| {
        a.get_short().map_or_else(Vec::new, |s| {
            let mut all = a.get_visible_short_aliases().unwrap_or_default();
            all.push(s);
            all
        })
    });
    let longs = named.iter().flat_map(|a| {
        a.get_long().map_or_else(Vec::new, |l| {
            let mut all = a.get_visible_aliases().unwrap_or_default();
            all.push(l);
            all
        })
    });
    let mut opts = String::new();
    for short in shorts {
        let _ = write!(opts, "-{short} ");
    }
    for long in longs {
        let _ = write!(opts, "--{long} ");
    }
    for pos in cmd.get_positionals() {
        match possible_values(pos) {
            Some(values) => {
                for value in values {
                    let _ = write!(opts, "{} ", value.get_name());
                }
            }
            None => {
                let _ = write!(opts, "{pos} ");
            }
        }
    }
    opts.pop();
    opts
}

fn bash_details(cmd: &Command) -> String {
    let mut out = vec![String::new()];
    for o in cmd.get_opts() {
        let hint = o.get_value_hint();
        let compopt = match hint {
            ValueHint::FilePath => Some("compopt -o filenames"),
            ValueHint::DirPath => Some("compopt -o plusdirs"),
            ValueHint::Other => Some("compopt -o nospace"),
            _ => None,
        };
        let vals = bash_vals(o);
        let entry = |label: String| {
            let mut v = vec![format!("{label})")];
            if hint == ValueHint::FilePath {
                v.extend([
                    "local oldifs".to_owned(),
                    r#"if [ -n "${IFS+x}" ]; then"#.to_owned(),
                    r#"    oldifs="$IFS""#.to_owned(),
                    "fi".to_owned(),
                    r"IFS=$'\n'".to_owned(),
                    format!("COMPREPLY=({vals})"),
                    r#"if [ -n "${oldifs+x}" ]; then"#.to_owned(),
                    r#"    IFS="$oldifs""#.to_owned(),
                    "fi".to_owned(),
                ]);
            } else {
                v.push(format!("COMPREPLY=({vals})"));
            }
            if let Some(copt) = compopt {
                v.extend([
                    r#"if [[ "${BASH_VERSINFO[0]}" -ge 4 ]]; then"#.to_owned(),
                    format!("    {copt}"),
                    "fi".to_owned(),
                ]);
            }
            v.extend(["return 0".to_owned(), ";;".to_owned()]);
            v.join("\n                    ")
        };
        if let Some(longs) = o.get_long_and_visible_aliases() {
            out.extend(longs.iter().map(|l| entry(format!("--{l}"))));
        }
        if let Some(shorts) = o.get_short_and_visible_aliases() {
            out.extend(shorts.iter().map(|s| entry(format!("-{s}"))));
        }
    }
    out.join("\n                ")
}

fn bash_vals(o: &Arg) -> String {
    if let Some(values) = possible_values(o) {
        format!(
            "$(compgen -W \"{}\" -- \"${{cur}}\")",
            visible_names(&values, " ")
        )
    } else if o.get_value_hint() == ValueHint::DirPath {
        String::new()
    } else if o.get_value_hint() == ValueHint::Other {
        String::from("\"${cur}\"")
    } else {
        String::from("$(compgen -f \"${cur}\")")
    }
}

fn named_args(cmd: &Command) -> impl Iterator<Item = &Arg> {
    cmd.get_opts().chain(flags(cmd))
}

fn elvish(cmd: &Command, name: &str) -> String {
    let escape = |help: Option<&StyledStr>, data: String| match help {
        Some(h) => h.to_string().replace('\n', " ").replace('\'', "''"),
        None => data,
    };
    let mut completions = String::new();
    for arg in named_args(cmd) {
        if let Some(shorts) = arg.get_short_and_visible_aliases() {
            let tooltip = escape(arg.get_help(), shorts[0].to_string());
            for short in shorts {
                let _ = write!(completions, "\n            cand -{short} '{tooltip}'");
            }
        }
        if let Some(longs) = arg.get_long_and_visible_aliases() {
            let tooltip = escape(arg.get_help(), longs[0].to_owned());
            for long in longs {
                let _ = write!(completions, "\n            cand --{long} '{tooltip}'");
            }
        }
    }
    format!(
        r"
use builtin;
use str;

set edit:completion:arg-completer[{name}] = {{|@words|
    fn spaces {{|n|
        builtin:repeat $n ' ' | str:join ''
    }}
    fn cand {{|text desc|
        edit:complex-candidate $text &display=$text' '(spaces (- 14 (wcswidth $text)))$desc
    }}
    var command = '{name}'
    for word $words[1..-1] {{
        if (str:has-prefix $word '-') {{
            break
        }}
        set command = $command';'$word
    }}
    var completions = [
        &'{name}'= {{{completions}
        }}
    ]
    $completions[$command]
}}
"
    )
}

fn powershell(cmd: &Command, name: &str) -> String {
    let escape = |help: Option<&StyledStr>, data: String| {
        let text = help_text(help);
        if text.is_empty() {
            data
        } else {
            text.replace('\n', " ")
                .replace('\'', "''")
                .replace('\u{2019}', "'\u{2019}")
        }
    };
    let preamble = "\n            [CompletionResult]::new(";
    let mut completions = String::new();
    for arg in named_args(cmd) {
        if let Some(shorts) = arg.get_short_and_visible_aliases() {
            let tooltip = escape(arg.get_help(), shorts[0].to_string());
            for s in shorts {
                let pad = if s.is_uppercase() { " " } else { "" };
                let _ = write!(
                    completions,
                    "{preamble}'-{s}', '-{s}{pad}', [CompletionResultType]::ParameterName, '{tooltip}')"
                );
            }
        }
        if let Some(longs) = arg.get_long_and_visible_aliases() {
            let tooltip = escape(arg.get_help(), longs[0].to_owned());
            for l in longs {
                let _ = write!(
                    completions,
                    "{preamble}'--{l}', '--{l}', [CompletionResultType]::ParameterName, '{tooltip}')"
                );
            }
        }
    }
    format!(
        r#"
using namespace System.Management.Automation
using namespace System.Management.Automation.Language

Register-ArgumentCompleter -Native -CommandName '{name}' -ScriptBlock {{
    param($wordToComplete, $commandAst, $cursorPosition)

    $commandElements = $commandAst.CommandElements
    $command = @(
        '{name}'
        for ($i = 1; $i -lt $commandElements.Count; $i++) {{
            $element = $commandElements[$i]
            if ($element -isnot [StringConstantExpressionAst] -or
                $element.StringConstantType -ne [StringConstantType]::BareWord -or
                $element.Value.StartsWith('-') -or
                $element.Value -eq $wordToComplete) {{
                break
        }}
        $element.Value
    }}) -join ';'

    $completions = @(switch ($command) {{
        '{name}' {{{completions}
            break
        }}
    }})

    $completions.Where{{ $_.CompletionText -like "$wordToComplete*" }} |
        Sort-Object -Property ListItemText
}}
"#
    )
}

fn fish_escape(text: &str, comma: bool) -> String {
    let text = text.replace('\\', "\\\\").replace('\'', "\\'");
    if comma {
        text.replace(',', "\\,")
    } else {
        text
    }
}

fn fish_help(help: &StyledStr) -> String {
    fish_escape(&help.to_string().replace('\n', " "), false)
}

fn fish(cmd: &Command, name: &str) -> String {
    let mut out = String::new();
    for arg in named_args(cmd) {
        let _ = write!(out, "complete -c {name}");
        for short in arg.get_short_and_visible_aliases().unwrap_or_default() {
            let _ = write!(out, " -s {short}");
        }
        for long in arg.get_long_and_visible_aliases().unwrap_or_default() {
            let _ = write!(out, " -l {}", fish_escape(long, false));
        }
        if let Some(help) = arg.get_help() {
            let _ = write!(out, " -d '{}'", fish_help(help));
        }
        out.push_str(&fish_values(arg));
        out.push('\n');
    }
    out
}

fn fish_values(arg: &Arg) -> String {
    if !takes_values(arg) {
        return String::new();
    }
    if let Some(values) = possible_values(arg) {
        return format!(
            " -r -f -a \"{}\"",
            values
                .iter()
                .filter(|v| !v.is_hide_set())
                .map(|v| format!(
                    "{}\\t'{}'",
                    fish_escape(v.get_name(), true),
                    fish_help(v.get_help().unwrap_or(&StyledStr::new()))
                ))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
    match arg.get_value_hint() {
        ValueHint::Unknown => " -r",
        ValueHint::AnyPath | ValueHint::FilePath | ValueHint::ExecutablePath => " -r -F",
        ValueHint::DirPath => " -r -f -a \"(__fish_complete_directories)\"",
        ValueHint::CommandString | ValueHint::CommandName => {
            " -r -f -a \"(__fish_complete_command)\""
        }
        ValueHint::Username => " -r -f -a \"(__fish_complete_users)\"",
        ValueHint::Hostname => " -r -f -a \"(__fish_print_hostnames)\"",
        _ => " -r -f",
    }
    .to_owned()
}

fn zsh(cmd: &Command, name: &str) -> String {
    let mut segments = vec![String::from("_arguments \"${_arguments_options[@]}\" : \\")];
    segments.extend(
        [zsh_opts(cmd), zsh_flags(cmd), zsh_positionals(cmd)]
            .into_iter()
            .filter(|s| !s.is_empty()),
    );
    if cmd.is_allow_external_subcommands_set() {
        segments.push(String::from("\"*::external_command:_default\" \\"));
    }
    segments.push(String::from("&& ret=0"));
    format!(
        "#compdef {name}

autoload -U is-at-least

_{name}() {{
    typeset -A opt_args
    typeset -a _arguments_options
    local ret=1

    if is-at-least 5.2; then
        _arguments_options=(-s -S -C)
    else
        _arguments_options=(-s -C)
    fi

    local context curcontext=\"$curcontext\" state line
    {args}
}}

(( $+functions[_{under}_commands] )) ||
_{under}_commands() {{
    local commands; commands=()
    _describe -t commands '{name} commands' commands \"$@\"
}}

if [ \"$funcstack[1]\" = \"_{name}\" ]; then
    _{name} \"$@\"
else
    compdef _{name} {name}
fi
",
        args = segments.join("\n"),
        under = name.replace(' ', "__"),
    )
}

fn zsh_escape_help(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('\'', "'\\''")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace(':', "\\:")
        .replace('$', "\\$")
        .replace('`', "\\`")
        .replace('\n', " ")
}

fn zsh_escape_value(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('\'', "'\\''")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace(':', "\\:")
        .replace('$', "\\$")
        .replace('`', "\\`")
        .replace('(', "\\(")
        .replace(')', "\\)")
        .replace(' ', "\\ ")
}

fn zsh_values(arg: &Arg) -> Option<String> {
    if let Some(values) = possible_values(arg) {
        let visible = values.iter().filter(|v| !v.is_hide_set());
        if visible.clone().any(|v| v.get_help().is_some()) {
            return Some(format!(
                "(({}))",
                visible
                    .map(|v| format!(
                        r#"{}\:"{}""#,
                        zsh_escape_value(v.get_name()),
                        zsh_escape_help(&help_text(v.get_help()))
                    ))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }
        return Some(format!("({})", visible_names(&values, " ")));
    }
    let hint = match arg.get_value_hint() {
        ValueHint::Unknown => "_default",
        ValueHint::Other => "",
        ValueHint::AnyPath | ValueHint::FilePath => "_files",
        ValueHint::DirPath => "_files -/",
        ValueHint::ExecutablePath => "_absolute_command_paths",
        ValueHint::CommandName => "_command_names -e",
        ValueHint::CommandString => "_cmdstring",
        ValueHint::CommandWithArguments => "_cmdambivalent",
        ValueHint::Username => "_users",
        ValueHint::Hostname => "_hosts",
        ValueHint::Url => "_urls",
        ValueHint::EmailAddress => "_email_addresses",
        _ => return None,
    };
    Some(hint.to_owned())
}

fn zsh_conflicts(cmd: &Command, arg: &Arg) -> String {
    let conflicts = cmd.get_arg_conflicts_with(arg);
    if conflicts.is_empty() {
        return String::new();
    }
    let names: Vec<String> = conflicts
        .iter()
        .flat_map(|c| {
            c.get_short()
                .map(|s| format!("-{s}"))
                .into_iter()
                .chain(c.get_long().map(|l| format!("--{l}")))
        })
        .collect();
    format!("({})", names.join(" "))
}

fn zsh_multiple(arg: &Arg) -> &'static str {
    if matches!(arg.get_action(), ArgAction::Count | ArgAction::Append) {
        "*"
    } else {
        ""
    }
}

fn zsh_opts(cmd: &Command) -> String {
    let mut ret = vec![];
    for o in cmd.get_opts() {
        let help = zsh_escape_help(&help_text(o.get_help()));
        let conflicts = zsh_conflicts(cmd, o);
        let multiple = zsh_multiple(o);
        let vn = o
            .get_value_names()
            .map_or_else(|| " ".to_owned(), |v| v[0].to_string());
        let vc = match zsh_values(o) {
            Some(val) => format!(":{vn}:{val}"),
            None => format!(":{vn}: "),
        };
        let vc = match o.get_num_args().map_or(1, |n| n.min_values()) {
            0 => format!(":{vc}"),
            min => vc.repeat(min),
        };
        for short in o.get_short_and_visible_aliases().unwrap_or_default() {
            ret.push(format!("'{conflicts}{multiple}-{short}+[{help}]{vc}' \\"));
        }
        for long in o.get_long_and_visible_aliases().unwrap_or_default() {
            ret.push(format!("'{conflicts}{multiple}--{long}=[{help}]{vc}' \\"));
        }
    }
    ret.join("\n")
}

fn zsh_flags(cmd: &Command) -> String {
    let mut ret = vec![];
    for f in flags(cmd) {
        let help = zsh_escape_help(&help_text(f.get_help()));
        let conflicts = zsh_conflicts(cmd, f);
        let multiple = zsh_multiple(f);
        if let Some(short) = f.get_short() {
            ret.push(format!("'{conflicts}{multiple}-{short}[{help}]' \\"));
            for alias in f.get_visible_short_aliases().unwrap_or_default() {
                ret.push(format!("'{conflicts}{multiple}-{alias}[{help}]' \\"));
            }
        }
        if let Some(long) = f.get_long() {
            ret.push(format!("'{conflicts}{multiple}--{long}[{help}]' \\"));
            for alias in f.get_visible_aliases().unwrap_or_default() {
                ret.push(format!("'{conflicts}{multiple}--{alias}[{help}]' \\"));
            }
        }
    }
    ret.join("\n")
}

fn zsh_positionals(cmd: &Command) -> String {
    let mut ret = vec![];
    let mut catch_all_emitted = false;
    for arg in cmd.get_positionals() {
        let multi = arg.get_num_args().is_some_and(|n| n.max_values() > 1);
        if catch_all_emitted && (arg.is_last_set() || multi) {
            continue;
        }
        let cardinality = if multi && !cmd.has_subcommands() {
            if let Some(t) = arg.get_value_terminator() {
                format!("*{}:", zsh_escape_value(t))
            } else {
                catch_all_emitted = true;
                "*:".to_owned()
            }
        } else if arg.is_required_set() {
            String::new()
        } else {
            ":".to_owned()
        };
        let help = arg
            .get_help()
            .map_or_else(String::new, |h| format!(" -- {h}"))
            .replace('[', "\\[")
            .replace(']', "\\]")
            .replace('\'', "'\\''")
            .replace(':', "\\:");
        ret.push(format!(
            "'{cardinality}:{}{help}:{}' \\",
            arg.get_id(),
            zsh_values(arg).unwrap_or_default()
        ));
    }
    ret.join("\n")
}

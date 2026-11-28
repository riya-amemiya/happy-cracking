use std::fmt::Write;

use super::{Completion, FlagDoc, rename, rg_flags, short_var};

const ENCODINGS: &str = include_str!("encodings.sh");
const FISH_PRELUDE: &str = include_str!("prelude.fish");

const REPEATABLE: &[&str] = &[
    "colors",
    "exclude",
    "exclude-dir",
    "exclude-from",
    "file",
    "glob",
    "iglob",
    "ignore-file",
    "include",
    "pre-glob",
    "regexp",
    "type",
    "type-add",
    "type-clear",
    "type-not",
    "unrestricted",
];

const ZSH: &str = r#"#compdef !PROG!

_!PROG!() {
  local -a args
  args=(
!SPECS!
  )
  _arguments -s -S : $args
}

_!PROG!_types() {
  local -a expl
  local -aU types
  types=( ${(@)${(f)"$( _call_program types $words[1] --type-list )"}//:[[:space:]]##/:} )
  _wanted types expl 'file type' compadd "$@" - ${(@)types%%:*}
}

_!PROG!_encodings() {
  local -a expl
  local -aU encodings
  encodings=(
!ENCODINGS!
  )
  _wanted encodings expl encoding compadd -a "$@" - encodings
}

if [[ $funcstack[1] == _!PROG! ]] || (( ! $+functions[compdef] )); then
  _!PROG! "$@"
else
  compdef _!PROG! !PROG!
fi
"#;

const BASH: &str = r#"
_!PROG!() {
  local i cur prev opts cmds
  COMPREPLY=()
  cur="${COMP_WORDS[COMP_CWORD]}"
  prev="${COMP_WORDS[COMP_CWORD-1]}"
  cmd=""
  opts=""

  for i in ${COMP_WORDS[@]}; do
    case "${i}" in
      !PROG!)
        cmd="!PROG!"
        ;;
      *)
        ;;
    esac
  done

  case "${cmd}" in
    !PROG!)
      opts="!OPTS!"
      if [[ ${cur} == -* || ${COMP_CWORD} -eq 1 ]] ; then
        COMPREPLY=($(compgen -W "${opts}" -- "${cur}"))
        return 0
      fi
      case "${prev}" in
!CASES!
      esac
      COMPREPLY=($(compgen -W "${opts}" -- "${cur}"))
      return 0
      ;;
  esac
}

complete -F _!PROG! -o bashdefault -o default !PROG!
"#;

const BASH_CASE: &str = r#"
        !FLAG!)
          COMPREPLY=($(compgen -f "${cur}"))
          return 0
          ;;"#;

const BASH_CASE_CHOICES: &str = r#"
        !FLAG!)
          COMPREPLY=($(compgen -W "!CHOICES!" -- "${cur}"))
          return 0
          ;;"#;

const POWERSHELL: &str = r#"
using namespace System.Management.Automation
using namespace System.Management.Automation.Language

Register-ArgumentCompleter -Native -CommandName '!PROG!' -ScriptBlock {
  param($wordToComplete, $commandAst, $cursorPosition)
  $commandElements = $commandAst.CommandElements
  $command = @(
    '!PROG!'
    for ($i = 1; $i -lt $commandElements.Count; $i++) {
        $element = $commandElements[$i]
        if ($element -isnot [StringConstantExpressionAst] -or
            $element.StringConstantType -ne [StringConstantType]::BareWord -or
            $element.Value.StartsWith('-')) {
            break
    }
    $element.Value
  }) -join ';'

  $completions = @(switch ($command) {
    '!PROG!' {
!FLAGS!
    }
  })

  $completions.Where{ $_.CompletionText -like "$wordToComplete*" } |
    Sort-Object -Property ListItemText
}
"#;

fn names(flag: &FlagDoc) -> Vec<String> {
    let long = flag.long.map(|long| format!("--{long}"));
    let short = flag.short_letter().map(|short| format!("-{short}"));
    let negated = flag.negated.map(|negated| format!("--{negated}"));
    [long, short, negated].into_iter().flatten().collect()
}

pub(crate) fn complete_bash(prog: &str) -> String {
    let mut opts = String::new();
    let mut cases = String::new();
    for flag in rg_flags() {
        let template = if flag.choices.is_empty() {
            BASH_CASE.to_string()
        } else {
            BASH_CASE_CHOICES.replace("!CHOICES!", &flag.choices.join(" "))
        };
        for name in names(flag) {
            write!(opts, "{name} ").unwrap();
            cases.push_str(&template.replace("!FLAG!", &name));
        }
    }
    opts.push_str("<PATTERN> <PATH>...");
    BASH.replace("!OPTS!", &opts)
        .replace("!CASES!", &cases)
        .replace("!PROG!", prog)
        .trim_start()
        .to_string()
}

pub(crate) fn complete_fish(prog: &str) -> String {
    let mut out = FISH_PRELUDE.replace("!PROG!", prog);
    out.push('\n');
    for flag in rg_flags() {
        let short = flag.short_letter();
        if flag.long.is_none() && short.is_none() {
            continue;
        }
        let doc = rename(flag.doc, prog, prog).replace('\'', "\\'");
        if let Some(long) = flag.long {
            let short = short.map_or_else(String::new, |s| format!("-s {s}"));
            write!(out, "complete -c {prog} {short} -l {long} -d '{doc}'").unwrap();
        } else {
            let short = short.unwrap_or_default();
            write!(out, "complete -c {prog} -s {short} -d '{doc}'").unwrap();
        }
        match flag.complete {
            Completion::Filename => out.push_str(" -r -F"),
            Completion::Executable => out.push_str(" -r -f -a '(__fish_complete_command)'"),
            Completion::Filetype => {
                write!(
                    out,
                    " -r -f -a '({prog} --type-list | string replace : \\t)'"
                )
                .unwrap();
            }
            Completion::Encoding => write!(out, " -r -f -a '{ENCODINGS}'").unwrap(),
            Completion::Other if !flag.choices.is_empty() => {
                write!(out, " -r -f -a '{}'", flag.choices.join(" ")).unwrap();
            }
            Completion::Other if !flag.is_switch() => out.push_str(" -r -f"),
            Completion::Other => {}
        }
        out.push('\n');
        if let (Some(negated), Some(long)) = (flag.negated, flag.long) {
            writeln!(
                out,
                "complete -c {prog} -l {negated} -n '__{prog}_contains_opt {long} {}' -d '{doc}'",
                short.unwrap_or_default()
            )
            .unwrap();
        }
    }
    out
}

pub(crate) fn complete_powershell(prog: &str) -> String {
    let mut entries = Vec::new();
    for flag in rg_flags() {
        let doc = rename(flag.doc, prog, prog).replace('\'', "''");
        for dash in names(flag) {
            let name = dash.trim_start_matches('-');
            entries.push(format!(
                "      [CompletionResult]::new('{dash}', '{name}', [CompletionResultType]::ParameterName, '{doc}')"
            ));
        }
    }
    POWERSHELL
        .trim_start()
        .replace("!FLAGS!", &entries.join("\n"))
        .replace("!PROG!", prog)
}

pub(crate) fn complete_zsh(prog: &str) -> String {
    let mut specs = Vec::new();
    for flag in rg_flags() {
        let names: Vec<String> = [
            flag.short_letter().map(|short| format!("-{short}")),
            flag.long.map(|long| format!("--{long}")),
        ]
        .into_iter()
        .flatten()
        .collect();
        if names.is_empty() {
            continue;
        }
        let doc = zsh_escape(&rename(flag.doc, prog, prog));
        let repeat = flag.long.is_some_and(|long| REPEATABLE.contains(&long));
        let prefix = match (repeat, names.len()) {
            (true, _) => "*".to_string(),
            (false, 1) => String::new(),
            (false, _) => format!("({})", names.join(" ")),
        };
        let value = flag.var.map_or_else(String::new, |var| {
            let message = short_var(var).to_lowercase();
            match zsh_action(flag, prog) {
                Some(action) => format!(":{message}:{action}"),
                None => format!(":{message}"),
            }
        });
        for name in &names {
            let suffix = match (flag.is_switch(), name.starts_with("--")) {
                (true, _) => "",
                (false, true) => "=",
                (false, false) => "+",
            };
            specs.push(zsh_quote(&format!("{prefix}{name}{suffix}[{doc}]{value}")));
        }
        if let (Some(negated), Some(long)) = (flag.negated, flag.long) {
            specs.push(zsh_quote(&format!("--{negated}[Negate --{long}.]")));
        }
    }
    specs.push(zsh_quote(
        r#"(--files --type-list -e --regexp -f --file)1: :_guard "^-*" pattern"#,
    ));
    specs.push(zsh_quote("*: :_files"));
    let specs: Vec<String> = specs
        .into_iter()
        .map(|spec| format!("    {spec}"))
        .collect();
    ZSH.replace("!SPECS!", &specs.join("\n"))
        .replace("!ENCODINGS!", ENCODINGS.trim_end())
        .replace("!PROG!", prog)
}

fn zsh_action(flag: &FlagDoc, prog: &str) -> Option<String> {
    match flag.complete {
        Completion::Filename => Some("_files".to_string()),
        Completion::Executable => Some("_command_names -e".to_string()),
        Completion::Filetype => Some(format!("_{prog}_types")),
        Completion::Encoding => Some(format!("_{prog}_encodings")),
        Completion::Other if !flag.choices.is_empty() => {
            Some(format!("({})", flag.choices.join(" ")))
        }
        Completion::Other => None,
    }
}

fn zsh_escape(doc: &str) -> String {
    doc.replace('\\', r"\\")
        .replace('[', r"\[")
        .replace(']', r"\]")
}

fn zsh_quote(spec: &str) -> String {
    format!("'{}'", spec.replace('\'', r"'\''"))
}

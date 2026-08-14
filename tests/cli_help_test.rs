use std::process::Command;

fn help_stdout() -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_happy-cracking"))
        .arg("--help")
        .output()
        .expect("run happy-cracking --help");
    assert!(out.status.success(), "stderr {:?}", out.stderr);
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn command_names(help: &str) -> Vec<String> {
    let start = help.find("Commands:").expect("Commands heading in --help");
    let rest = &help[start..];
    let end = rest.find("\nOptions:").unwrap_or(rest.len());
    rest[..end]
        .lines()
        .skip(1)
        .filter_map(|line| {
            let name = line.split_whitespace().next()?;
            if name == "help" {
                return None;
            }
            Some(name.to_string())
        })
        .collect()
}

#[test]
fn help_lists_hgrep_companion() {
    let help = help_stdout();
    assert!(
        help.contains("hgrep"),
        "top-level help should mention the hgrep companion binary, got {help}"
    );
}

#[test]
fn readme_documents_every_cli_command() {
    let help = help_stdout();
    let names = command_names(&help);
    assert!(
        !names.is_empty(),
        "parsed no command names from --help:\n{help}"
    );

    let readme = include_str!("../README.md");
    let missing: Vec<&str> = names
        .iter()
        .map(String::as_str)
        .filter(|name| !readme.contains(&format!("`{name}`")))
        .collect();
    assert!(
        missing.is_empty(),
        "README is missing CLI commands: {missing:?}"
    );
}

#[test]
fn readme_documents_hgrep() {
    let readme = include_str!("../README.md");
    assert!(readme.contains("`hgrep`"), "README should document hgrep");
}

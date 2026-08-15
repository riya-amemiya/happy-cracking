let meta = #{
    name: "build-hfind",
    description: "Add hg alias for hgrep and implement find-compatible hfind/hfd with adversarial review, QA, and coverage",
    when_to_use: "When adding the hg alias or building or hardening the hfind/hfd companion binary",
    phases: [
        #{ title: "Survey", detail: "read hgrep, cargo layout, tests, README" },
        #{ title: "Spec", detail: "lock find-compatible surface and alias wiring" },
        #{ title: "Implement", detail: "write hg alias, hfind/hfd, tests, docs" },
        #{ title: "Review", detail: "adversarial review across four dimensions" },
        #{ title: "Verify", detail: "independent skeptics fail-closed on each finding" },
        #{ title: "Fix", detail: "land only confirmed findings" },
        #{ title: "QA", detail: "fmt, clippy, test, llvm-cov" },
        #{ title: "Cover", detail: "close coverage gaps toward 100 percent" },
    ],
};

let notes_schema = #{
    "type": "object",
    "properties": #{
        "notes": #{ "type": "string" },
        "files_written": #{ "type": "array", "items": #{ "type": "string" } },
    },
};

let findings_schema = #{
    "type": "object",
    "properties": #{
        "findings": #{
            "type": "array",
            "maxItems": 8,
            "items": #{
                "type": "object",
                "properties": #{
                    "file": #{ "type": "string" },
                    "issue": #{ "type": "string" },
                },
            },
        },
    },
};

let verdict_schema = #{
    "type": "object",
    "properties": #{
        "real": #{ "type": "boolean" },
        "reason": #{ "type": "string" },
        "evidence": #{ "type": "string" },
    },
};

let qa_schema = #{
    "type": "object",
    "properties": #{
        "fmt_ok": #{ "type": "boolean" },
        "clippy_ok": #{ "type": "boolean" },
        "test_ok": #{ "type": "boolean" },
        "coverage_pct": #{ "type": "number" },
        "uncovered": #{ "type": "string" },
        "notes": #{ "type": "string" },
    },
};

fn repo_root(raw) {
    if raw == () { return "."; }
    if type_of(raw) != "map" { return "."; }
    if raw.repo == () { return "."; }
    raw.repo
}

fn encode_out(r) {
    if r == () { return ""; }
    if !r.success { return ""; }
    if r.output == () { return ""; }
    json_encode(r.output)
}

fn as_findings(r) {
    let out = [];
    if r == () { return out; }
    if !r.success { return out; }
    if r.output == () { return out; }
    if r.output.findings == () { return out; }
    r.output.findings
}

fn qa_pct(r) {
    if r == () { return (); }
    if !r.success { return (); }
    if r.output == () { return (); }
    r.output.coverage_pct
}

fn qa_uncovered(r) {
    if r == () { return ""; }
    if !r.success { return ""; }
    if r.output == () { return ""; }
    if r.output.uncovered == () { return ""; }
    r.output.uncovered
}

let repo = repo_root(args);
let coverage_target = 100;
if args != () && type_of(args) == "map" && args.coverage_target != () {
    coverage_target = args.coverage_target;
}

const MAX_VERIFY = 12;

let facts = "";
facts += "Repository root: " + repo + "\n";
facts += "This is happy-cracking, a Rust 2024 CTF toolkit.\n";
facts += "hgrep already lives at src/bin/hgrep/ with modules cli.rs gitconfig.rs ignore.rs main.rs matcher.rs search.rs source.rs walk.rs.\n";
facts += "Cargo auto-discovers the hgrep binary from that directory. cargo install --list shows happy-cracking installing happy-cracking and hgrep.\n";
facts += "There is no [[bin]] table in Cargo.toml yet. Adding [[bin]] entries does not disable autobins.\n";
facts += "Wire the hg alias as [[bin]] name hg path src/bin/hgrep/main.rs so hg and hgrep share one crate root.\n";
facts += "Create src/bin/hfind/main.rs plus sibling modules. Wire [[bin]] name hfind and [[bin]] name hfd both pointing at src/bin/hfind/main.rs.\n";
facts += "Do not invent extra binaries.\n";
facts += "hgrep walk visits files only, uses rayon per directory level, optional gitignore, macOS NFC via iconv UTF-8-MAC, and prefixes errors with hgrep:.\n";
facts += "hfind must visit directories, regular files, and symlinks. GNU find default is -P, do not follow symlinks, detect cycles by device plus inode.\n";
facts += "Reuse gitignore and gitconfig ideas if --gitignore is set. Do not skip .git or hidden names unless --gitignore says so. find lists hidden names.\n";
facts += "Do not extract hgrep walk into the lib unless you can keep every existing hgrep test green without behavior change. Prefer an hfind-owned walk that copies only what it needs.\n";
facts += "No new Cargo dependencies. regex, rayon, memchr, clap, anyhow, libc are already in Cargo.toml.\n";
facts += "Add zero comments and zero Japanese in any code, tests, or commit-bound artifact. Test data stays ASCII or \\u escapes.\n";
facts += "Match existing style. Integration tests go in tests/hfind_test.rs using CARGO_BIN_EXE_hfind, a temp sandbox, HOME and GIT_CONFIG isolation like tests/hgrep_test.rs.\n";
facts += "Also assert CARGO_BIN_EXE_hg exists and behaves like hgrep, and CARGO_BIN_EXE_hfd exists and behaves like hfind.\n";
facts += "Update src/main.rs after_help, tests/cli_help_test.rs, and README.md companion section plus install line plus usage.\n";
facts += "cargo fmt and cargo clippy with warnings denied must pass. cargo test must stay green for the whole crate.\n";

let find_spec = "";
find_spec += "hfind is a parallel find-compatible walker. hfd is only another argv0 for the same binary.\n";
find_spec += "Invocation is hfind [options] [path ...] [expression]. No paths means start at .\n";
find_spec += "Global options allowed before paths are -H -L -P and --gitignore and --help. -P is default. -L follows symlinks with cycle guards. -H follows only command-line symlinks.\n";
find_spec += "Expression starts at the first operand that is (, !, ,, or a token beginning with -.\n";
find_spec += "If the expression has no action, imply -print. Supported actions are -print and -print0.\n";
find_spec += "Supported tests are -name -iname -path -ipath -regex -iregex -type -size -empty -mtime -mmin -newer -true -false.\n";
find_spec += "-type accepts f d l. -size is [+-]N[cwbkMG] with 512-byte blocks as the default unit like GNU find.\n";
find_spec += "Global walk limits -maxdepth N and -mindepth N work from any position in the expression like GNU find.\n";
find_spec += "Operators are juxtaposition and -a and -and, -o and -or, -not and !, and parentheses. Or is looser than and.\n";
find_spec += "Do not implement -exec -delete -printf -fprintf -ls -quit -fstype or the rest of GNU find.\n";
find_spec += "Exit 0 when the walk finishes without filesystem errors. Exit 1 when any walk error happened. Exit 2 on usage or parse errors.\n";
find_spec += "Print paths the way find does, using the user-supplied root plus a slash plus the relative entry, not a forced absolute path.\n";
find_spec += "Do not sort. Stream matches. Use a 256KiB stdout buffer and a mutex like hgrep.\n";
find_spec += "Speed comes from rayon level-parallel readdir, dirent file type without extra stat when d_type is known, no file-content reads, and cheap glob matching for -name.\n";
find_spec += "Reject unbounded symlink loops. Validate numeric arguments. Do not panic on empty roots, broken symlinks, or unreadable directories.\n";
find_spec += "Aim for 100 percent line coverage of src/bin/hfind/** measured with cargo llvm-cov. Cover aliases, parse errors, every predicate, operators, depths, gitignore, symlink modes, empty trees, and binary-name help.\n";

phase("Survey");
log("Reading hgrep, Cargo.toml, tests, and README before any write");
let survey_prompt = "";
survey_prompt += "You are surveying " + repo + " so a later implementer can add hg and hfind.\n";
survey_prompt += "Use read_file and directory listing tools. Do not answer from memory. Do not edit files.\n";
survey_prompt += "Read Cargo.toml, src/main.rs after_help, src/lib.rs, every file under src/bin/hgrep/, tests/hgrep_test.rs, tests/cli_help_test.rs, README.md companion and install sections, and .github/workflows/static-check.yml.\n";
survey_prompt += "An empty notes field is valid only after those files have been read.\n";
survey_prompt += "Write notes that name actual paths, binary discovery, walk behavior, test helpers, and anything that would break if aliases or a new bin are added.\n";
survey_prompt += facts;
let survey = agent(survey_prompt, #{
    label: "survey-hgrep",
    capability_mode: "read-only",
    output_schema: notes_schema,
});

phase("Spec");
let spec_prompt = "";
spec_prompt += "Write a concrete implementation spec for hg plus hfind/hfd in " + repo + ".\n";
spec_prompt += "Use read_file on the same files the survey named. Do not edit files.\n";
spec_prompt += "Survey output follows.\n";
spec_prompt += encode_out(survey);
spec_prompt += "\n";
spec_prompt += facts;
spec_prompt += find_spec;
spec_prompt += "Your notes must list every file that will be created or changed, the Cargo [[bin]] tables, the expression grammar, the walk rules, the test file plan, and the coverage plan. Do not invent extra flags.\n";
let spec = agent(spec_prompt, #{
    label: "spec-hfind",
    capability_mode: "read-only",
    output_schema: notes_schema,
});

phase("Implement");
let impl_prompt = "";
impl_prompt += "Implement hg and hfind/hfd in " + repo + " in this workspace. Do not use an isolated worktree. Edits must land in the parent tree.\n";
impl_prompt += "Read every file you will change before writing. Run cargo test for the new binaries and the whole suite before you finish. Run cargo fmt. Run clippy with warnings denied.\n";
impl_prompt += "If a test fails, fix it in this same turn. Do not leave a broken tree.\n";
impl_prompt += facts;
impl_prompt += find_spec;
impl_prompt += "Survey notes:\n";
impl_prompt += encode_out(survey);
impl_prompt += "\nSpec notes:\n";
impl_prompt += encode_out(spec);
impl_prompt += "\nFiles you must produce or update include Cargo.toml, src/bin/hfind/*, tests/hfind_test.rs, tests/cli_help_test.rs, README.md, src/main.rs after_help.\n";
impl_prompt += "A valid empty files_written list is not allowed. You must write the alias and the new binary.\n";
let implemented = agent(impl_prompt, #{
    label: "implement-hfind",
    capability_mode: "all",
    output_schema: notes_schema,
});

phase("Review");
let review_base = "";
review_base += "Adversarially review the shipped tree in " + repo + " for the hg alias and hfind/hfd.\n";
review_base += "Use read_file and tests. Do not edit. Do not answer from memory. An empty findings list is valid only after you have read src/bin/hfind/, Cargo.toml, tests/hfind_test.rs, and the alias wiring.\n";
review_base += "Report at most 8 concrete findings as file plus issue. Missing files, missing alias bins, find incompatibilities, panics, DoS, and untested branches all count.\n";
review_base += "Implementer notes:\n";
review_base += encode_out(implemented);
review_base += "\n";
review_base += facts;
review_base += find_spec;

let dimensions = [
    "correctness and find expression parsing",
    "walk semantics, symlinks, gitignore, and DoS",
    "alias wiring, CLI help, README, and clap bins",
    "test gaps versus 100 percent coverage of src/bin/hfind",
];
let review_jobs = [];
let di = 0;
for d in dimensions {
    let p = review_base;
    p += "Focus only on " + d + ".\n";
    review_jobs.push(#{
        prompt: p,
        label: "review-" + di.to_string(),
        capability_mode: "read-only",
        output_schema: findings_schema,
    });
    di += 1;
}
let reviews = parallel(review_jobs);

let findings = [];
for r in reviews {
    let fs = as_findings(r);
    for f in fs {
        if f == () { continue; }
        if f.file == () { continue; }
        if f.issue == () { continue; }
        findings.push(f);
    }
}
if findings.len() > MAX_VERIFY {
    log("Truncated findings from " + findings.len().to_string() + " to " + MAX_VERIFY.to_string());
    let kept = [];
    let k = 0;
    for f in findings {
        if k < MAX_VERIFY { kept.push(f); }
        k += 1;
    }
    findings = kept;
}
log(findings.len().to_string() + " review findings queued for verification");

phase("Verify");
let confirmed = [];
if findings.len() > 0 {
    let vjobs = [];
    let fi = 0;
    for f in findings {
        let vp = "";
        vp += "Adversarially verify this finding against the shipped code in " + repo + ".\n";
        vp += "Read the named file and its callers. Set real true only with concrete evidence you independently inspected. Otherwise real false.\n";
        vp += "Missing evidence means real false. Do not edit files.\n";
        vp += "File: " + json_encode(f.file) + "\n";
        vp += "Issue: " + json_encode(f.issue) + "\n";
        vjobs.push(#{
            prompt: vp,
            label: "verify-" + fi.to_string(),
            capability_mode: "read-only",
            output_schema: verdict_schema,
        });
        fi += 1;
    }
    let verdicts = parallel(vjobs);
    let i = 0;
    for v in verdicts {
        if i < findings.len()
            && v != ()
            && v.success
            && v.output != ()
            && v.output.real == true
            && v.output.evidence != ()
            && v.output.evidence != ""
        {
            confirmed.push(findings[i]);
        }
        i += 1;
    }
}
log(confirmed.len().to_string() + "/" + findings.len().to_string() + " findings survived verification");

phase("Fix");
let fix_prompt = "";
fix_prompt += "Work in " + repo + " in this workspace. Do not use an isolated worktree.\n";
fix_prompt += "Confirmed findings follow. If the list is empty, re-read src/bin/hfind, Cargo.toml, and tests, run cargo test, and only fix compile or test failures you observe.\n";
fix_prompt += "Do not add comments. Do not add Japanese. Keep find compatibility. Re-run cargo fmt, clippy with warnings denied, and cargo test.\n";
fix_prompt += "Confirmed JSON:\n";
fix_prompt += json_encode(confirmed);
fix_prompt += "\n";
fix_prompt += facts;
let fixed = agent(fix_prompt, #{
    label: "fix-confirmed",
    capability_mode: "all",
    output_schema: notes_schema,
});

phase("QA");
let qa_prompt = "";
qa_prompt += "In " + repo + ", run cargo fmt --all -- --check, cargo clippy -- -D warnings, cargo test, and coverage for the hfind binary sources.\n";
qa_prompt += "Use cargo llvm-cov if that binary is installed, scoped so you can name uncovered lines under src/bin/hfind. If llvm-cov cannot run, say so in notes and still run cargo test.\n";
qa_prompt += "Do not edit files. Record fmt_ok, clippy_ok, test_ok, coverage_pct as a number, and uncovered as a compact list of file plus line ranges.\n";
qa_prompt += "coverage_target is " + coverage_target.to_string() + ".\n";
let qa = agent(qa_prompt, #{
    label: "qa-suite",
    capability_mode: "execute",
    output_schema: qa_schema,
});
let pct = qa_pct(qa);
let uncovered = qa_uncovered(qa);

phase("Cover");
let cover_round = 0;
let last_fp = "";
let cover_notes = [];
while cover_round < 2 {
    if pct != () && pct >= coverage_target { break; }
    let fp = fingerprint(uncovered);
    if cover_round > 0 && fp == last_fp {
        log("Coverage stalled");
        break;
    }
    last_fp = fp;
    let cover_prompt = "";
    cover_prompt += "Raise line coverage of src/bin/hfind in " + repo + " toward " + coverage_target.to_string() + " percent.\n";
    cover_prompt += "Read the uncovered lines, add tests in tests/hfind_test.rs, or delete truly unreachable branches. Do not add comments. Do not weaken tests.\n";
    cover_prompt += "Do not drop find compatibility. Run cargo test and cargo fmt. clippy with warnings denied must pass.\n";
    cover_prompt += "Uncovered from QA:\n";
    cover_prompt += uncovered;
    cover_prompt += "\nFix notes:\n";
    cover_prompt += encode_out(fixed);
    let covered = agent(cover_prompt, #{
        label: "cover-" + cover_round.to_string(),
        capability_mode: "all",
        output_schema: notes_schema,
    });
    cover_notes.push(encode_out(covered));
    let qa2_prompt = qa_prompt;
    qa2_prompt += "This is a remeasure after a coverage pass.\n";
    qa = agent(qa2_prompt, #{
        label: "qa-remeasure-" + cover_round.to_string(),
        capability_mode: "execute",
        output_schema: qa_schema,
    });
    pct = qa_pct(qa);
    uncovered = qa_uncovered(qa);
    cover_round += 1;
}

let report = "";
report += "# build-hfind\n\n";
report += "repo " + repo + "\n";
report += "coverage_target " + coverage_target.to_string() + "\n";
if pct != () {
    report += "coverage_pct " + pct.to_string() + "\n";
} else {
    report += "coverage_pct unknown\n";
}
report += "confirmed_findings " + confirmed.len().to_string() + "\n";
report += "cover_rounds " + cover_round.to_string() + "\n\n";
report += "## survey\n" + encode_out(survey) + "\n\n";
report += "## spec\n" + encode_out(spec) + "\n\n";
report += "## implement\n" + encode_out(implemented) + "\n\n";
report += "## fix\n" + encode_out(fixed) + "\n\n";
report += "## qa\n" + encode_out(qa) + "\n\n";
report += "## uncovered\n" + uncovered + "\n";
let path = write_scratch_file("report.md", report);
complete(#{
    path: path,
    report: report,
    coverage_pct: pct,
    confirmed: confirmed.len(),
    cover_rounds: cover_round,
});

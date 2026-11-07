use happy_cracking::crypto::solve::{
    self, SolveAction, SolveOptions, looks_like_flag, score_plaintext,
};
use happy_cracking::crypto::substitution;

#[test]
fn looks_like_flag_detects_flag_brace() {
    assert!(looks_like_flag("flag{hello_world}"));
    assert!(looks_like_flag("FLAG{hello}"));
    assert!(looks_like_flag("CTF{abc}"));
    assert!(looks_like_flag("picoctf{xyz}"));
    assert!(looks_like_flag("HK{x}"));
    assert!(looks_like_flag("hk{x}"));
    assert!(looks_like_flag("xxxx{yyyy}"));
    assert!(looks_like_flag("flag{"));
    assert!(!looks_like_flag("nope"));
    assert!(!looks_like_flag("{x}"));
}

#[test]
fn score_prefers_flag_and_english() {
    let flag = score_plaintext("flag{test}");
    let random = score_plaintext("xqzjw vbnmpl");
    assert!(flag > random);
}

#[test]
fn solve_caesar_recovers_hello() {
    // "Khoor" is "Hello" with shift 3
    let results = solve::solve(
        "Khoor",
        &SolveOptions {
            substitution_iters: 0,
            max_rails: 3,
            aggressive: false,
            max_depth: 2,
        },
    );
    assert!(
        results
            .iter()
            .any(|c| c.plaintext.to_ascii_lowercase().contains("hello")),
        "expected Hello in candidates, got {:?}",
        results
            .iter()
            .map(|c| c.plaintext.clone())
            .take(10)
            .collect::<Vec<_>>()
    );
}

#[test]
fn solve_base64_layer() {
    // "flag{b64}" base64
    let encoded = "ZmxhZ3tiNjR9";
    let results = solve::solve(
        encoded,
        &SolveOptions {
            substitution_iters: 0,
            max_rails: 2,
            aggressive: true,
            max_depth: 3,
        },
    );
    assert!(
        results.iter().any(|c| c.plaintext.contains("flag{")),
        "expected flag in candidates"
    );
}

#[test]
fn solve_empty_input() {
    let results = solve::solve("", &SolveOptions::default());
    assert!(results.is_empty());
}

#[test]
fn solve_run_rejects_excessive_substitution_iters() {
    let action = SolveAction::Run {
        input: "HELLOWORLD".to_string(),
        top: 5,
        max_rails: 3,
        substitution_iters: substitution::MAX_SOLVE_ITERATIONS + 1,
        aggressive: false,
        max_depth: 2,
    };
    let start = std::time::Instant::now();
    let res = solve::run(action);
    assert!(
        res.is_err(),
        "Expected error for excessive substitution_iters"
    );
    let err = res.unwrap_err().to_string();
    assert!(
        err.contains("exceeds the maximum allowed limit"),
        "unexpected error: {err}"
    );
    assert!(
        start.elapsed().as_millis() < 100,
        "Rejection took too long: {:?}",
        start.elapsed()
    );
}

#[test]
fn check_solve_input_len_rejects_oversized() {
    let start = std::time::Instant::now();
    let err = solve::check_solve_input_len("abcd", 2).unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
    assert!(
        start.elapsed().as_millis() < 100,
        "oversized solve input must be rejected before cipher trials"
    );
}

#[test]
fn check_solve_input_len_accepts_len_at_limit() {
    solve::check_solve_input_len("ab", 2).unwrap();
}

#[test]
fn check_solve_input_len_accepts_empty() {
    solve::check_solve_input_len("", 16).unwrap();
}

#[test]
fn run_rejects_oversized_solve_input() {
    let start = std::time::Instant::now();
    let err = solve::run(SolveAction::Run {
        input: "a".repeat(solve::MAX_SOLVE_BYTES + 1),
        top: 5,
        max_rails: 3,
        substitution_iters: 0,
        aggressive: false,
        max_depth: 2,
    })
    .unwrap_err()
    .to_string();
    assert!(err.contains("Denial of Service"), "unexpected error: {err}");
    assert!(
        start.elapsed().as_millis() < 100,
        "oversized solve CLI input must be rejected before cipher trials"
    );
}

#[test]
fn run_solves_under_limit() {
    solve::run(SolveAction::Run {
        input: "Khoor".into(),
        top: 5,
        max_rails: 3,
        substitution_iters: 0,
        aggressive: false,
        max_depth: 2,
    })
    .unwrap();
}

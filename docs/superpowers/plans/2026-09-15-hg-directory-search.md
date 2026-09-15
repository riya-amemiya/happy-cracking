# hg directory search Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `hg` recursively search directory operands (and `.` on a TTY) with gitignore on by default, while leaving `hgrep` grep-compatible and staying faster than `rg`.

**Architecture:** After clap parse, `apply_search_defaults` inspects argv0. For `hg`, empty TTY operands become `.`, directory operands set `recursive`, and tree walks set `gitignore` unless `--no-ignore`. The existing parallel walker is unchanged.

**Tech Stack:** Rust 2024, clap, existing `hgrep` walker / gitignore.

## Global Constraints

- `hgrep` directory-without-`-r` still errors `Is a directory` and exits 2.
- `--gitignore` still requires `--recursive` at the clap layer.
- Do not skip hidden files.
- Release `hg PATTERN .` must be faster than default `rg` on this repo.

---

### Task 1: Failing hg directory-search tests

**Files:**
- Create: `tests/hg_test.rs`
- Modify: `src/hgrep/cli.rs` (flag only after tests fail for missing `--no-ignore` in help, later task)

**Interfaces:**
- Consumes: `CARGO_BIN_EXE_hg`, scratch-tree helpers copied from `tests/hgrep_test.rs` patterns
- Produces: failing tests for recurse-without-`-r`, filename prefix, gitignore default, `--no-ignore`

- [ ] **Step 1: Write the failing tests** in `tests/hg_test.rs`
- [ ] **Step 2: Run `cargo test --test hg_test -- --nocapture` and confirm they fail** because directories still error
- [ ] **Step 3: Implement defaults + `--no-ignore`**
- [ ] **Step 4: Re-run tests; they pass**
- [ ] **Step 5: Commit**

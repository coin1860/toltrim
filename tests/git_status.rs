//! What an agent reads after `git status`, `git log`, and `git branch`.
//!
//! Every fixture here was recorded from a real git repository (see the command
//! comment above each one); nothing is hand-written. Each test asserts in both
//! directions: Evidence must survive and Noise must not. A test that only
//! asserted "the output got smaller" would pass while the rule quietly deleted
//! the branch the agent is on or the one file it was about to commit.
//!
//! All assertions go through the pure render seam, so no process is started.

use std::fs;

use toltrim::trim::{self, Tool};

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"))
}

/// Read a fixture through the seam, exactly as the entry point does.
fn render(tool: Tool, args: &[&str], raw: &str, code: i32, preserve: bool) -> String {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let bytes = trim::render(tool, &args, raw.as_bytes(), code, preserve);
    String::from_utf8(bytes).expect("fixtures are valid UTF-8")
}

// Recorded in a throwaway repository at /Users/shaneshou/Dev/qwen-wt/scratch-t05:
//   git status --untracked-files=all > git-status-large-untracked.txt
// `-uall` is what makes git print one line per file under `target/`; the fixture
// is the flood an agent pays for when untracked files are listed individually.
#[test]
fn an_untracked_directory_collapses_to_one_counted_line() {
    let raw = fixture("git-status-large-untracked.txt");
    let out = render(Tool::Git, &["status"], &raw, 0, false);

    // Evidence: the section headers that tell an agent what kind of change each
    // path is, so staged, unstaged, and untracked stay distinguishable.
    assert!(
        out.contains("Changes to be committed:"),
        "the staged section header was trimmed away:\n{out}"
    );
    assert!(
        out.contains("Changes not staged for commit:"),
        "the unstaged section header was trimmed away:\n{out}"
    );
    assert!(
        out.contains("Untracked files:"),
        "the untracked section header was trimmed away:\n{out}"
    );

    // Evidence: every changed file and its status code.
    assert!(out.contains("modified:   staged.txt"), "got:\n{out}");
    assert!(out.contains("modified:   unstaged.txt"), "got:\n{out}");

    // Evidence: the single untracked file is neither collapsed nor counted.
    assert!(
        out.contains("\tsolo.txt\n"),
        "the single untracked file was lost or miscounted:\n{out}"
    );

    // Evidence: the untracked directory, named, with the number of files it
    // swallowed — one line instead of 130.
    assert!(
        out.contains("target/ (130 files)"),
        "the untracked directory did not collapse to a counted line:\n{out}"
    );

    // Noise: the per-file listing inside the untracked directory.
    assert!(
        !out.contains("target/classes/Foo.class"),
        "the untracked flood survived:\n{out}"
    );
    assert!(
        !out.contains("target/classes/pkg/Cls100.java"),
        "an inner path survived the collapse:\n{out}"
    );

    // Noise: git's advice, including the trailing "no changes added" line.
    assert!(!out.contains("(use \"git"), "advice lines survived:\n{out}");
    assert!(
        !out.contains("no changes added to commit"),
        "the trailing advice line survived:\n{out}"
    );

    // The whole point: a repository with a large build directory no longer costs
    // hundreds of lines.
    assert!(
        out.len() * 10 < raw.len(),
        "expected a large saving, got {} -> {} bytes",
        raw.len(),
        out.len()
    );
}

// Recorded in the same throwaway repository:
//   git log > git-log.txt
#[test]
fn a_log_keeps_hash_author_date_and_subject() {
    let raw = fixture("git-log.txt");
    let out = render(Tool::Git, &["log"], &raw, 0, false);

    // Evidence: the newest commit, whole: hash, author, date, subject.
    assert!(
        out.contains("4cbcb58f0eb38faae723f4921967bfb561adab46"),
        "the commit hash was trimmed away:\n{out}"
    );
    assert!(
        out.contains("Alice Example"),
        "the author was trimmed away:\n{out}"
    );
    assert!(
        out.contains("2024-03-20"),
        "the date was trimmed away:\n{out}"
    );
    assert!(
        out.contains("Rename the module"),
        "the subject was trimmed away:\n{out}"
    );

    // Evidence: the other commits are still findable.
    assert!(
        out.contains("13c7d32abcfbc99d899acff659f3c0c3e0806c1c"),
        "got:\n{out}"
    );
    assert!(out.contains("2024-02-14"));
    assert!(out.contains("Fix the parser bug"));
    assert!(out.contains("4a9a39cbaf8803d36156831149c34a023731fd6e"));
    assert!(out.contains("2024-01-02"));
    assert!(out.contains("Add the first file"));

    // Noise: the commit body, and the field labels that only make sense to a
    // reader of the block form.
    assert!(
        !out.contains("The parser dropped the trailing token"),
        "the commit body survived:\n{out}"
    );
    assert!(
        !out.contains("without a newline"),
        "a multi-line body line survived:\n{out}"
    );
    assert!(
        !out.contains("Author:"),
        "the Author: label survived:\n{out}"
    );
    assert!(!out.contains("Date:"), "the Date: label survived:\n{out}");
}

/// Build a default-shaped `git log` with `n` commits, newest first. Invented
/// input is fine here: the recorded fixtures carry the fidelity check, and this
/// one only has to be long enough to trip the twenty-commit cap.
fn synthetic_log(n: usize) -> String {
    let mut out = String::new();
    for i in 0..n {
        out.push_str(&format!(
            "commit {hash}\nAuthor: Alice Example <alice@example.com>\n\
             Date:   Tue Jan 2 09:15:00 2024 +0000\n\n    Subject {i}\n\n",
            hash = synthetic_hash(i),
        ));
    }
    out
}

fn synthetic_hash(i: usize) -> String {
    format!("{i:040x}")
}

#[test]
fn a_log_defaults_to_the_most_recent_twenty_commits() {
    let raw = synthetic_log(25);
    let out = render(Tool::Git, &["log"], &raw, 0, false);

    // The most recent twenty are the ones an agent would ask for by default.
    for i in 0..20 {
        assert!(
            out.contains(&synthetic_hash(i)),
            "commit {i} should have been kept:\n{out}"
        );
    }
    // The older five are omitted, and the omission is stated rather than silent.
    for i in 20..25 {
        assert!(
            !out.contains(&synthetic_hash(i)),
            "commit {i} should have been omitted:\n{out}"
        );
    }
    assert!(
        out.contains("5 older commits omitted"),
        "the omission was not stated:\n{out}"
    );
}

// Recorded in the throwaway repository:
//   git branch > git-branch.txt
#[test]
fn a_branch_listing_is_names_only_with_the_current_branch_marked() {
    let raw = fixture("git-branch.txt");
    let out = render(Tool::Git, &["branch"], &raw, 0, false);

    // Evidence: every branch name, in the order git printed them.
    assert!(
        out.contains("feature/parser"),
        "a branch name was lost:\n{out}"
    );
    assert!(
        out.contains("release-1.0"),
        "a branch name was lost:\n{out}"
    );
    // Evidence: git's own marker says which one is checked out.
    assert!(
        out.contains("* main\n"),
        "the current branch is not marked:\n{out}"
    );
}

// Recorded in the throwaway repository after configuring upstreams:
//   git branch -v > git-branch-verbose.txt
// Plain `git branch` never prints `[ahead …]`; `-v` does, and that decoration
// must go while the names stay.
#[test]
fn branch_decoration_is_dropped() {
    let raw = fixture("git-branch-verbose.txt");
    let out = render(Tool::Git, &["branch", "-v"], &raw, 0, false);

    assert!(
        out.contains("feature/parser"),
        "a branch name was lost:\n{out}"
    );
    assert!(
        out.contains("release-1.0"),
        "a branch name was lost:\n{out}"
    );
    assert!(
        out.contains("* main"),
        "the current branch is not marked:\n{out}"
    );

    assert!(!out.contains("[ahead"), "ahead decoration survived:\n{out}");
    assert!(
        !out.contains("[behind"),
        "behind decoration survived:\n{out}"
    );
    assert!(
        !out.contains("Work on the parser"),
        "the verbose subject survived:\n{out}"
    );
    assert!(
        !out.contains("0fec132"),
        "the verbose hash survived:\n{out}"
    );
}

// Recorded in the throwaway repository:
//   git status --porcelain > git-status-porcelain.txt
#[test]
fn a_machine_readable_status_keeps_its_bytes_and_only_loses_ansi() {
    let raw = fixture("git-status-porcelain.txt");
    // The caller asked for a parseable format, so the seam must only strip ANSI.
    let bytes = trim::render(
        Tool::Git,
        &["status".to_string(), "--porcelain".to_string()],
        raw.as_bytes(),
        0,
        true,
    );
    assert_eq!(
        String::from_utf8(bytes).unwrap(),
        raw,
        "a porcelain status must arrive byte-identical"
    );

    // Now say the same thing with colour on: the codes go, nothing else does.
    let coloured = format!("\u{1b}[32m{raw}\u{1b}[0m");
    let stripped = trim::render(
        Tool::Git,
        &["status".to_string(), "--porcelain".to_string()],
        coloured.as_bytes(),
        0,
        true,
    );
    assert_eq!(
        String::from_utf8(stripped).unwrap(),
        raw,
        "only ANSI may be removed from a Machine-Readable Request"
    );

    // The preservation decision is itself a pure function of the seam's inputs.
    let args = vec!["status".to_string(), "--porcelain".to_string()];
    assert!(
        trim::should_preserve(false, Tool::Git, &args, false, false),
        "--porcelain is a Machine-Readable Request"
    );
}

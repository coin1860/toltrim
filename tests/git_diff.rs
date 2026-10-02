//! Ticket 04: `git diff` and `git show` keep the patch and stop costing
//! metadata. This is the command where a Trim must never hide a change, so every
//! fixture gets both kinds of assertion: the changed lines must survive, and the
//! per-file metadata must not.
//!
//! Fixtures are recorded from real `git`, in a throwaway repository created
//! outside this one:
//!
//! * `git-diff-metadata.txt` — `git diff -M --cached` after staging a rename
//!   with an edit (`git mv old-name.txt new-name.txt`, then a change), a mode
//!   change (`chmod +x script.sh`), an edit, and an edit to a file whose last
//!   line has no trailing newline.
//! * `git-show-commit.txt` — `git show -M HEAD` of the commit made from those
//!   staged changes.
//!
//! Everything here goes through the pure render seam, so no process is started.

use std::fs;

use toltrim::trim::{self, Tool};

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"))
}

/// Read the fixture through `render`, exactly as the entry point does.
fn render(tool: Tool, args: &[&str], raw: &str, code: i32, preserve: bool) -> String {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let bytes = trim::render(tool, &args, raw.as_bytes(), code, preserve);
    String::from_utf8(bytes).expect("fixtures are valid UTF-8")
}

/// The number of file headers a stream carries: one per changed file.
fn file_headers(text: &str) -> usize {
    text.lines()
        .filter(|l| l.starts_with("diff --git "))
        .count()
}

/// The hunk headers a stream carries, verbatim, in order.
fn hunk_headers(text: &str) -> Vec<&str> {
    text.lines().filter(|l| l.starts_with("@@")).collect()
}

#[test]
fn a_recorded_diff_keeps_the_changed_lines_and_drops_the_metadata() {
    let raw = fixture("git-diff-metadata.txt");
    let out = render(Tool::Git, &["diff"], &raw, 1, false);

    // Evidence: the changed lines themselves, in both directions.
    assert!(
        out.contains("+modify line 6 CHANGED"),
        "the added line was trimmed away:\n{out}"
    );
    assert!(
        out.contains("-modify line 6\n"),
        "the removed line was trimmed away:\n{out}"
    );
    assert!(out.contains("+gamma CHANGED"), "got:\n{out}");
    assert!(out.contains("-gamma\n"), "got:\n{out}");
    assert!(
        out.contains("+last line CHANGED without newline"),
        "the change to the file with no trailing newline was trimmed away:\n{out}"
    );
    assert!(out.contains("-last line without newline\n"), "got:\n{out}");

    // Noise: per-file metadata an agent cannot act on.
    assert!(!out.contains("index "), "index lines leaked:\n{out}");
    assert!(!out.contains("old mode "), "mode lines leaked:\n{out}");
    assert!(!out.contains("new mode "), "mode lines leaked:\n{out}");
    assert!(
        !out.contains("similarity index"),
        "similarity percentages leaked:\n{out}"
    );
    assert!(
        !out.contains("rename from "),
        "rename metadata leaked:\n{out}"
    );
    assert!(
        !out.contains("rename to "),
        "rename metadata leaked:\n{out}"
    );
    assert!(
        !out.contains("\\ No newline at end of file"),
        "the no-newline marker leaked:\n{out}"
    );
}

#[test]
fn context_is_reduced_from_three_lines_to_one() {
    let raw = fixture("git-diff-metadata.txt");
    let out = render(Tool::Git, &["diff"], &raw, 1, false);

    // Recorded git context for the `modify.txt` hunk is lines 3..9 around the
    // change on line 6. One line of context either side must remain…
    assert!(
        out.contains(" modify line 5\n"),
        "the context line adjacent to the change was dropped:\n{out}"
    );
    assert!(
        out.contains(" modify line 7\n"),
        "the context line adjacent to the change was dropped:\n{out}"
    );
    // …and the context three lines away must be gone.
    assert!(
        !out.contains(" modify line 3\n"),
        "three-line context survived:\n{out}"
    );
    assert!(
        !out.contains(" modify line 9\n"),
        "three-line context survived:\n{out}"
    );
    // The recorded fixture really does carry three lines on each side, so the
    // assertions above are about reduction and not about a short hunk.
    assert!(raw.contains("\n modify line 3\n") && raw.contains("\n modify line 4\n"));
    assert!(raw.contains("\n modify line 8\n") && raw.contains("\n modify line 9\n"));
}

#[test]
fn every_changed_file_and_every_hunk_survives() {
    let raw = fixture("git-diff-metadata.txt");
    let out = render(Tool::Git, &["diff"], &raw, 1, false);

    // The recorded fixture changed four files: an edit, a rename-with-edit, an
    // edit with a missing trailing newline, and a mode-only change.
    assert_eq!(file_headers(&raw), 4, "fixture drift: {raw}");
    assert_eq!(
        file_headers(&out),
        file_headers(&raw),
        "a changed file was dropped:\n{out}"
    );
    for header in [
        "diff --git a/modify.txt b/modify.txt",
        "diff --git a/old-name.txt b/new-name.txt",
        "diff --git a/nonewline.txt b/nonewline.txt",
        "diff --git a/script.sh b/script.sh",
    ] {
        assert!(out.contains(header), "{header} was dropped:\n{out}");
    }

    // No hunk header may go missing, whatever the context rule keeps.
    let (raw_hunks, out_hunks) = (hunk_headers(&raw), hunk_headers(&out));
    assert_eq!(raw_hunks.len(), 3, "fixture drift: {raw}");
    for hunk in &raw_hunks {
        assert!(out.contains(hunk), "hunk header {hunk} was dropped:\n{out}");
    }
    assert_eq!(out_hunks.len(), raw_hunks.len());
}

#[test]
fn a_renamed_file_is_still_identifiable() {
    let raw = fixture("git-diff-metadata.txt");
    let out = render(Tool::Git, &["diff"], &raw, 1, false);

    // The rename metadata is gone, but the file header names both paths and the
    // `---`/`+++` pair still says the old name became the new name.
    assert!(
        out.contains("diff --git a/old-name.txt b/new-name.txt"),
        "got:\n{out}"
    );
    assert!(out.contains("--- a/old-name.txt"), "got:\n{out}");
    assert!(out.contains("+++ b/new-name.txt"), "got:\n{out}");
    assert!(!out.contains("rename from"), "got:\n{out}");
    assert!(!out.contains("rename to"), "got:\n{out}");
    assert!(!out.contains("similarity index"), "got:\n{out}");
}

#[test]
fn a_machine_readable_request_is_preserved() {
    // A `--stat`/`--name-only`/`--name-status`/`--porcelain` diff is a
    // Machine-Readable Request: only ANSI may be removed, nothing else.
    let raw = fixture("git-diff-metadata.txt");
    let coloured = format!("\u{1b}[32m{raw}\u{1b}[0m");
    for flag in ["--stat", "--name-only", "--name-status", "--porcelain"] {
        let args = vec!["diff".to_string(), flag.to_string()];
        let preserve = trim::should_preserve(false, Tool::Git, &args, false, false);
        assert!(preserve, "{flag} is a Machine-Readable Request");

        let bytes = trim::render(Tool::Git, &args, coloured.as_bytes(), 0, preserve);
        let out = String::from_utf8(bytes).expect("the fixture is valid UTF-8");

        assert_eq!(
            out,
            toltrim::ansi::strip(&coloured),
            "{flag} was not preserved whole"
        );
        assert!(
            !out.contains('\u{1b}'),
            "ANSI survived a preserved Invocation"
        );
        // Nothing was removed, metadata included: the caller asked for it.
        assert!(
            out.contains("index 6d7ff92..fb5b8b6 100644"),
            "{flag} lost a line"
        );
        assert!(out.contains("similarity index 54%"), "{flag} lost a line");
        assert!(
            out.contains("\\ No newline at end of file"),
            "{flag} lost a line"
        );
    }
}

#[test]
fn the_exit_code_is_never_reinterpreted() {
    let raw = fixture("git-diff-metadata.txt");

    // `git diff` exits 1 when there are differences; `git show` exits 0. Neither
    // number may change what is trimmed.
    let differences = render(Tool::Git, &["diff"], &raw, 1, false);
    let clean = render(Tool::Git, &["diff"], &raw, 0, false);
    assert_eq!(
        differences, clean,
        "the exit code changed the trimmed output"
    );
    assert!(
        differences.contains("+modify line 6 CHANGED"),
        "got:\n{differences}"
    );
    assert!(!differences.contains("index "), "got:\n{differences}");
}

#[test]
fn a_recorded_git_show_is_trimmed_the_same_way() {
    let raw = fixture("git-show-commit.txt");
    let out = render(Tool::Git, &["show"], &raw, 0, false);

    // The commit the reader asked to inspect is still identifiable.
    let commit_line = raw.lines().next().expect("a recorded commit");
    assert!(
        commit_line.starts_with("commit "),
        "fixture drift: {commit_line}"
    );
    assert!(
        out.contains(commit_line),
        "the commit line was dropped:\n{out}"
    );
    assert!(
        out.contains(
            "    fixture change with a rename, a mode change, and a missing trailing newline"
        ),
        "the subject was dropped:\n{out}"
    );

    // The same patch rules apply: changes survive, metadata and three-line
    // context do not.
    assert!(out.contains("+modify line 6 CHANGED"), "got:\n{out}");
    assert!(out.contains("-gamma\n"), "got:\n{out}");
    assert!(!out.contains("\\ No newline at end of file"), "got:\n{out}");
    assert!(!out.contains("similarity index"), "got:\n{out}");
    assert!(out.contains(" modify line 5\n"), "got:\n{out}");
    assert!(!out.contains(" modify line 3\n"), "got:\n{out}");
}

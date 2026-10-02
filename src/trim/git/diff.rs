//! `git diff` and `git show`.
//!
//! Ticket 04 owns this rule set. The patch is Evidence: every changed file and
//! every hunk survives, with no per-file truncation and no dropped hunk. The
//! per-file metadata is Noise — hashes, modes, similarity percentages, rename and
//! copy announcements, and the "no newline at end of file" marker — and so is the
//! third line of context either side of a change.
//!
//! `git show` reaches the same rule set because its patch is a diff; the commit
//! header that precedes it carries no per-file metadata and is left alone, so the
//! commit the caller asked to inspect stays identifiable.

/// The marker git writes after a last line that has no trailing newline.
const NO_NEWLINE: &str = "\\ No newline at end of file";

/// `text` has already had ANSI removed and is valid UTF-8. `exit_code` is the
/// child's verbatim and is deliberately not consulted: `git diff` can exit 1
/// when there are differences while `git show` exits 0, and reinterpreting
/// either would turn "there are changes" into "the command failed" (ADR-0003).
pub fn render(text: &str, _exit_code: i32) -> String {
    let ends_with_newline = text.ends_with('\n');
    let mut out = String::with_capacity(text.len());
    // Inside a file section, before its first hunk: where per-file metadata
    // lives. The `---`/`+++` pair and a binary-diff notice are in this section
    // too, and they are kept.
    let mut in_file_header = false;
    let mut hunk: Option<Hunk<'_>> = None;

    for raw in text.split_inclusive('\n') {
        let line = logical(raw);
        if line.starts_with("@@") {
            flush(&mut out, hunk.take());
            hunk = Some(Hunk::new(raw));
            in_file_header = false;
        } else if line.starts_with("diff --") {
            flush(&mut out, hunk.take());
            out.push_str(raw);
            in_file_header = true;
        } else if line == NO_NEWLINE {
            // Noise: the marker describes an absent newline, not the change.
        } else if in_file_header && is_file_metadata(line) {
            // Noise: a hash, a mode, or a similarity percentage.
        } else if let Some(hunk) = hunk.as_mut() {
            hunk.body.push(raw);
        } else {
            // Not in a hunk: a file header (`---`/`+++`), a binary-diff notice,
            // or, for `git show`, the commit header itself.
            out.push_str(raw);
        }
    }
    flush(&mut out, hunk.take());

    if !ends_with_newline && out.ends_with('\n') {
        // The terminator-less last line of the input was dropped; do not hand
        // back a newline the input never had.
        out.truncate(out.len() - if out.ends_with("\r\n") { 2 } else { 1 });
    }
    out
}

/// One `@@` hunk: its header, which is always kept, and the body lines that
/// follow it.
struct Hunk<'a> {
    header: &'a str,
    body: Vec<&'a str>,
    /// A combined diff (`@@@ … @@@`, `git diff --cc`) has two prefix columns, so
    /// a body line that looks like context can still carry a change; nothing in
    /// such a hunk is dropped.
    combined: bool,
}

impl<'a> Hunk<'a> {
    fn new(header: &'a str) -> Self {
        Self {
            header,
            body: Vec::new(),
            combined: logical(header).starts_with("@@@"),
        }
    }
}

/// Emit a hunk: the header, every change, and only the context lines adjacent to
/// a change — three lines of context become one on each side of a change run. A
/// hunk with no change at all keeps its header; no hunk is ever dropped.
fn flush(out: &mut String, hunk: Option<Hunk<'_>>) {
    let Some(hunk) = hunk else {
        return;
    };
    out.push_str(hunk.header);
    if hunk.combined {
        for raw in hunk.body {
            out.push_str(raw);
        }
        return;
    }
    for (i, raw) in hunk.body.iter().enumerate() {
        let changed_before = i
            .checked_sub(1)
            .and_then(|j| hunk.body.get(j).copied())
            .is_some_and(is_change);
        let changed_after = hunk.body.get(i + 1).copied().is_some_and(is_change);
        // A change is kept wherever it sits; a context line is kept only as the
        // neighbour of one; anything unrecognised is kept rather than risk
        // dropping Evidence.
        if !logical(raw).starts_with(' ') || changed_before || changed_after {
            out.push_str(raw);
        }
    }
}

/// A line without its terminator, so markers match on CRLF input too.
fn logical(raw: &str) -> &str {
    let body = raw.strip_suffix('\n').unwrap_or(raw);
    body.strip_suffix('\r').unwrap_or(body)
}

/// Is this line part of the change rather than its surroundings?
fn is_change(raw: &str) -> bool {
    let line = logical(raw);
    line.starts_with('+') || line.starts_with('-')
}

/// Per-file metadata that names a hash, a mode, or a similarity percentage:
/// never the change itself. Only ever dropped in a file's header, before its
/// hunks, so a commit message in `git show` cannot be mistaken for it.
fn is_file_metadata(line: &str) -> bool {
    line.starts_with("index ")
        || line.starts_with("old mode ")
        || line.starts_with("new mode ")
        || line.starts_with("similarity index ")
        || line.starts_with("dissimilarity index ")
        || line.starts_with("rename from ")
        || line.starts_with("rename to ")
        || line.starts_with("copy from ")
        || line.starts_with("copy to ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_stays_empty() {
        assert_eq!(render("", 0), "");
    }

    #[test]
    fn a_file_with_no_hunks_keeps_its_header_and_loses_only_metadata() {
        // A mode-only change: the file must still be visible as changed.
        let text = "diff --git a/s.sh b/s.sh\nold mode 100644\nnew mode 100755\n";
        assert_eq!(render(text, 0), "diff --git a/s.sh b/s.sh\n");
    }

    #[test]
    fn a_hunk_with_no_changes_keeps_its_header() {
        // Nothing here is a change, so nothing is a context line next to one;
        // the hunk still must not vanish.
        let text = "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1,3 +1,3 @@\n one\n two\n three\n";
        assert_eq!(
            render(text, 0),
            "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1,3 +1,3 @@\n"
        );
    }

    #[test]
    fn context_more_than_one_line_from_a_change_is_dropped() {
        let text = concat!(
            "diff --git a/x b/x\n",
            "--- a/x\n",
            "+++ b/x\n",
            "@@ -1,7 +1,7 @@\n",
            " far-above\n",
            " two-above\n",
            " one-above\n",
            "-old\n",
            "+new\n",
            " one-below\n",
            " two-below\n",
            " far-below\n",
        );
        let out = render(text, 0);
        assert!(out.contains(" one-above\n"), "got:\n{out}");
        assert!(out.contains(" one-below\n"), "got:\n{out}");
        assert!(out.contains("-old\n+new\n"), "got:\n{out}");
        for dropped in [
            " far-above\n",
            " two-above\n",
            " two-below\n",
            " far-below\n",
        ] {
            assert!(!out.contains(dropped), "{dropped:?} survived:\n{out}");
        }
    }

    #[test]
    fn crlf_input_matches_markers_through_a_carriage_return_and_keeps_its_endings() {
        let text = concat!(
            "diff --git a/x b/x\r\n",
            "index 1111111..2222222 100644\r\n",
            "--- a/x\r\n",
            "+++ b/x\r\n",
            "@@ -1,3 +1,3 @@\r\n",
            " far-above\r\n",
            " one-above\r\n",
            "-old\r\n",
            "+new\r\n",
            "\\ No newline at end of file\r\n",
        );
        assert_eq!(
            render(text, 0),
            "diff --git a/x b/x\r\n--- a/x\r\n+++ b/x\r\n@@ -1,3 +1,3 @@\r\n one-above\r\n-old\r\n+new\r\n"
        );
    }

    #[test]
    fn a_missing_final_newline_is_not_invented() {
        // The input's last line has no terminator and is dropped; the trimmed
        // text must not gain one either.
        let text = "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1,1 +1,1 @@\n-old\n+new\n\\ No newline at end of file";
        assert_eq!(
            render(text, 0),
            "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1,1 +1,1 @@\n-old\n+new"
        );
    }

    #[test]
    fn a_combined_diff_hunk_keeps_every_body_line() {
        // `git diff --cc` body lines carry two prefix columns, so a line that
        // looks like context may still be a change.
        let text = "diff --cc x\n@@@ -1,3 -1,3 +1,4 @@@\n  both\n++added in both\n +added in one\n";
        assert_eq!(render(text, 0), text);
    }

    #[test]
    fn the_exit_code_does_not_change_the_result() {
        let text = "diff --git a/x b/x\n@@ -1,3 +1,3 @@\n a\n b\n-old\n+new\n c\n d\n";
        assert_eq!(render(text, 1), render(text, 0));
        assert_eq!(render(text, 1), render(text, -9));
    }
}

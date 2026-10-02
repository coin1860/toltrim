//! `git diff` and `git show`.
//!
//! Ticket 04 owns this rule set: reduce context to one line, drop per-file
//! metadata, keep every changed file and every hunk. Until then the rule is the
//! identity, which is the pre-seam behaviour.

/// `text` has already had ANSI removed and is valid UTF-8. `exit_code` is the
/// child's; `git diff` exits 1 when there are differences.
pub fn render(text: &str, _exit_code: i32) -> String {
    text.to_string()
}

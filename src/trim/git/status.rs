//! `git status`, `git log`, and `git branch`.
//!
//! Ticket 05 owns these rule sets: collapse an untracked directory's files into
//! one counted line, reduce history to hash/author/date/subject, list branch
//! names only. Until then each rule is the identity, which is the pre-seam
//! behaviour.

/// `git status`. `text` has already had ANSI removed and is valid UTF-8.
pub fn render(text: &str, _exit_code: i32) -> String {
    text.to_string()
}

/// `git log`.
pub fn render_log(text: &str, _exit_code: i32) -> String {
    text.to_string()
}

/// `git branch`.
pub fn render_branch(text: &str, _exit_code: i32) -> String {
    text.to_string()
}

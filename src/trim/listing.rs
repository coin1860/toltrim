//! `ls`, `dir`, and `tree` share one rule set.
//!
//! Ticket 06 owns this rule set: collapse noise directories to a visible count,
//! group entries by kind, truncate long lines. Until then the rule is the
//! identity, which is the pre-seam behaviour.

/// `text` has already had ANSI removed and is valid UTF-8.
pub fn render(text: &str) -> String {
    text.to_string()
}

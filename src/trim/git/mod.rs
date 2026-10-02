//! git trimming, dispatched by subcommand.
//!
//! `diff`/`show` and `status`/`log`/`branch` are separate rule sets in separate
//! modules; this file only decides which one an Invocation reaches. Every other
//! subcommand is a Passthrough Command (ADR-0008) and is handed back unchanged.

pub mod diff;
pub mod status;

/// The git subcommand an Invocation names, skipping the global options that may
/// precede it (`-C <path>`, `-c <name=value>`, `--no-pager`, …).
pub fn subcommand(args: &[String]) -> Option<String> {
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        if arg == "-C" || arg == "-c" {
            // Options that take a separate value; the value is not the subcommand.
            rest.next();
            continue;
        }
        if arg.starts_with('-') {
            continue;
        }
        return Some(arg.clone());
    }
    None
}

/// Apply the rule set for this subcommand. `exit_code` is the child's, verbatim:
/// `git diff` exits 1 when there are differences, and no rule may reinterpret it.
pub fn render(text: &str, exit_code: i32, args: &[String]) -> String {
    match subcommand(args).as_deref() {
        Some("diff" | "show") => diff::render(text, exit_code),
        Some("status") => status::render(text, exit_code),
        Some("log") => status::render_log(text, exit_code),
        Some("branch") => status::render_branch(text, exit_code),
        // A Passthrough Command keeps its bytes exactly.
        _ => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn subcommand_is_found_past_path_options() {
        assert_eq!(
            subcommand(&args(&["-C", "repo", "diff"])),
            Some("diff".into())
        );
        assert_eq!(
            subcommand(&args(&["--no-pager", "log"])),
            Some("log".into())
        );
        assert_eq!(subcommand(&args(&["status"])), Some("status".into()));
        assert_eq!(subcommand(&args(&[])), None);
    }

    #[test]
    fn unproxied_subcommands_keep_their_bytes() {
        let text = "raw commit output\n";
        assert_eq!(render(text, 0, &args(&["commit", "-m", "x"])), text);
    }
}

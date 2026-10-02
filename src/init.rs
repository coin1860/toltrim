//! `toltrim init`: write the command mapping into the project's `AGENTS.md`.
//!
//! Ticket 02 owns this module. The interface `main.rs` relies on is [`run`]: it
//! implements the `toltrim init` verb and returns the process exit code.
//!
//! The mapping teaches an agent the explicit prefix (ADR-0002) for the Proxied
//! Commands in scope (ADR-0008). It is delimited by HTML comments so that a
//! re-run finds the block anywhere in the file and replaces it in place, which
//! makes init idempotent at the byte level. A preview mode prints the file that
//! would result and writes nothing. Nothing outside the project is ever
//! touched: the only path this module writes is `<project root>/AGENTS.md`.

use std::io::Write;
use std::path::{Path, PathBuf};

/// The file the mapping is written into, in the project root.
const FILE_NAME: &str = "AGENTS.md";

/// The block delimiters. An agent reads them as HTML comments and ignores them;
/// init uses them to find and replace its own block.
const BEGIN: &str = "<!-- toltrim:begin -->";
const END: &str = "<!-- toltrim:end -->";

/// The mapping an agent working in the project must see. It is written with
/// `\n` line endings; [`restyle`] gives a CRLF project a CRLF block.
const MAPPING: &str = "\
<!-- toltrim:begin -->
## Run commands with the toltrim prefix

Prefix these commands with `toltrim` so their output is trimmed to the evidence
you need to act, and nothing else:

- `toltrim mvn test`
- `toltrim mvn verify`
- `toltrim mvn package`
- `toltrim mvn compile`
- `toltrim mvn install`
- `toltrim mvn clean`
- `toltrim mvn test-compile`
- `toltrim mvn integration-test`
- `toltrim git status`
- `toltrim git diff`
- `toltrim git log`
- `toltrim git show`
- `toltrim git branch`
- `toltrim ls`
- `toltrim dir`
- `toltrim tree`

Every other command runs unchanged: `toltrim <command> <args...>` produces the
same bytes and the same exit code as running `<command> <args...>` alone.
<!-- toltrim:end -->";

/// The `toltrim init` verb. `args` is everything after `init`. Returns the
/// process exit code.
pub fn run(args: &[String]) -> i32 {
    let mut dry_run = false;
    for arg in args {
        match arg.as_str() {
            "--dry-run" => dry_run = true,
            other => {
                eprintln!(
                    "toltrim init: unknown argument `{other}`; usage: toltrim init [--dry-run]"
                );
                return 2;
            }
        }
    }
    let root = match std::env::current_dir() {
        Ok(root) => root,
        Err(error) => {
            eprintln!("toltrim init: cannot determine the project root: {error}");
            return 1;
        }
    };
    if let Err(error) = init_in(&root, dry_run) {
        eprintln!("toltrim init: {error}");
        return 1;
    }
    0
}

/// What init is about to do, composed before anything is written.
struct Plan {
    path: PathBuf,
    /// The file as it is now; `None` when `AGENTS.md` does not exist.
    existing: Option<Vec<u8>>,
    /// The complete new file.
    content: String,
}

/// Write the mapping into `root/AGENTS.md`, or print the file that would result
/// when `dry_run`. The whole new content is built in memory first, so a failure
/// leaves the file — and everything else on disk — exactly as it was.
fn init_in(root: &Path, dry_run: bool) -> Result<(), String> {
    let plan = plan(root)?;
    if dry_run {
        let mut stdout = std::io::stdout().lock();
        return stdout
            .write_all(plan.content.as_bytes())
            .and_then(|()| stdout.flush())
            .map_err(|error| format!("cannot write the preview to stdout: {error}"));
    }
    // A re-run that would change nothing must not rewrite the file, so its
    // permissions and timestamp survive.
    if plan.existing.as_deref() == Some(plan.content.as_bytes()) {
        return Ok(());
    }
    std::fs::write(&plan.path, plan.content.as_bytes())
        .map_err(|error| format!("cannot write {}: {error}", plan.path.display()))
}

/// Read the existing `AGENTS.md` and compose its replacement. `NotFound` is the
/// only read error that means "there is nothing to preserve"; every other read
/// error — an unreadable file, a directory in the file's place — is a failure
/// reported before anything is written.
fn plan(root: &Path) -> Result<Plan, String> {
    let path = root.join(FILE_NAME);
    let existing = match std::fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("cannot read {}: {error}", path.display())),
    };
    let text = match &existing {
        Some(bytes) => Some(
            std::str::from_utf8(bytes)
                .map_err(|_| format!("{} is not valid UTF-8", path.display()))?
                .to_owned(),
        ),
        None => None,
    };
    Ok(Plan {
        path,
        existing,
        content: compose(text.as_deref()),
    })
}

/// The complete new contents of `AGENTS.md` given the current ones. The mapping
/// block is replaced in place when it is already present anywhere in the file,
/// and appended otherwise; everything around it keeps its bytes.
fn compose(existing: Option<&str>) -> String {
    let newline = match existing {
        Some(text) if text.contains("\r\n") => "\r\n",
        _ => "\n",
    };
    let block = restyle(MAPPING, newline);

    let Some(existing) = existing else {
        return format!("{block}{newline}");
    };

    if let Some(begin) = existing.find(BEGIN) {
        let end = existing[begin..]
            .find(END)
            .map(|offset| begin + offset + END.len());
        let mut content = String::with_capacity(existing.len() + block.len());
        content.push_str(&existing[..begin]);
        content.push_str(&block);
        // A block whose end marker is gone is repaired to the end of the file,
        // so the file never keeps a half block no later run can find.
        content.push_str(end.map_or(newline, |end| &existing[end..]));
        return content;
    }

    let mut content = String::with_capacity(existing.len() + block.len() + newline.len());
    content.push_str(existing);
    // Start the block on its own line without inserting a blank one.
    if !existing.is_empty() && !existing.ends_with('\n') {
        content.push_str(newline);
    }
    content.push_str(&block);
    content.push_str(newline);
    content
}

/// The mapping with the file's line-ending style, so a CRLF project does not get
/// one LF block spliced into it.
fn restyle(block: &str, newline: &str) -> String {
    if newline == "\n" {
        block.to_owned()
    } else {
        block.replace('\n', newline)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_the_mapping_advertises_is_actually_proxied() {
        for line in MAPPING.lines() {
            let Some(rest) = line.strip_prefix("- `toltrim ") else {
                continue;
            };
            let command = rest.strip_suffix('`').expect("a bullet ends its code span");
            let words: Vec<String> = command.split_whitespace().map(str::to_owned).collect();
            let (program, args) = words.split_first().expect("a bullet names a command");
            let tool = crate::trim::detect(program);
            assert_ne!(
                tool,
                crate::trim::Tool::Unknown,
                "`{command}` is advertised but not detected"
            );
            assert!(
                crate::trim::is_proxied(tool, args),
                "`{command}` is advertised but not proxied"
            );
        }
    }

    #[test]
    fn the_mapping_does_not_advertise_the_deferred_tool() {
        assert!(!MAPPING.to_ascii_lowercase().contains("gradle"));
    }

    #[test]
    fn a_truncated_block_is_repaired_rather_than_duplicated() {
        let existing = format!("kept\n{BEGIN}\nstale without an end marker");
        let content = compose(Some(&existing));
        assert!(content.starts_with("kept\n"));
        assert_eq!(content.matches(BEGIN).count(), 1);
        assert_eq!(content.matches(END).count(), 1);
        assert!(!content.contains("stale"));
        // And the repair is itself byte-stable.
        assert_eq!(compose(Some(&content)), content);
    }

    #[test]
    fn composing_its_own_output_is_byte_identical() {
        for existing in [
            None,
            Some(""),
            Some("# Notes"),
            Some("# Notes\n"),
            Some("# Notes\r\n\r\nkept\r\n"),
        ] {
            let once = compose(existing);
            assert_eq!(compose(Some(&once)), once, "unstable for {existing:?}");
        }
    }

    #[test]
    fn an_existing_line_ending_style_is_kept() {
        let content = compose(Some("# Notes\r\n"));
        assert!(content.contains(&format!("{BEGIN}\r\n")));
        assert!(!content.contains(&format!("{BEGIN}\n\n")));
        assert!(content.starts_with("# Notes\r\n"));
    }

    #[test]
    fn a_file_without_a_trailing_newline_is_not_joined_to_the_block() {
        let content = compose(Some("# Notes"));
        assert!(content.starts_with("# Notes\n"));
        assert!(!content.contains("# Notes\n\n"));
    }
}

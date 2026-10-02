//! Which tool a command is, whether this Invocation may be trimmed at all, and
//! the one pure function every Trim rule is reached through.
//!
//! `render` is the seam: it takes the tool, the caller's arguments, the raw
//! output bytes, the exit code, and the preservation decision, and returns the
//! bytes an agent will read. It knows nothing about processes, so every rule and
//! every preservation path is testable without starting one.

pub mod git;
pub mod listing;
pub mod maven;

use crate::ansi;

/// The rule version recorded with each Gain Record. Bump it when a rule changes
/// shape, so savings measured under different rules are never silently compared.
pub const RULES_VERSION: i64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Maven,
    Git,
    Listing,
    Unknown,
}

impl Tool {
    /// The name a Gain Record stores for this tool. An Unknown tool is a
    /// Passthrough Command, which is what the record's `tool` column says.
    pub fn as_str(self) -> &'static str {
        match self {
            Tool::Maven => "maven",
            Tool::Git => "git",
            Tool::Listing => "listing",
            Tool::Unknown => "passthrough",
        }
    }
}

/// How one Invocation was handled; this is the `mode` column of a Gain Record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// A Proxied Command whose output a rule set rewrote.
    Trimmed,
    /// A Proxied Command the caller asked to keep intact; only ANSI was removed.
    Preserved,
    /// A Passthrough Command, emitted byte for byte and excluded from the
    /// Savings Rate.
    Passthrough,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Trimmed => "trimmed",
            Mode::Preserved => "preserved",
            Mode::Passthrough => "passthrough",
        }
    }
}

/// Identify the tool from the program name the caller typed.
///
/// Accepts a path and a Windows executable suffix, because `PATH` resolution on
/// Windows hands us `mvn.cmd` and `gradle.bat` rather than a bare name.
pub fn detect(program: &str) -> Tool {
    let name = program
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(program)
        .to_ascii_lowercase();
    let name = name
        .strip_suffix(".exe")
        .or_else(|| name.strip_suffix(".cmd"))
        .or_else(|| name.strip_suffix(".bat"))
        .unwrap_or(&name);
    match name {
        "mvn" | "mvnw" => Tool::Maven,
        "git" => Tool::Git,
        "ls" | "dir" | "tree" => Tool::Listing,
        _ => Tool::Unknown,
    }
}

/// Does a Trim rule exist for this exact Invocation?
///
/// A command can name a Proxied tool and still be a Passthrough Command: ADR-0008
/// keeps state-changing git subcommands (`add`, `commit`, `checkout`, …) out of
/// the rules, and those are never interpreted.
pub fn is_proxied(tool: Tool, args: &[String]) -> bool {
    match tool {
        Tool::Maven | Tool::Listing => true,
        Tool::Git => matches!(
            subcommand(tool, args).as_deref(),
            Some("status" | "diff" | "log" | "show" | "branch")
        ),
        Tool::Unknown => false,
    }
}

/// Which of the three modes this Invocation is in.
pub fn classify(tool: Tool, args: &[String], preserve: bool) -> Mode {
    if !is_proxied(tool, args) {
        Mode::Passthrough
    } else if preserve {
        Mode::Preserved
    } else {
        Mode::Trimmed
    }
}

/// The subcommand a Gain Record stores: `test` for `mvn -q test`, `diff` for
/// `git -C repo diff`. `None` when the invocation names no subcommand.
pub fn subcommand(tool: Tool, args: &[String]) -> Option<String> {
    match tool {
        Tool::Maven => first_non_flag(args),
        Tool::Git => git::subcommand(args),
        Tool::Listing | Tool::Unknown => None,
    }
}

/// The first argument that is not an option, so `mvn -q -DskipTests test` yields
/// `test`. Maven's own flags are never inspected beyond this.
pub fn first_non_flag(args: &[String]) -> Option<String> {
    args.iter().find(|a| !a.starts_with('-')).cloned()
}

/// Flags whose whole purpose is to make output parseable by another program.
/// When the caller asked for one, trimming would take away what they asked for.
const MACHINE_READABLE_LONG: &[&str] = &[
    "--porcelain",
    "-z",
    "--color=always",
    "--batch-mode",
    "--quiet",
    "--debug",
    "--errors",
    "--json",
    "--stat",
    "--name-only",
    "--name-status",
    "--oneline",
    "--raw",
];

/// Short flags are tool-specific: `-e` means `--errors` to Maven but `--edit` to
/// git, so it must never be treated as a global marker.
fn machine_readable_short(tool: Tool) -> &'static [&'static str] {
    match tool {
        Tool::Maven => &["-q", "-B", "-X", "-e"],
        Tool::Git => &["-s"],
        Tool::Listing | Tool::Unknown => &[],
    }
}

/// Did the caller ask for output that another program will parse?
pub fn requests_machine_readable(tool: Tool, args: &[String]) -> bool {
    let shorts = machine_readable_short(tool);
    args.iter().any(|arg| {
        MACHINE_READABLE_LONG.contains(&arg.as_str()) || shorts.contains(&arg.as_str())
    })
}

/// A person piping toltrim in a terminal looks like this: their shell gives us a
/// non-terminal stdout, but stdin is still their terminal. An agent capturing
/// output has neither. Without this, the two are indistinguishable, and treating
/// "not a terminal" as "do not trim" would disable trimming for the main case.
pub fn is_human_pipe(stdout_is_terminal: bool, stdin_is_terminal: bool) -> bool {
    !stdout_is_terminal && stdin_is_terminal
}

/// The whole preservation decision. It is a pure function so that `--raw`, a
/// Machine-Readable Request, and the human-pipe case are all testable without a
/// process.
pub fn should_preserve(
    raw: bool,
    tool: Tool,
    args: &[String],
    stdout_is_terminal: bool,
    stdin_is_terminal: bool,
) -> bool {
    raw || requests_machine_readable(tool, args)
        || is_human_pipe(stdout_is_terminal, stdin_is_terminal)
}

/// The pure render seam: the only way any Trim rule is reached.
///
/// * Not valid UTF-8, or an Unknown tool: emit the bytes unchanged (ADR-0002), so
///   a command we cannot trim behaves exactly as it would alone.
/// * `preserve`: strip ANSI and remove nothing else.
/// * Otherwise apply the tool's rule set.
pub fn render(
    tool: Tool,
    args: &[String],
    stdout: &[u8],
    exit_code: i32,
    preserve: bool,
) -> Vec<u8> {
    let Ok(text) = std::str::from_utf8(stdout) else {
        return stdout.to_vec();
    };
    if tool == Tool::Unknown {
        return stdout.to_vec();
    }
    if preserve {
        return ansi::strip(text).into_bytes();
    }
    match tool {
        Tool::Maven => maven::render(text, exit_code).into_bytes(),
        Tool::Git => git::render(text, exit_code, args).into_bytes(),
        Tool::Listing => listing::render(text).into_bytes(),
        Tool::Unknown => unreachable!("handled above"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn detects_tools_through_paths_and_suffixes() {
        assert_eq!(detect("mvn"), Tool::Maven);
        assert_eq!(detect("/opt/homebrew/bin/mvn"), Tool::Maven);
        assert_eq!(detect("mvn.cmd"), Tool::Maven);
        assert_eq!(detect("gradle.bat"), Tool::Unknown);
        assert_eq!(detect("git"), Tool::Git);
        assert_eq!(detect("dir"), Tool::Listing);
        assert_eq!(detect("gcloud"), Tool::Unknown);
    }

    #[test]
    fn machine_readable_flags_are_recognised() {
        assert!(requests_machine_readable(
            Tool::Git,
            &args(&["status", "--porcelain"])
        ));
        assert!(requests_machine_readable(Tool::Maven, &args(&["-q", "test"])));
        assert!(!requests_machine_readable(Tool::Maven, &args(&["test"])));
    }

    #[test]
    fn short_flag_meanings_do_not_leak_across_tools() {
        // `-e` is --errors for Maven but --edit for git.
        assert!(requests_machine_readable(Tool::Maven, &args(&["-e", "test"])));
        assert!(!requests_machine_readable(Tool::Git, &args(&["commit", "-e"])));
    }

    #[test]
    fn only_a_terminal_stdin_with_a_piped_stdout_counts_as_human_pipe() {
        assert!(is_human_pipe(false, true));
        assert!(!is_human_pipe(false, false)); // an agent capturing output
        assert!(!is_human_pipe(true, false));
        assert!(!is_human_pipe(true, true));
    }

    #[test]
    fn an_unknown_command_is_emitted_byte_for_byte() {
        // Deliberately not valid UTF-8: even a byte we cannot decode must reach
        // the caller unchanged rather than being replaced or dropped.
        let bytes = b"\xff\xfe not utf8 \x00";
        assert_eq!(render(Tool::Unknown, &[], bytes, 0, false), bytes.to_vec());
        assert_eq!(render(Tool::Unknown, &[], bytes, 0, true), bytes.to_vec());
    }

    #[test]
    fn a_preserved_invocation_only_loses_ansi() {
        let bytes = b"\x1b[31mkept text\x1b[0m\n";
        assert_eq!(render(Tool::Git, &args(&["diff"]), bytes, 0, true), b"kept text\n");
    }

    #[test]
    fn the_preservation_decision_is_process_free() {
        // `--raw` wins over everything.
        assert!(should_preserve(true, Tool::Maven, &args(&["test"]), false, false));
        // A Machine-Readable Request.
        assert!(should_preserve(false, Tool::Git, &args(&["status", "--porcelain"]), false, false));
        // A human piping in a terminal: stdout is not a tty, stdin is.
        assert!(should_preserve(false, Tool::Maven, &args(&["test"]), false, true));
        // An agent capturing output: neither stream is a terminal.
        assert!(!should_preserve(false, Tool::Maven, &args(&["test"]), false, false));
        assert!(!should_preserve(false, Tool::Git, &args(&["diff"]), true, true));
    }

    #[test]
    fn state_changing_git_subcommands_are_passthrough_commands() {
        assert_eq!(classify(Tool::Git, &args(&["commit", "-m", "x"]), false), Mode::Passthrough);
        assert_eq!(classify(Tool::Git, &args(&["diff"]), false), Mode::Trimmed);
        assert_eq!(
            classify(Tool::Git, &args(&["diff", "--stat"]), requests_machine_readable(Tool::Git, &args(&["diff", "--stat"]))),
            Mode::Preserved
        );
        assert_eq!(classify(Tool::Unknown, &args(&["gcloud", "x"]), false), Mode::Passthrough);
        assert_eq!(classify(Tool::Maven, &args(&["test"]), true), Mode::Preserved);
        assert_eq!(classify(Tool::Maven, &args(&["test"]), false), Mode::Trimmed);
    }

    #[test]
    fn subcommands_are_found_past_options() {
        assert_eq!(subcommand(Tool::Maven, &args(&["-q", "-DskipTests", "test"])), Some("test".into()));
        assert_eq!(subcommand(Tool::Git, &args(&["-C", "repo", "diff"])), Some("diff".into()));
        assert_eq!(subcommand(Tool::Listing, &args(&["-la"])), None);
    }
}

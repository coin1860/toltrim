//! Which tool a command is, and whether this Invocation may be trimmed at all.

pub mod maven;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Maven,
    Git,
    Listing,
    Unknown,
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
        assert!(requests_machine_readable(Tool::Git, &args(&["status", "--porcelain"])));
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
}

//! Process execution. This module never inspects output; it only collects it.

use std::ffi::OsString;
use std::io::{IsTerminal, Read};
use std::process::{Command, Stdio};

/// Everything one child process produced, plus how it ended.
pub struct Outcome {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    /// The child's exit code, verbatim. A process killed by signal `n` reports
    /// `128 + n`, matching shell convention.
    pub code: i32,
    /// `Some` only when the process could not be started at all.
    pub spawn_error: Option<String>,
}

/// Run `program` with `args`, capturing both streams.
///
/// stdin is inherited only when toltrim's own stdin is a terminal, so an agent
/// invocation fails fast instead of blocking on a prompt that nobody will
/// answer. Commands are spawned with an argument vector and never through a
/// shell; on Windows a failed lookup is retried through `cmd /c`, which is what
/// makes shell builtins such as `dir` work.
///
/// That retry always starts — `cmd.exe` exists even when the program does not —
/// so a successful spawn there is not proof that the program was found. The
/// retry's own report has to decide, or "not installed" would reach the agent
/// as cmd's exit code and cmd's message instead of 127.
pub fn run(program: &OsString, args: &[OsString]) -> Outcome {
    let mut last: Option<std::io::Error> = None;
    for via_cmd in attempts() {
        match attempt(program, args, *via_cmd) {
            Ok(outcome) => return interpret(*via_cmd, outcome, program),
            Err(e) => {
                let not_found = e.kind() == std::io::ErrorKind::NotFound;
                last = Some(e);
                if !not_found {
                    break;
                }
            }
        }
    }
    let e = last.expect("at least one attempt is always made");
    missing_program(program, &e.to_string())
}

/// What one attempt's result means.
///
/// A direct spawn that succeeded speaks for itself. A `cmd /c` retry does not:
/// `cmd.exe` starts even when the program does not, so its own report decides
/// whether the program was found. This is split out of [`run`] so that decision
/// is testable without a Windows host; on Unix `via_cmd` is always false and the
/// outcome is returned untouched.
fn interpret(via_cmd: bool, outcome: Outcome, program: &OsString) -> Outcome {
    if via_cmd && cmd_reports_missing(outcome.code, &outcome.stderr) {
        return missing_program(program, "not found");
    }
    outcome
}

/// The shape a program that could not be started produces: exit 127, one
/// explanatory line, and nothing on either stream.
fn missing_program(program: &OsString, reason: &str) -> Outcome {
    Outcome {
        stdout: Vec::new(),
        stderr: Vec::new(),
        code: 127,
        spawn_error: Some(format!("{}: {reason}", program.to_string_lossy())),
    }
}

/// Did a `cmd /c` attempt fail because the program does not exist?
///
/// Only the retry path can produce this, but the decision is pure and is tested
/// on every platform. `cmd.exe` answers a missing program either with the
/// "command not recognized" status `9009`, or with exit code `1` plus its
/// classic message on stderr. Exit code `1` on its own is never evidence: a
/// real command is allowed to fail, and reporting that as "not installed" would
/// be a lie the agent cannot detect.
fn cmd_reports_missing(exit_code: i32, stderr: &[u8]) -> bool {
    /// How `cmd.exe` spells it, recorded from a Windows run.
    const NOT_RECOGNIZED: &[u8] = b"is not recognized as an internal or external command";
    if exit_code == 0 {
        return false;
    }
    exit_code == 9009
        || (exit_code == 1
            && stderr
                .windows(NOT_RECOGNIZED.len())
                .any(|window| window == NOT_RECOGNIZED))
}

fn attempts() -> &'static [bool] {
    if cfg!(windows) {
        &[false, true]
    } else {
        &[false]
    }
}

fn attempt(program: &OsString, args: &[OsString], via_cmd: bool) -> std::io::Result<Outcome> {
    let mut cmd = if via_cmd {
        let mut c = Command::new("cmd");
        c.arg("/c").arg(program);
        c.args(args);
        c
    } else {
        let mut c = Command::new(program);
        c.args(args);
        c
    };

    cmd.stdin(if std::io::stdin().is_terminal() {
        Stdio::inherit()
    } else {
        Stdio::null()
    });
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn()?;

    // Read both pipes concurrently: a child that fills one pipe while we wait on
    // the other would otherwise deadlock.
    let out_pipe = child.stdout.take();
    let err_pipe = child.stderr.take();
    let out_thread = std::thread::spawn(move || drain(out_pipe));
    let err_thread = std::thread::spawn(move || drain(err_pipe));

    let status = child.wait()?;
    let stdout = out_thread.join().unwrap_or_default();
    let stderr = err_thread.join().unwrap_or_default();

    Ok(Outcome {
        stdout,
        stderr,
        code: exit_code(&status),
        spawn_error: None,
    })
}

fn drain<R: Read>(reader: Option<R>) -> Vec<u8> {
    let mut buf = Vec::new();
    if let Some(mut r) = reader {
        let _ = r.read_to_end(&mut buf);
    }
    buf
}

fn exit_code(status: &std::process::ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128 + signal;
        }
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact text `cmd.exe` prints, recorded from a Windows run.
    const NOT_RECOGNIZED: &[u8] =
        b"'foo' is not recognized as an internal or external command,\r\n\
                                    operable program or batch file.\r\n";

    #[test]
    fn a_missing_program_is_read_out_of_the_cmd_retry() {
        // The message alone, and the status alone, are each enough.
        assert!(cmd_reports_missing(1, NOT_RECOGNIZED));
        assert!(cmd_reports_missing(9009, b""));
    }

    #[test]
    fn a_real_command_that_failed_is_not_a_missing_program() {
        // Exit code 1 without cmd's message is an ordinary failure.
        assert!(!cmd_reports_missing(1, b""));
        assert!(!cmd_reports_missing(1, b"some real command failed\r\n"));
    }

    #[test]
    fn a_successful_command_is_never_a_missing_program() {
        assert!(!cmd_reports_missing(0, NOT_RECOGNIZED));
    }

    #[test]
    fn a_program_that_could_not_be_started_reports_127_and_nothing_else() {
        let outcome = missing_program(&OsString::from("nope"), "not found");
        assert_eq!(outcome.code, 127);
        assert!(outcome.stdout.is_empty());
        assert!(outcome.stderr.is_empty());
        assert_eq!(outcome.spawn_error.as_deref(), Some("nope: not found"));
    }

    /// What `cmd /c` returns when the program is missing: a start that
    /// succeeded, cmd's status, and cmd's message.
    fn cmd_said_missing() -> Outcome {
        Outcome {
            stdout: b"whatever cmd printed".to_vec(),
            stderr: NOT_RECOGNIZED.to_vec(),
            code: 1,
            spawn_error: None,
        }
    }

    #[test]
    fn the_cmd_retry_that_says_missing_becomes_a_127() {
        let interpreted = interpret(true, cmd_said_missing(), &OsString::from("nope"));
        assert_eq!(interpreted.code, 127);
        assert!(
            interpreted.stdout.is_empty(),
            "cmd's own output must not reach the caller"
        );
        assert!(
            interpreted.stderr.is_empty(),
            "cmd's own message must not reach the caller"
        );
        assert_eq!(interpreted.spawn_error.as_deref(), Some("nope: not found"));
    }

    #[test]
    fn a_direct_spawn_is_never_reinterpreted() {
        // On Unix `via_cmd` is always false. A real command that prints that
        // text and exits 1 is a real failure, not a missing program, and must
        // arrive exactly as it was.
        let outcome = interpret(false, cmd_said_missing(), &OsString::from("real-tool"));
        assert_eq!(outcome.code, 1);
        assert_eq!(outcome.stdout, b"whatever cmd printed".to_vec());
        assert_eq!(outcome.stderr, NOT_RECOGNIZED.to_vec());
        assert!(outcome.spawn_error.is_none());
    }
}

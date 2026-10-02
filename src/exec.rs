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
pub fn run(program: &OsString, args: &[OsString]) -> Outcome {
    let mut last: Option<std::io::Error> = None;
    for via_cmd in attempts() {
        match attempt(program, args, *via_cmd) {
            Ok(outcome) => return outcome,
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
    Outcome {
        stdout: Vec::new(),
        stderr: Vec::new(),
        code: 127,
        spawn_error: Some(format!("{}: {e}", program.to_string_lossy())),
    }
}

fn attempts() -> &'static [bool] {
    if cfg!(windows) { &[false, true] } else { &[false] }
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

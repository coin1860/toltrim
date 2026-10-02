//! Seam 2: the process contract, observed against the built binary.
//!
//! A pure function cannot see an exit code, a byte-exact passthrough, a stdin
//! that is not a terminal, or a command that never started. Those promises are
//! only observable by running `toltrim` with a stand-in command, so every test
//! here spawns the binary cargo built (`CARGO_BIN_EXE_toltrim`) with a stand-in
//! chosen by the platform: on Unix a `#!/bin/sh` script written into a uniquely
//! named directory at test time, on Windows a name that `PATH` cannot resolve.
//! Nothing in the worktree is touched, no developer's PATH is searched beyond
//! `/bin/sh`, and every Invocation points `TOLTRIM_DB` at the test's own
//! directory so a Gain Record can never land in the developer's real database.
//!
//! Most tests are Unix-only because their stand-ins are shell scripts:
//! `#[cfg(unix)]` guards each of those and the helpers only they use. The one
//! promise that needs a Windows-specific stand-in — a command that does not
//! exist — is guarded with `#[cfg(windows)]` and spawns nothing but the binary
//! itself.

use std::ffi::OsStr;
use std::fs;
use std::io::Read;
#[cfg(unix)]
use std::path::Path;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The binary under test, as cargo built it for this test run.
const BIN: &str = env!("CARGO_BIN_EXE_toltrim");

/// A run that outlives this is a hang, which is itself the failure.
const DEADLINE: Duration = Duration::from_secs(5);

/// How often the deadline loop looks at the child.
const POLL: Duration = Duration::from_millis(10);

/// A directory that outlives one test and is removed when the test ends.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// A fresh directory under cargo's per-crate test temp root, falling back to
    /// the system temp directory. It is uniquely named on every run, so parallel
    /// test processes cannot collide, and it lives under `target/`, which is
    /// ignored by git.
    fn new(label: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let base = std::env::var_os("CARGO_TARGET_TMPDIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|since| since.as_nanos())
            .unwrap_or_default();
        let path = base.join(format!(
            "toltrim-process-contract-{label}-{}-{nanos}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path)
            .unwrap_or_else(|e| panic!("cannot create {}: {e}", path.display()));
        TempDir { path }
    }

    #[cfg(unix)]
    fn path(&self) -> &Path {
        &self.path
    }

    /// The Gain Record database for this test alone.
    fn db(&self) -> PathBuf {
        self.path.join("history.db")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Write one stand-in program and return its path.
///
/// The file carries no suffix, so `trim::detect` reads the name itself: a
/// stand-in named `mvn` is a Proxied Command, and anything else is unknown.
#[cfg(unix)]
fn stand_in(dir: &TempDir, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let path = dir.path().join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}"))
        .unwrap_or_else(|e| panic!("cannot write {}: {e}", path.display()));
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|e| panic!("cannot make {} executable: {e}", path.display()));
    path
}

/// One finished run: what it wrote and how it ended.
struct Finished {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl Finished {
    /// The exit code, verbatim. Nothing in this file is ever killed by a signal
    /// itself: the one stand-in that is reports `143` through toltrim.
    fn code(&self) -> i32 {
        self.status
            .code()
            .unwrap_or_else(|| panic!("the process was killed by a signal: {:?}", self.status))
    }

    fn stdout_lossy(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    fn stderr_lossy(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
}

fn spawn(mut cmd: Command, label: &str) -> Child {
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.spawn()
        .unwrap_or_else(|e| panic!("cannot start {label}: {e}"))
}

/// Wait for a child with a deadline instead of blocking forever. A hang is one
/// of the failures this file exists to catch, so it must not be able to hang the
/// suite: the child is killed and the test fails on its own terms.
fn finish(mut child: Child, label: &str) -> Finished {
    let deadline = Instant::now() + DEADLINE;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut stdout = Vec::new();
                let mut stderr = Vec::new();
                child
                    .stdout
                    .take()
                    .expect("stdout is piped")
                    .read_to_end(&mut stdout)
                    .expect("stdout can be read");
                child
                    .stderr
                    .take()
                    .expect("stderr is piped")
                    .read_to_end(&mut stderr)
                    .expect("stderr can be read");
                return Finished {
                    status,
                    stdout,
                    stderr,
                };
            }
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                panic!(
                    "{label} was still running after {DEADLINE:?}: it blocked instead of finishing"
                );
            }
            Ok(None) => std::thread::sleep(POLL),
            Err(e) => panic!("cannot wait for {label}: {e}"),
        }
    }
}

/// `toltrim <args…>`, with stdin closed and the test's own database.
fn toltrim(dir: &TempDir, args: &[&OsStr]) -> Command {
    let mut cmd = Command::new(BIN);
    cmd.args(args);
    cmd.env("TOLTRIM_DB", dir.db());
    cmd.stdin(Stdio::null());
    cmd
}

fn run_toltrim(dir: &TempDir, args: &[&OsStr], label: &str) -> Finished {
    finish(spawn(toltrim(dir, args), label), label)
}

/// The same stand-in run with no toltrim in front of it, which is what "byte
/// identical" and "verbatim" must be measured against.
#[cfg(unix)]
fn run_stand_in(program: &Path, label: &str) -> Finished {
    let mut cmd = Command::new(program);
    cmd.stdin(Stdio::null());
    finish(spawn(cmd, label), label)
}

#[cfg(unix)]
#[test]
fn exit_codes_are_forwarded_verbatim() {
    let dir = TempDir::new("exit-codes");
    for code in [0, 1, 2, 42, 255] {
        let name = format!("standin-exit-{code}");
        let program = stand_in(
            &dir,
            &name,
            &format!("printf 'stand-in ran\\n'\nexit {code}\n"),
        );
        let run = run_toltrim(
            &dir,
            &[program.as_os_str()],
            &format!("the {code} stand-in"),
        );
        assert_eq!(
            run.code(),
            code,
            "exit code {code} was rewritten; stdout={:?} stderr={:?}",
            run.stdout_lossy(),
            run.stderr_lossy()
        );
        assert_eq!(
            run.stdout, b"stand-in ran\n",
            "an unknown command's output must arrive unchanged"
        );
        assert!(
            run.stderr.is_empty(),
            "stderr appeared from nowhere: {:?}",
            run.stderr_lossy()
        );
    }
}

#[cfg(unix)]
#[test]
fn a_command_that_ran_fine_but_exits_non_zero_keeps_its_own_code() {
    // The `git diff` shape: exit 1 means "there are differences", not "the
    // command failed" (ADR-0003).
    let dir = TempDir::new("git-diff-shape");
    let git = stand_in(
        &dir,
        "git",
        "printf 'diff --git a/x b/x\\n--- a/x\\n+++ b/x\\n@@ -1 +1 @@\\n-old\\n+new\\n'\nexit 1\n",
    );
    let run = run_toltrim(
        &dir,
        &[git.as_os_str(), OsStr::new("diff")],
        "a git diff stand-in",
    );
    assert_eq!(
        run.code(),
        1,
        "a Proxied Command's non-zero code must not be explained away; stderr={:?}",
        run.stderr_lossy()
    );
    assert!(
        run.stdout_lossy().contains("+new"),
        "the diff body must survive: {:?}",
        run.stdout_lossy()
    );
}

#[cfg(unix)]
#[test]
fn an_unknown_command_is_byte_identical_to_running_it_alone() {
    let dir = TempDir::new("byte-identity");
    // Not valid UTF-8, and no trailing newline: a comparison made after decoding
    // would pass while the bytes themselves changed.
    let program = stand_in(
        &dir,
        "standin-bytes",
        "printf 'plain \\377\\376 bytes'\nprintf ' and no trailing newline'\nexit 3\n",
    );

    let direct = run_stand_in(&program, "the stand-in alone");
    let proxied = run_toltrim(&dir, &[program.as_os_str()], "the stand-in through toltrim");

    assert_eq!(
        direct.stdout,
        b"plain \xff\xfe bytes and no trailing newline".to_vec(),
        "the comparison itself is wrong"
    );
    assert_eq!(
        proxied.stdout, direct.stdout,
        "an unknown command was not byte-identical"
    );
    assert_eq!(proxied.code(), direct.code());
    assert_eq!(proxied.code(), 3);
}

#[cfg(unix)]
#[test]
fn an_unknown_command_keeps_its_stderr_byte_for_byte() {
    let dir = TempDir::new("stderr-identity");
    let program = stand_in(
        &dir,
        "standin-stderr",
        "printf 'warning: plain\\n\\377\\376\\n' >&2\nprintf 'body\\n'\n",
    );

    let direct = run_stand_in(&program, "the stand-in alone");
    let proxied = run_toltrim(&dir, &[program.as_os_str()], "the stand-in through toltrim");

    assert_eq!(
        proxied.stderr, direct.stderr,
        "stderr was not forwarded byte for byte"
    );
    assert_eq!(proxied.stderr, b"warning: plain\n\xff\xfe\n".to_vec());
    assert_eq!(proxied.stdout, direct.stdout);
}

#[cfg(unix)]
#[test]
fn with_no_terminal_on_stdin_an_interactive_command_fails_fast() {
    let dir = TempDir::new("stdin-null");
    // `read` would wait forever for a line no agent will ever type; with no
    // terminal on toltrim's stdin it must see end-of-file instead.
    let program = stand_in(
        &dir,
        "standin-interactive",
        "read line\nprintf 'read returned\\n'\n",
    );

    let started = Instant::now();
    let run = run_toltrim(
        &dir,
        &[program.as_os_str()],
        "an interactive stand-in with stdin closed",
    );
    let elapsed = started.elapsed();

    assert_eq!(run.code(), 0, "stderr={:?}", run.stderr_lossy());
    assert_eq!(
        run.stdout, b"read returned\n",
        "the stand-in never got past its read"
    );
    assert!(
        elapsed < DEADLINE,
        "the run took {elapsed:?}, which is not failing fast"
    );
}

#[cfg(unix)]
#[test]
fn a_stdin_pipe_that_is_never_written_does_not_block_the_run() {
    let dir = TempDir::new("stdin-pipe");
    let program = stand_in(
        &dir,
        "standin-pipe",
        "read line\nprintf 'read returned\\n'\n",
    );

    let mut cmd = toltrim(&dir, &[program.as_os_str()]);
    // Exactly what an agent hands toltrim: a non-terminal stdin that nobody
    // writes to and whose write end is held open, so there is no end-of-file to
    // be had unless toltrim declines to hand it to the child.
    cmd.stdin(Stdio::piped());
    let label = "an interactive stand-in with an unwritten stdin pipe";
    let mut child = spawn(cmd, label);
    // Keep the write end open for the whole run; `let _ =` here would close it.
    let _write_end_is_held_open = child.stdin.take().expect("stdin is piped");

    let started = Instant::now();
    let run = finish(child, label);
    let elapsed = started.elapsed();

    assert_eq!(run.code(), 0, "stderr={:?}", run.stderr_lossy());
    assert_eq!(run.stdout, b"read returned\n");
    assert!(
        elapsed < DEADLINE,
        "the run took {elapsed:?}, which is not failing fast"
    );
}

#[cfg(unix)]
#[test]
fn a_command_that_does_not_exist_exits_127_with_one_line_on_stderr() {
    let dir = TempDir::new("missing-command");
    // An absolute path that does not exist: hermetic, and no PATH lookup can
    // accidentally find something.
    let missing = dir.path().join("toltrim-standin-that-does-not-exist");
    assert!(!missing.exists());

    let run = run_toltrim(
        &dir,
        &[missing.as_os_str()],
        "a command that does not exist",
    );

    assert_eq!(run.code(), 127);
    assert!(
        run.stdout.is_empty(),
        "toltrim wrote to stdout for a command that never started: {:?}",
        run.stdout_lossy()
    );
    let stderr = run.stderr_lossy();
    assert!(
        stderr.starts_with("toltrim: "),
        "expected toltrim's own one-line report, got {stderr:?}"
    );
    assert_eq!(
        stderr.lines().count(),
        1,
        "expected a single explanatory line, got {stderr:?}"
    );
    assert!(
        stderr.ends_with('\n'),
        "the line must be terminated: {stderr:?}"
    );
    assert!(
        stderr.contains("toltrim-standin-that-does-not-exist"),
        "the line must name the command that could not be started: {stderr:?}"
    );
    assert!(
        String::from_utf8(run.stderr).is_ok(),
        "the report must be readable text"
    );
}

#[cfg(unix)]
#[test]
fn a_command_killed_by_signal_n_reports_128_plus_n() {
    let dir = TempDir::new("signal");
    // SIGTERM is 15, so shell convention says 143. `exit 99` is only a fallback:
    // if the signal never arrives the test must fail loudly rather than quietly
    // pass on some other code.
    let program = stand_in(&dir, "standin-signal", "kill -TERM $$\nexit 99\n");

    let run = run_toltrim(
        &dir,
        &[program.as_os_str()],
        "a stand-in that kills itself with SIGTERM",
    );

    assert_eq!(
        run.code(),
        143,
        "128 + 15 was not reported; stderr={:?}",
        run.stderr_lossy()
    );
}

#[cfg(unix)]
#[test]
fn a_proxied_command_forwards_stderr_verbatim_while_stdout_is_trimmed() {
    let dir = TempDir::new("maven-stderr");
    // Named `mvn`, so `trim::detect` recognises it through its absolute path and
    // the run really is a Proxied Command. The stdout is Maven-shaped; the
    // stderr is distinctive bytes that trimming must never touch.
    let mvn = stand_in(
        &dir,
        "mvn",
        "printf '%s\\n' \
         '[INFO] Scanning for projects...' \
         '[INFO] -----< com.example:demo >-----' \
         '[INFO] Tests run: 3, Failures: 0, Errors: 0, Skipped: 0' \
         '[INFO] Total time:  2.345 s' \
         '[INFO] BUILD SUCCESS' \
         '[INFO] T E S T S'\n\
         printf 'distinct stderr\\n\\377\\376 raw\\n' >&2\n\
         exit 0\n",
    );

    let direct = run_stand_in(&mvn, "the maven stand-in alone");
    let proxied = run_toltrim(
        &dir,
        &[mvn.as_os_str()],
        "the maven stand-in through toltrim",
    );

    assert_eq!(
        proxied.stderr, direct.stderr,
        "a Proxied Command's stderr must be forwarded verbatim"
    );
    assert_eq!(
        proxied.stderr,
        b"distinct stderr\n\xff\xfe raw\n".to_vec(),
        "trimming must never remove stderr"
    );
    assert_eq!(proxied.code(), 0);

    let summary = proxied.stdout_lossy();
    assert!(summary.contains("BUILD SUCCESS"), "got {summary:?}");
    assert!(
        summary.contains("Tests run: 3, Failures: 0, Errors: 0, Skipped: 0"),
        "the counts that prove the work ran must survive: {summary:?}"
    );
    assert!(
        !summary.contains("Scanning for projects"),
        "Noise survived into the Success Summary: {summary:?}"
    );
    assert!(
        proxied.stdout.len() < direct.stdout.len(),
        "the maven stand-in's stdout was not trimmed: {:?}",
        direct.stdout
    );
}

#[cfg(windows)]
#[test]
fn a_command_that_does_not_exist_exits_127_on_windows_too() {
    let dir = TempDir::new("missing-command-windows");
    // A bare name, so the direct spawn goes through the Windows lookup, fails,
    // and reaches the `cmd /c` retry — which starts `cmd.exe` successfully and
    // reports the missing program itself. That retry's own message is what has
    // to become the 127, not cmd's exit code (ADR-0009).
    let program = OsStr::new("definitely-not-a-real-command-toltrim");

    let run = run_toltrim(&dir, &[program], "a command that does not exist on Windows");

    assert_eq!(run.code(), 127);
    assert!(
        run.stdout.is_empty(),
        "toltrim wrote to stdout for a command that never started: {:?}",
        run.stdout_lossy()
    );
    let stderr = run.stderr_lossy();
    assert_eq!(
        stderr.lines().count(),
        1,
        "expected a single explanatory line, got {stderr:?}"
    );
    assert_eq!(
        stderr, "toltrim: definitely-not-a-real-command-toltrim: not found\n",
        "the retry's own report must become toltrim's one line"
    );
}

//! Verification of the claim itself (spec, Testing Decisions): the Savings Rate
//! published in `README.md` must be recomputable, offline, from the recorded
//! fixtures, and the Gain Records the binary writes must agree with the fixture
//! byte counts the pure seam produces.
//!
//! There are three moving parts:
//!
//! 1. [`INVOCATIONS`], the explicit manifest. Every fixture under
//!    `tests/fixtures/` that records an Invocation appears once, with its tool,
//!    its arguments, its exit code, and whether it was a Machine-Readable
//!    Request. Each entry carries its own provenance comment. A guard below
//!    asserts that every fixture file is either in the manifest or in
//!    [`NOT_AN_INVOCATION`], so a new fixture cannot be added without deciding
//!    what it is.
//! 2. The recomputation. Every fixture is pushed through the same pure seam the
//!    binary uses, `toltrim::trim::render`, and the Estimated Tokens are summed
//!    the way `toltrim gain` sums them: bytes / 4, rounded up, once per
//!    Invocation, over Proxied Commands only.
//! 3. The agreement. For one trimmed Invocation, one preserved one, and one
//!    Passthrough Command, a stand-in program replays the fixture bytes through
//!    the built binary, and `toltrim gain` is then asked to report what it
//!    recorded. Those numbers must equal the seam's.
//!
//! The README is the other side of the claim: the published figure and the
//! Invocation count are parsed out of it and compared with the recomputation, so
//! the number cannot drift away from the fixtures — or the fixtures away from
//! the number — without a failing test.

use std::fs;
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::process::{Command, Output, Stdio};
#[cfg(unix)]
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(unix)]
use std::time::{SystemTime, UNIX_EPOCH};

use toltrim::gain::tokens;
use toltrim::trim::{self, Mode, Tool};

/// The binary cargo built for this test run.
const BIN: &str = env!("CARGO_BIN_EXE_toltrim");

/// Where the fixtures and the README live.
const MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

/// One recorded Invocation.
struct Invocation {
    /// The fixture file that holds its stdout.
    fixture: &'static str,
    /// The program the caller typed, e.g. `mvn` or `git`.
    program: &'static str,
    /// The arguments after the program, verbatim.
    args: &'static [&'static str],
    /// The exit code the recorded run ended with.
    exit_code: i32,
    /// Was this a Machine-Readable Request? When true the seam is asked to
    /// preserve: it strips ANSI and removes nothing else.
    preserve: bool,
    /// Why this fixture is an Invocation, and where its bytes came from.
    note: &'static str,
}

/// Every recorded fixture that represents a real Invocation, once.
///
/// The list is the demo's fixture replay: the commands an agent fixes a failing
/// test with, in the shapes that were actually captured. Arguments are the ones
/// recorded with the fixture wherever the provenance names them, because a
/// Machine-Readable Request is a property of the arguments.
const INVOCATIONS: &[Invocation] = &[
    // ---- Maven -----------------------------------------------------------------
    Invocation {
        fixture: "mvn-test-pass.txt",
        program: "mvn",
        args: &["test"],
        exit_code: 0,
        preserve: false,
        note: "a recorded `mvn test` on a single-module project that passed: the \
               Success Summary path.",
    },
    Invocation {
        fixture: "mvn-test-fail-surefire.txt",
        program: "mvn",
        args: &["test"],
        exit_code: 1,
        preserve: false,
        note: "a recorded `mvn test` whose Surefire assertion failed: the Failure \
               Extract the demo turns on, and the fixture that proves an \
               `[ERROR]`-only rule would lose the assertion line.",
    },
    Invocation {
        fixture: "mvn-dependency-resolution-failure.txt",
        program: "mvn",
        args: &["test"],
        exit_code: 1,
        preserve: false,
        note: "a recorded `mvn test` that could not resolve junit:junit:4.13.2 \
               offline: the dependency-resolution failure path.",
    },
    Invocation {
        fixture: "mvn-compile-error.txt",
        program: "mvn",
        args: &["compile"],
        exit_code: 1,
        preserve: false,
        note: "recorded as `mvn compile` with one uncached dependency, so a \
               transfer-progress block sits just above the compile errors; the \
               recorded arguments are used here.",
    },
    Invocation {
        fixture: "mvn-quiet-test-fail.txt",
        program: "mvn",
        args: &["-o", "-q", "-Dstyle.color=always", "test"],
        exit_code: 1,
        preserve: true,
        note: "recorded as `mvn -o -q -Dstyle.color=always test`, a failing test \
               under `-q`. `-q` makes it a Machine-Readable Request, so it is \
               preserved (ANSI stripped only) and saves nothing.",
    },
    Invocation {
        fixture: "mvn-reactor-test-pass.txt",
        program: "mvn",
        args: &["-o", "test"],
        exit_code: 0,
        preserve: false,
        note: "recorded as `mvn -o test` in a three-module reactor: one Success \
               Summary line per module.",
    },
    // ---- git -------------------------------------------------------------------
    Invocation {
        fixture: "git-status-large-untracked.txt",
        program: "git",
        args: &["status", "--untracked-files=all"],
        exit_code: 0,
        preserve: false,
        note: "recorded as `git status --untracked-files=all`: the untracked flood \
               an agent pays for when every file under target/ is listed.",
    },
    Invocation {
        fixture: "git-status-porcelain.txt",
        program: "git",
        args: &["status", "--porcelain"],
        exit_code: 0,
        preserve: true,
        note: "recorded as `git status --porcelain`: a Machine-Readable Request, \
               preserved byte for byte, so it counts as a Proxied Command that \
               saved nothing.",
    },
    Invocation {
        fixture: "git-diff-metadata.txt",
        program: "git",
        args: &["diff", "-M", "--cached"],
        exit_code: 1,
        preserve: false,
        note: "recorded as `git diff -M --cached` after a rename with an edit, a \
               mode change, and a file with no trailing newline. Exit 1 means \
               \"there are differences\", not \"the command failed\".",
    },
    Invocation {
        fixture: "git-show-commit.txt",
        program: "git",
        args: &["show", "-M", "HEAD"],
        exit_code: 0,
        preserve: false,
        note: "recorded as `git show -M HEAD` of the commit made from the staged \
               changes above.",
    },
    Invocation {
        fixture: "git-log.txt",
        program: "git",
        args: &["log"],
        exit_code: 0,
        preserve: false,
        note: "a recorded `git log` reduced to hash, author, date, and subject.",
    },
    Invocation {
        fixture: "git-branch.txt",
        program: "git",
        args: &["branch"],
        exit_code: 0,
        preserve: false,
        note: "a recorded `git branch`: names only, current branch marked.",
    },
    Invocation {
        fixture: "git-branch-verbose.txt",
        program: "git",
        args: &["branch", "-v"],
        exit_code: 0,
        preserve: false,
        note: "recorded as `git branch -v`, whose ahead/behind decoration is Noise.",
    },
    // ---- file listings ---------------------------------------------------------
    Invocation {
        fixture: "ls-project-root.txt",
        program: "ls",
        args: &[],
        exit_code: 0,
        preserve: false,
        note: "recorded as `ls` in a real project root: the Noise directories \
               collapse to a count.",
    },
    Invocation {
        fixture: "ls-1-project-root.txt",
        program: "ls",
        args: &["-1"],
        exit_code: 0,
        preserve: false,
        note: "recorded as `ls -1` in the same root. It is a separate Invocation \
               with its own argument vector, not a transformation of the plain \
               `ls` capture, even though a piped `ls` happens to print the same \
               bytes.",
    },
    Invocation {
        fixture: "ls-l-project-root.txt",
        program: "ls",
        args: &["-l"],
        exit_code: 0,
        preserve: false,
        note: "recorded as `ls -l` in the same root: the long-listing shape.",
    },
    Invocation {
        fixture: "ls-la-project-root.txt",
        program: "ls",
        args: &["-la"],
        exit_code: 0,
        preserve: false,
        note: "recorded as `ls -la` in the same root, hidden entries included.",
    },
    Invocation {
        fixture: "ls-la-trailing-slash.txt",
        program: "ls",
        args: &["-la", "./"],
        exit_code: 0,
        preserve: false,
        note: "recorded as `ls -la \"$PWD/\"`: a second, independent capture of \
               the same directory used to show that a trailing slash cannot \
               change the reading. It was run, not derived from its twin, so it \
               is a separate Invocation.",
    },
    Invocation {
        fixture: "ls-columns-project-root.txt",
        program: "ls",
        args: &["-m"],
        exit_code: 0,
        preserve: false,
        note: "recorded as `ls -m`, the width-independent way to capture the \
               multi-column shape.",
    },
    Invocation {
        fixture: "ls-long-line.txt",
        program: "ls",
        args: &["-1"],
        exit_code: 0,
        preserve: false,
        note: "recorded as `ls -1` while a 130-character file name existed in the \
               root, so the truncation rule has something to truncate.",
    },
    Invocation {
        fixture: "tree-project-root.txt",
        program: "tree",
        args: &[],
        exit_code: 0,
        preserve: false,
        note: "a `tree` of the same root. This host has no `tree` binary \
               (`command -v tree` finds none), so the capture is COMPOSED in \
               tree's documented format from the real directory's contents; \
               tests/listing.rs says so too. It is kept in the manifest because \
               it is exactly the Invocation toltrim must handle on a machine that \
               does have `tree`.",
    },
    Invocation {
        fixture: "dir-project-root.txt",
        program: "dir",
        args: &[],
        exit_code: 0,
        preserve: false,
        note: "a Windows `dir` of the same root. This host is not Windows and has \
               no `dir`, so the capture is COMPOSED to be format-accurate for \
               cmd's `dir` (volume header, rows, byte footer); tests/listing.rs \
               says so too. Kept because it is the Invocation a Windows agent \
               runs.",
    },
];

/// Fixtures under `tests/fixtures/` that are deliberately not Invocations.
/// A guard asserts the directory and this list agree, so nothing is silently
/// left out of the published figure.
const NOT_AN_INVOCATION: &[(&str, &str)] = &[(
    "mvn-reactor-test-pass-crlf.txt",
    "a derived twin: the LF reactor capture with every line ending converted to \
     CRLF, not a second run. tests/maven_shapes.rs asserts the conversion is \
     exactly that. Counting it would count the reactor Invocation twice.",
)];

// ---------------------------------------------------------------------------
// Seam 1: recompute the figure from the fixtures.
// ---------------------------------------------------------------------------

fn fixture_path(name: &str) -> PathBuf {
    Path::new(MANIFEST_DIR)
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn fixture_bytes(name: &str) -> Vec<u8> {
    let path = fixture_path(name);
    fs::read(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

fn invocation_args(invocation: &Invocation) -> Vec<String> {
    invocation
        .args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect()
}

/// Read one fixture through the pure seam, exactly as `main.rs` does.
fn render(invocation: &Invocation) -> Vec<u8> {
    let args = invocation_args(invocation);
    trim::render(
        trim::detect(invocation.program),
        &args,
        &fixture_bytes(invocation.fixture),
        invocation.exit_code,
        invocation.preserve,
    )
}

fn tool_of(invocation: &Invocation) -> Tool {
    trim::detect(invocation.program)
}

/// What the manifest says the run is, checked against the code rather than
/// trusted. A flag that is dropped from the Machine-Readable Request list, or a
/// subcommand that stops being proxied, changes this and fails the test.
fn expected_mode(invocation: &Invocation) -> Mode {
    let args = invocation_args(invocation);
    let tool = tool_of(invocation);
    let preserve = trim::should_preserve(false, tool, &args, false, false);
    assert_eq!(
        preserve, invocation.preserve,
        "{}: the manifest says preserve={}, the code says {preserve}",
        invocation.fixture, invocation.preserve
    );
    trim::classify(tool, &args, preserve)
}

/// The recomputed view: Estimated Tokens in and out, summed per Invocation the
/// way `toltrim gain` sums Gain Records, over Proxied Commands only.
struct Recomputed {
    invocations: usize,
    trimmed: usize,
    preserved: usize,
    tokens_in: i64,
    tokens_out: i64,
}

fn recompute() -> Recomputed {
    let mut out = Recomputed {
        invocations: 0,
        trimmed: 0,
        preserved: 0,
        tokens_in: 0,
        tokens_out: 0,
    };
    for invocation in INVOCATIONS {
        let bytes_in = fixture_bytes(invocation.fixture);
        let bytes_out = render(invocation);
        match expected_mode(invocation) {
            Mode::Trimmed => out.trimmed += 1,
            Mode::Preserved => out.preserved += 1,
            Mode::Passthrough => panic!(
                "{} is in the savings manifest but is a Passthrough Command",
                invocation.fixture
            ),
        }
        out.invocations += 1;
        // The same per-record rounding the Gain Record uses (ADR-0007): one
        // rounding per Invocation, not one over the sum of bytes.
        out.tokens_in += tokens(bytes_in.len());
        out.tokens_out += tokens(bytes_out.len());
    }
    out
}

/// The Savings Rate exactly as the `gain` view states it: one decimal place,
/// over Proxied Commands only.
fn savings_rate(tokens_in: i64, tokens_out: i64) -> String {
    let rate = (tokens_in - tokens_out) as f64 / tokens_in as f64 * 100.0;
    format!("{rate:.1}%")
}

fn readme() -> String {
    let path = Path::new(MANIFEST_DIR).join("README.md");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// The published figure: `Savings Rate: <n>% (recorded fixtures, <n> Invocations)`.
/// Parsing it rather than containing it means the README line is a claim the
/// test can hold to account, not decoration.
fn published_claim(readme: &str) -> (String, usize) {
    const PREFIX: &str = "Savings Rate: ";
    for line in readme.lines() {
        let Some(rest) = line.trim().strip_prefix(PREFIX) else {
            continue;
        };
        let Some((rate, tail)) = rest.split_once("% (recorded fixtures, ") else {
            continue;
        };
        let Some(count) = tail.strip_suffix(" Invocations)") else {
            continue;
        };
        return (
            format!("{rate}%"),
            count.parse().unwrap_or_else(|e| {
                panic!("cannot parse the published Invocation count {count:?}: {e}")
            }),
        );
    }
    panic!("README.md publishes no `Savings Rate: <n>% (recorded fixtures, <n> Invocations)` line");
}

#[test]
fn every_fixture_is_either_an_invocation_or_explicitly_not_one() {
    let mut files: Vec<String> =
        fs::read_dir(Path::new(MANIFEST_DIR).join("tests").join("fixtures"))
            .expect("the fixtures directory is readable")
            .map(|entry| {
                entry
                    .expect("a readable directory entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .filter(|name| name.ends_with(".txt"))
            .collect();
    files.sort();

    let mut known: Vec<String> = INVOCATIONS
        .iter()
        .map(|invocation| invocation.fixture.to_string())
        .chain(
            NOT_AN_INVOCATION
                .iter()
                .map(|(name, _)| (*name).to_string()),
        )
        .collect();
    known.sort();

    assert_eq!(
        files, known,
        "every fixture must be either an Invocation in the manifest or listed as \
         not one; a new fixture has to be classified"
    );
    // No fixture is counted twice.
    let mut unique = known.clone();
    unique.dedup();
    assert_eq!(unique.len(), known.len(), "a fixture is listed twice");

    // Reading the justification here keeps it part of the manifest rather than
    // decoration: an entry without one is an unexamined entry.
    for invocation in INVOCATIONS {
        assert!(
            invocation.note.len() >= 20,
            "{} is in the manifest with no provenance comment",
            invocation.fixture
        );
    }
    for (name, reason) in NOT_AN_INVOCATION {
        assert!(
            reason.len() >= 20,
            "{name} is excluded from the manifest with no reason"
        );
    }
}

#[test]
fn the_published_savings_rate_is_the_one_the_fixtures_produce() {
    let recomputed = recompute();
    let (published_rate, published_count) = published_claim(&readme());

    let expected_rate = savings_rate(recomputed.tokens_in, recomputed.tokens_out);

    assert_eq!(
        published_count, recomputed.invocations,
        "README publishes {published_count} Invocations, the manifest has {}",
        recomputed.invocations
    );
    assert_eq!(
        published_rate,
        expected_rate,
        "README publishes {published_rate} over {} Invocations \
         ({} trimmed, {} preserved; {} Estimated Tokens in, {} out); \
         the fixtures produce {expected_rate}",
        recomputed.invocations,
        recomputed.trimmed,
        recomputed.preserved,
        recomputed.tokens_in,
        recomputed.tokens_out
    );
    // The denominator has to be real: a manifest of empty fixtures would make
    // the division above meaningless.
    assert!(recomputed.tokens_in > 0, "no Estimated Tokens in");
    assert!(
        recomputed.tokens_out < recomputed.tokens_in,
        "the fixtures record no saving at all"
    );
}

// ---------------------------------------------------------------------------
// Seam 2: the Gain Records the binary writes must agree with the seam.
// ---------------------------------------------------------------------------

/// A directory that outlives one test and is removed when it ends.
#[cfg(unix)]
struct TempDir {
    path: PathBuf,
}

#[cfg(unix)]
impl TempDir {
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
            "toltrim-savings-{label}-{}-{nanos}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path)
            .unwrap_or_else(|e| panic!("cannot create {}: {e}", path.display()));
        TempDir { path }
    }

    fn db(&self) -> PathBuf {
        self.path.join("history.db")
    }
}

#[cfg(unix)]
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Write a stand-in program that replays one fixture's bytes and exits with the
/// recorded code. Its file name carries the tool: a stand-in named `git` is a
/// Proxied Command, and anything else is a Passthrough Command.
#[cfg(unix)]
fn stand_in(dir: &TempDir, name: &str, fixture: &str, exit_code: i32) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let path = dir.path.join(name);
    let body = format!(
        "#!/bin/sh\ncat '{}'\nexit {exit_code}\n",
        fixture_path(fixture).display()
    );
    fs::write(&path, body).unwrap_or_else(|e| panic!("cannot write {}: {e}", path.display()));
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|e| panic!("cannot make {} executable: {e}", path.display()));
    path
}

/// Run the built binary with `args`, this test's own database, and no terminal.
#[cfg(unix)]
fn run(dir: &TempDir, args: &[&std::ffi::OsStr]) -> Output {
    let mut cmd = Command::new(BIN);
    cmd.args(args);
    cmd.env("TOLTRIM_DB", dir.db());
    cmd.current_dir(&dir.path);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    let output = cmd
        .output()
        .unwrap_or_else(|e| panic!("cannot start {BIN}: {e}"));
    assert!(
        output.stderr.is_empty(),
        "toltrim wrote to stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

/// Replay one manifest entry through the built binary and return what
/// `toltrim gain` reports about the single Gain Record it wrote.
#[cfg(unix)]
fn replay(invocation: &Invocation, label: &str) -> (Output, String) {
    let dir = TempDir::new(label);
    let program = stand_in(
        &dir,
        invocation.program,
        invocation.fixture,
        invocation.exit_code,
    );
    let mut args: Vec<std::ffi::OsString> = vec![program.into_os_string()];
    args.extend(invocation.args.iter().map(std::ffi::OsString::from));

    let replayed = run(
        &dir,
        &args.iter().map(|a| a.as_os_str()).collect::<Vec<_>>(),
    );
    assert_eq!(
        replayed.status.code(),
        Some(invocation.exit_code),
        "{}: the exit code was not forwarded verbatim",
        invocation.fixture
    );

    let gain = run(&dir, &[std::ffi::OsStr::new("gain")]);
    assert_eq!(gain.status.code(), Some(0), "toltrim gain failed");
    let view = String::from_utf8(gain.stdout).expect("the gain view is UTF-8");
    (replayed, view)
}

/// The one number a named label reports, e.g. `Input tokens: 42`. Values may be
/// K/M abbreviated (`1.6K`), so the abbreviation is expanded before parsing.
#[cfg(unix)]
fn reported(view: &str, label: &str) -> i64 {
    let prefix = format!("{label}: ");
    let value = view
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .unwrap_or_else(|| panic!("the gain view has no {label:?} line:\n{view}"));
    let cell = value
        .split_whitespace()
        .next()
        .unwrap_or_else(|| panic!("{label} has no value in:\n{view}"))
        .to_string();
    parse_cell(&cell, label, view)
}

/// Expand the view's K/M abbreviation: `1.6K` -> 1600, `2M` -> 2_000_000.
#[cfg(unix)]
fn parse_cell(cell: &str, label: &str, view: &str) -> i64 {
    let (number, multiplier) = match cell.chars().last() {
        Some('K') => (&cell[..cell.len() - 1], 1_000),
        Some('M') => (&cell[..cell.len() - 1], 1_000_000),
        _ => (cell, 1),
    };
    number
        .parse::<f64>()
        .map(|value| (value * multiplier as f64).round() as i64)
        .unwrap_or_else(|e| panic!("cannot parse {label} from {cell:?} in:\n{view}\n{e}"))
}

#[cfg(unix)]
#[test]
fn a_trimmed_invocation_is_recorded_with_the_seams_byte_counts() {
    let invocation = INVOCATIONS
        .iter()
        .find(|invocation| invocation.fixture == "git-diff-metadata.txt")
        .expect("the manifest names the recorded diff");
    let expected_in = tokens(fixture_bytes(invocation.fixture).len());
    let expected_out = tokens(render(invocation).len());

    let (replayed, view) = replay(invocation, "trimmed");
    assert_eq!(
        replayed.stdout,
        render(invocation),
        "the trimmed output the binary emitted differs from the seam's"
    );

    assert_eq!(
        reported(&view, "Input tokens"),
        expected_in,
        "the Gain Record disagrees with the fixture's byte count:\n{view}"
    );
    assert_eq!(
        reported(&view, "Output tokens"),
        expected_out,
        "the Gain Record disagrees with the seam's output size:\n{view}"
    );
    assert_eq!(
        reported(&view, "Tokens saved"),
        expected_in - expected_out,
        "the recorded saving is not in - out:\n{view}"
    );
    assert!(
        view.contains("Commands proxied: 1 (trimmed 1, preserved 0)"),
        "a trimmed Proxied Command must be counted as trimmed:\n{view}"
    );
    assert!(
        view.contains(&format!(
            "Savings Rate: {} of the original, over Proxied Commands only",
            savings_rate(expected_in, expected_out)
        )),
        "the recorded rate is not the one the fixture bytes produce:\n{view}"
    );
}

#[cfg(unix)]
#[test]
fn a_machine_readable_request_is_recorded_as_preserved() {
    let invocation = INVOCATIONS
        .iter()
        .find(|invocation| invocation.fixture == "git-status-porcelain.txt")
        .expect("the manifest names the porcelain status");
    let expected_in = tokens(fixture_bytes(invocation.fixture).len());
    let expected_out = tokens(render(invocation).len());
    assert_eq!(
        expected_in, expected_out,
        "this test is only meaningful while a porcelain status has no ANSI to strip"
    );

    let (replayed, view) = replay(invocation, "preserved");
    assert_eq!(
        replayed.stdout,
        fixture_bytes(invocation.fixture),
        "a Machine-Readable Request must arrive byte-identical"
    );
    assert_eq!(reported(&view, "Input tokens"), expected_in, "{view}");
    assert_eq!(reported(&view, "Output tokens"), expected_out, "{view}");
    assert!(
        view.contains("Commands proxied: 1 (trimmed 0, preserved 1)"),
        "`--porcelain` must count as preserved, not trimmed:\n{view}"
    );
    assert!(
        view.contains("Savings Rate: 0.0% of the original, over Proxied Commands only"),
        "a preserved Invocation saves nothing:\n{view}"
    );
}

#[cfg(unix)]
#[test]
fn a_command_with_no_rule_is_a_passthrough_command_outside_the_rate() {
    // `cat` is out of scope by ADR-0008, so its Invocation is a Passthrough
    // Command: emitted byte for byte, recorded, and excluded from the rate.
    let passthrough = Invocation {
        fixture: "mvn-test-pass.txt",
        program: "cat",
        args: &[],
        exit_code: 0,
        preserve: false,
        note: "a stand-in for a command toltrim has no rule for",
    };

    let (replayed, view) = replay(&passthrough, "passthrough");
    assert_eq!(
        replayed.stdout,
        fixture_bytes(passthrough.fixture),
        "a Passthrough Command must be byte-identical to running it alone"
    );
    // The Passthrough Command's bytes are recorded in the database and counted
    // on its own line; they never enter the rate, so the view publishes no
    // Proxied Estimated Tokens for them.
    assert!(
        view.contains("Passthrough:      1 (excluded from the Savings Rate)"),
        "a command with no rule must be recorded as a Passthrough Command:\n{view}"
    );
    assert!(
        view.contains("Commands proxied: 0 (trimmed 0, preserved 0)"),
        "a Passthrough Command must not enter the Proxied Command count:\n{view}"
    );
    assert!(
        view.contains("Savings Rate: not yet measurable"),
        "a Passthrough Command must not flatter the Savings Rate:\n{view}"
    );
}

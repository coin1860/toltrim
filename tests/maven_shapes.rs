//! Four more recorded Maven shapes: a three-module reactor, a compile error
//! behind a transfer-progress block, a quiet (`-q`) run, and a CRLF-terminated
//! reactor run. The three LF fixtures are real stdout, recorded with the command
//! that produced them:
//!
//! * `mvn-reactor-test-pass.txt` — `cd reactor && mvn -o test` on a parent POM
//!   with `alpha` (2 passing tests), `beta` (no tests at all) and `gamma`
//!   (3 passing tests), Maven 3.9.12 against the warm local repository.
//! * `mvn-compile-error.txt` — `cd broken && MAVEN_OPTS='-Duser.language=en
//!   -Duser.country=US -Duser.script=Latn' mvn compile`, with one uncached
//!   dependency so the download block lands just before the compile errors. The
//!   `MAVEN_OPTS` locale keeps javac's diagnostics in the documented English
//!   shape (`symbol:` / `location:`).
//! * `mvn-quiet-test-fail.txt` — `cd quiet && MAVEN_OPTS='-Duser.language=en
//!   -Duser.country=US -Duser.script=Latn' mvn -o -q -Dstyle.color=always test`,
//!   a failing test under `-q`. `-Dstyle.color=always` is what puts real ANSI
//!   codes in the capture, so "passed through with ANSI stripped" is observable
//!   rather than trivially true.
//! * `mvn-reactor-test-pass-crlf.txt` — derived: the LF reactor run with every
//!   line ending converted to CRLF, which is what Maven writes on Windows. This
//!   machine is not Windows, and re-recording under a line-ending filter would
//!   carry different timings, so the CRLF fixture could not then be compared
//!   against its LF twin. The conversion is asserted to be exactly that: no
//!   other byte changed.
//!
//! Every test asserts both directions — the Evidence that must survive and the
//! Noise that must not — and none of them counts kept lines or window sizes.

use std::fs;

use toltrim::trim::{self, Tool};

fn fixture_bytes(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    fs::read(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"))
}

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"))
}

/// Read a fixture through `render`, exactly as the entry point does.
fn render(args: &[&str], raw: &str, code: i32, preserve: bool) -> String {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let bytes = trim::render(Tool::Maven, &args, raw.as_bytes(), code, preserve);
    String::from_utf8(bytes).expect("fixtures are valid UTF-8")
}

/// The one-line-per-module Success Summary of the recorded reactor, with each
/// module's own test totals. `beta` ran no tests, so it has no count segment: a
/// build that ran no tests must never claim they passed.
const REACTOR_SUMMARY: [&str; 4] = [
    "reactor: BUILD SUCCESS | 0.978 s | W:0",
    "alpha: BUILD SUCCESS | Tests run: 2, Failures: 0, Errors: 0, Skipped: 0 | 0.978 s | W:0",
    "beta: BUILD SUCCESS | 0.978 s | W:0",
    "gamma: BUILD SUCCESS | Tests run: 3, Failures: 0, Errors: 0, Skipped: 0 | 0.978 s | W:0",
];

fn assert_reactor_summary(out: &str) {
    for line in REACTOR_SUMMARY {
        assert!(out.contains(line), "missing Success Summary {line:?} in:\n{out}");
    }

    // Attribution, module by module: no module is given another module's totals,
    // and the module that ran no tests carries no total at all.
    assert!(!out.contains("alpha: BUILD SUCCESS | Tests run: 3"), "alpha got gamma's totals:\n{out}");
    assert!(!out.contains("gamma: BUILD SUCCESS | Tests run: 2"), "gamma got alpha's totals:\n{out}");
    assert!(!out.contains("beta: BUILD SUCCESS | Tests run"), "beta claimed tests it never ran:\n{out}");

    // The summary is one line per module in reactor order; that order is the
    // contract the ticket names, not an internal detail of a window.
    let order = [
        "reactor: BUILD SUCCESS",
        "alpha: BUILD SUCCESS",
        "beta: BUILD SUCCESS",
        "gamma: BUILD SUCCESS",
    ];
    let positions: Vec<usize> = order
        .iter()
        .map(|label| out.find(label).unwrap_or_else(|| panic!("{label:?} missing from:\n{out}")))
        .collect();
    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "modules are not in reactor order:\n{out}"
    );
}

fn assert_reactor_noise_is_gone(out: &str) {
    assert!(!out.contains("Scanning for projects"));
    assert!(!out.contains("Reactor Build Order"));
    assert!(!out.contains("--- resources:"));
    assert!(!out.contains("--- compiler:"));
    assert!(!out.contains("--- surefire:"));
    assert!(!out.contains("T E S T S"));
    assert!(!out.contains("Running demo."));
    assert!(!out.contains("Time elapsed"));
    assert!(!out.contains("Reactor Summary"));
    assert!(!out.contains("Finished at"));
    // Only the count is reported, never the warning text.
    assert!(!out.contains("[WARNING]"));
}

#[test]
fn a_reactor_run_gives_one_summary_per_module_with_that_modules_own_counts() {
    let raw = fixture("mvn-reactor-test-pass.txt");
    let out = render(&[], &raw, 0, false);
    assert_reactor_summary(&out);
    assert_reactor_noise_is_gone(&out);
}

#[test]
fn a_compile_error_keeps_every_file_and_line_and_the_detail_that_explains_it() {
    let raw = fixture("mvn-compile-error.txt");
    let out = render(&[], &raw, 1, false);

    // Every compilation error keeps its file and line: that pair is what lets an
    // agent open the right file, so no window may reach past it.
    assert!(out.contains("Broken.java:[5,16] cannot find symbol"), "got:\n{out}");
    assert!(
        out.contains("Broken.java:[9,16] incompatible types: int cannot be converted to java.lang.String"),
        "got:\n{out}"
    );
    assert!(
        out.contains("Other.java:[5,21] incompatible types: java.lang.String cannot be converted to int"),
        "got:\n{out}"
    );
    // The `symbol:` / `location:` lines that explain the first error carry no
    // `[ERROR]` prefix, exactly like a Surefire assertion line, so a rule that
    // kept only marked lines would leave the agent without the cause.
    assert!(out.contains("\n  symbol:   variable missingThing"), "got:\n{out}");
    assert!(out.contains("\n  location: class demo.broken.Broken"), "got:\n{out}");
    assert!(out.contains("COMPILATION ERROR"));
    assert!(out.contains("BUILD FAILURE"));

    // Noise: the plugin banners, the compile chatter above the error, the
    // transfer-progress block above those, and Maven's advice below.
    assert!(!out.contains("Scanning for projects"));
    assert!(!out.contains("--- resources:"));
    assert!(!out.contains("--- compiler:"));
    assert!(!out.contains("skip non existing resourceDirectory"));
    assert!(!out.contains("Recompiling the module"));
    assert!(!out.contains("Compiling 2 source files"));
    assert!(!out.contains("Downloading from"));
    assert!(!out.contains("Downloaded from"));
    assert!(!out.contains("Progress ("));
    assert!(!out.contains("Help 1"));
    assert!(!out.contains("-e switch"));
    assert!(!out.contains("Re-run Maven using the -X switch"));
    // The progress bar overwrites itself with carriage returns; the failure
    // extract must not carry one into the agent's output.
    assert!(!out.contains('\r'), "a carriage return leaked from the progress bar:\n{out}");
}

#[test]
fn a_quiet_run_is_a_machine_readable_request_that_loses_only_ansi() {
    let bytes = fixture_bytes("mvn-quiet-test-fail.txt");
    let args: Vec<String> = vec!["-q".into(), "test".into()];

    // The preservation decision is itself a pure function of the seam's inputs.
    assert!(
        trim::should_preserve(false, Tool::Maven, &args, false, false),
        "-q makes a Maven run a Machine-Readable Request"
    );

    // The seam returns the ANSI-stripped bytes and removes nothing else. The
    // recorded run carries real ANSI codes, so this is not the identity function
    // and "nothing removed" cannot be satisfied by accident.
    let text = std::str::from_utf8(&bytes).expect("fixtures are valid UTF-8");
    let stripped = toltrim::ansi::strip(text).into_bytes();
    assert_ne!(stripped, bytes, "the quiet fixture must carry ANSI codes to prove stripping");
    let out = trim::render(Tool::Maven, &args, &bytes, 1, true);
    assert_eq!(out, stripped, "a quiet run must be passed through with ANSI stripped");
    let out = String::from_utf8(out).expect("ANSI-stripped output is valid UTF-8");

    // Evidence survives, because nothing was removed.
    assert!(out.contains("expected: <7> but was: <2>"), "got:\n{out}");
    assert!(out.contains("CounterTest.incrementsAgain:17"));

    // So does the Noise — that is the whole point of a Machine-Readable Request.
    // The help URL and the re-run advice are still here even though the trim rule
    // drops them, which is what makes the preservation load-bearing.
    assert!(out.contains("[Help 1]"));
    assert!(out.contains("-e switch"));
    let trimmed = render(&["-q", "test"], text, 1, false);
    assert!(trimmed.contains("expected: <7> but was: <2>"), "got:\n{trimmed}");
    assert!(!trimmed.contains("Help 1"), "the trim rule should have dropped the help URL");
    assert!(!trimmed.contains("-e switch"), "the trim rule should have dropped the re-run advice");
}

#[test]
fn a_crlf_run_summarises_identically_to_its_lf_twin() {
    let lf = fixture("mvn-reactor-test-pass.txt");
    let crlf = fixture("mvn-reactor-test-pass-crlf.txt");

    // Provenance: the Windows fixture is the recorded LF run with only its line
    // endings changed, so a difference in the output cannot come from elsewhere.
    assert!(crlf.contains("\r\n"), "the Windows fixture must be CRLF-terminated");
    assert_eq!(crlf.replace("\r\n", "\n"), lf, "CRLF fixture is not a pure line-ending conversion");

    let out_lf = render(&[], &lf, 0, false);
    let out_crlf = render(&[], &crlf, 0, false);

    // The same run, so the same Success Summary.
    assert_eq!(out_crlf, out_lf, "CRLF changed the Success Summary");
    // No carriage return survives, and a CRLF is not read as a blank line.
    assert!(!out_crlf.contains('\r'), "a carriage return survived: {out_crlf:?}");
    assert!(!out_crlf.contains("\n\n"), "a spurious blank line appeared: {out_crlf:?}");
    // The same Evidence, and the same silence about the Noise.
    assert_reactor_summary(&out_crlf);
    assert_reactor_noise_is_gone(&out_crlf);
}

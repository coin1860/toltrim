//! The claim this tool makes is checked here against output recorded from real
//! Maven runs: every line an agent needs must survive trimming, and the noise
//! must not. A test that only asserted "the output got smaller" would pass while
//! the tool quietly deleted the reason a build failed.
//!
//! Everything here goes through the pure render seam, so no process is started.

use std::fs;

use toltrim::trim::{self, Tool};

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"))
}

/// Read the fixture bytes through `render`, exactly as the entry point does.
fn render(tool: Tool, args: &[&str], raw: &str, code: i32, preserve: bool) -> String {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let bytes = trim::render(tool, &args, raw.as_bytes(), code, preserve);
    String::from_utf8(bytes).expect("fixtures are valid UTF-8")
}

#[test]
fn a_real_surefire_failure_keeps_the_assertion_that_explains_it() {
    let raw = fixture("mvn-test-fail-surefire.txt");
    let out = render(Tool::Maven, &["test"], &raw, 1, false);

    // Must survive. The assertion line carries no `[ERROR]` prefix, so a rule
    // that kept only marked lines would drop it and leave the agent guessing.
    assert!(
        out.contains("expected:<alice[]> but was:<alice[ ]>"),
        "the assertion was trimmed away:\n{out}"
    );
    assert!(out.contains("demo.FooTest.boom"));
    assert!(out.contains("BUILD FAILURE"));

    // Must not survive.
    assert!(!out.contains("Scanning for projects"));
    assert!(!out.contains("Help 1"));
    assert!(!out.contains("-X switch"));
    assert!(
        !out.contains("Downloaded from central"),
        "transfer noise leaked into the context window:\n{out}"
    );
    assert!(!out.contains("Progress ("));

    // No hole in the stack trace: a middle frame must be present, not only the
    // first frames and the tail.
    assert!(
        out.contains("ParentRunner"),
        "the stack trace has a hole in the middle:\n{out}"
    );
    assert!(out.contains("ForkedBooter.main"));

    assert!(
        out.len() * 5 < raw.len(),
        "expected a large saving, got {} -> {} bytes",
        raw.len(),
        out.len()
    );
}

#[test]
fn a_successful_run_becomes_one_line_per_module() {
    let raw = fixture("mvn-test-pass.txt");
    let out = render(Tool::Maven, &["test"], &raw, 0, false);

    assert!(out.contains("demo: BUILD SUCCESS"), "got:\n{out}");
    assert!(out.contains("Tests run: 2, Failures: 0, Errors: 0, Skipped: 0"));
    assert_eq!(out.trim_end().lines().count(), 1, "got:\n{out}");
    assert!(!out.contains("Scanning for projects"));
    assert!(!out.contains("T E S T S"));
    assert!(out.len() < 200, "got {} bytes:\n{out}", out.len());
}

#[test]
fn a_dependency_failure_keeps_the_cause() {
    let raw = fixture("mvn-dependency-resolution-failure.txt");
    let out = render(Tool::Maven, &["test"], &raw, 1, false);

    assert!(
        out.contains("Could not resolve dependencies"),
        "got:\n{out}"
    );
    assert!(out.contains("Cannot access central"));
    assert!(!out.contains("Help 1"));
}

#[test]
fn an_unproxied_command_passes_through_the_seam_unchanged() {
    let raw = fixture("mvn-test-pass.txt");
    let out = render(Tool::Unknown, &[], &raw, 0, false);
    assert_eq!(out, raw, "an unknown command must be byte-identical");
}

#[test]
fn a_machine_readable_maven_run_is_preserved() {
    let raw = fixture("mvn-test-pass.txt");
    let args = vec!["-q".to_string(), "test".to_string()];
    // The preservation decision is itself a pure function of the seam's inputs.
    let preserve = trim::should_preserve(false, Tool::Maven, &args, false, false);
    let out = render(Tool::Maven, &["-q", "test"], &raw, 0, preserve);
    assert_eq!(out, raw, "a quiet run is a Machine-Readable Request");
}

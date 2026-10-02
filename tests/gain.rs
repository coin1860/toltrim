//! Gain Records and the `toltrim gain` view, through the built binary — Seam 2.
//!
//! Environment variables are process-global, so every child process gets its own
//! `TOLTRIM_DB` inside a throwaway directory. No test can reach the developer's
//! real user-level database. Proxied Commands are stand-in programs named `mvn`
//! (Maven, trimmed) and `gcloud` (unknown, a Passthrough Command): each replays
//! fixed bytes and exits with a chosen code, so the recorded byte counts are
//! known exactly.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use toltrim::trim::RULES_VERSION;

const BIN: &str = env!("CARGO_BIN_EXE_toltrim");

/// A throwaway directory that deletes itself.
struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("the clock is after 1970")
            .as_nanos();
        let unique = UNIQUE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "toltrim-gain-{label}-{}-{nanos}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("a writable temp directory");
        TempDir(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    /// A database path whose parent directory does not exist yet, so every test
    /// also covers "the parent directory is created if missing".
    fn db(&self) -> PathBuf {
        self.0.join("data").join("history.db")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

static UNIQUE: AtomicUsize = AtomicUsize::new(0);

/// Write a stand-in command named `name` that replays `body` and exits with
/// `exit_code`. Returns what to pass toltrim as the command to proxy.
///
/// The name is the whole point: `mvn` is detected as Maven and trimmed, while
/// `gcloud` is an unknown tool and therefore a Passthrough Command.
fn stand_in(dir: &Path, name: &str, body: &[u8], exit_code: i32) -> String {
    let output = dir.join(format!("{name}-output"));
    fs::write(&output, body).expect("the stand-in output is writable");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let script = dir.join(name);
        fs::write(
            &script,
            format!("#!/bin/sh\ncat '{}'\nexit {exit_code}\n", output.display()),
        )
        .expect("the stand-in script is writable");
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755))
            .expect("the stand-in script is executable");
        script.to_string_lossy().into_owned()
    }

    #[cfg(windows)]
    {
        // Windows cannot start a `.cmd` directly, so it is reached by bare name
        // through `PATH` and `PATHEXT`.
        let script = dir.join(format!("{name}.cmd"));
        fs::write(
            &script,
            format!(
                "@echo off\r\ntype \"{}\"\r\nexit /b {exit_code}\r\n",
                output.display()
            ),
        )
        .expect("the stand-in script is writable");
        name.to_string()
    }
}

/// One toltrim Invocation with its own working directory, database, and
/// stand-in directory on `PATH`.
fn toltrim_in(cwd: &Path, db: &Path, tools: &Path, args: &[&str]) -> Output {
    let mut path = std::ffi::OsString::from(tools);
    if let Some(existing) = std::env::var_os("PATH") {
        path.push(if cfg!(windows) { ";" } else { ":" });
        path.push(existing);
    }
    Command::new(BIN)
        .args(args)
        .current_dir(cwd)
        .env("TOLTRIM_DB", db)
        .env("PATH", path)
        .output()
        .expect("the built binary runs")
}

fn text_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[track_caller]
fn must_contain(text: &str, needle: &str) {
    assert!(text.contains(needle), "expected {needle:?} in:\n{text}");
}

/// The numbers of one command's row in the per-command table.
#[track_caller]
fn table_row(view: &str, label: &str) -> Vec<i64> {
    let table = view
        .split_once("COMMAND")
        .map(|(_, table)| table)
        .unwrap_or_else(|| panic!("no per-command table in:\n{view}"));
    let prefix = format!("{label} ");
    let line = table
        .lines()
        .find(|line| line.starts_with(&prefix))
        .unwrap_or_else(|| panic!("no `{label}` row in:\n{view}"));
    line.split_whitespace()
        .skip(label.split_whitespace().count())
        .map(|cell| cell.parse().expect("a table cell is a number"))
        .collect()
}

fn line_starting_with<'a>(text: &'a str, prefix: &str) -> &'a str {
    text.lines()
        .find(|line| line.starts_with(prefix))
        .unwrap_or_else(|| panic!("no line starting with {prefix:?} in:\n{text}"))
}

/// A recorded successful Maven run, replayed by a stand-in so the recorded byte
/// counts are exact.
fn maven_pass_fixture() -> Vec<u8> {
    fs::read(format!(
        "{}/tests/fixtures/mvn-test-pass.txt",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("the fixture is readable")
}

/// Bytes / 4, rounded up — recomputed here so the view's numbers are checked
/// against the definition rather than against the implementation.
fn estimated_tokens(bytes: usize) -> i64 {
    (i64::try_from(bytes).expect("a fixture fits in i64") + 3) / 4
}

#[test]
fn one_trimmed_invocation_is_one_proxied_command_with_estimated_tokens_and_a_rate() {
    let db = TempDir::new("one-db");
    let cwd = TempDir::new("one-cwd");
    let tools = TempDir::new("one-tools");
    let body = maven_pass_fixture();
    let program = stand_in(tools.path(), "mvn", &body, 0);

    let proxied = toltrim_in(cwd.path(), &db.db(), tools.path(), &[&program, "test"]);
    assert_eq!(proxied.status.code(), Some(0));
    let trimmed = text_of(&proxied);
    must_contain(&trimmed, "BUILD SUCCESS");
    assert!(
        trimmed.len() < body.len(),
        "the Proxied Command was not trimmed: {} -> {} bytes",
        body.len(),
        trimmed.len()
    );

    let view = toltrim_in(cwd.path(), &db.db(), tools.path(), &["gain"]);
    assert_eq!(view.status.code(), Some(0));
    assert!(stderr_of(&view).is_empty(), "stderr: {}", stderr_of(&view));
    let text = text_of(&view);

    let tokens_in = estimated_tokens(body.len());
    let tokens_out = estimated_tokens(trimmed.len());
    let saved = tokens_in - tokens_out;
    let rate = format!("{:.1}%", saved as f64 / tokens_in as f64 * 100.0);

    must_contain(&text, "Commands proxied: 1 (trimmed 1, preserved 0)");
    must_contain(
        &text,
        "Passthrough Commands: 0 (excluded from the Savings Rate)",
    );
    must_contain(&text, &format!("Estimated Tokens in: {tokens_in}"));
    must_contain(&text, &format!("Estimated Tokens out: {tokens_out}"));
    must_contain(&text, &format!("Estimated Tokens saved: {saved}"));
    must_contain(
        &text,
        &format!("Savings Rate: {rate} of the original, over Proxied Commands only"),
    );
    assert_eq!(
        table_row(&text, "maven test"),
        vec![1, tokens_in, tokens_out, saved],
        "view:\n{text}"
    );
}

#[test]
fn a_passthrough_command_is_recorded_counted_separately_and_excluded_from_the_rate() {
    let body = maven_pass_fixture();
    let chatter: &[u8] = b"gcloud: chatter that no rule trims\n";

    // One Proxied Command alone.
    let solo_db = TempDir::new("solo-db");
    let solo_cwd = TempDir::new("solo-cwd");
    let solo_tools = TempDir::new("solo-tools");
    let solo_mvn = stand_in(solo_tools.path(), "mvn", &body, 0);
    toltrim_in(
        solo_cwd.path(),
        &solo_db.db(),
        solo_tools.path(),
        &[&solo_mvn, "test"],
    );
    let solo = text_of(&toltrim_in(
        solo_cwd.path(),
        &solo_db.db(),
        solo_tools.path(),
        &["gain"],
    ));

    // The same Proxied Command, plus a Passthrough Command.
    let both_db = TempDir::new("both-db");
    let both_cwd = TempDir::new("both-cwd");
    let both_tools = TempDir::new("both-tools");
    let both_mvn = stand_in(both_tools.path(), "mvn", &body, 0);
    let gcloud = stand_in(both_tools.path(), "gcloud", chatter, 0);
    toltrim_in(
        both_cwd.path(),
        &both_db.db(),
        both_tools.path(),
        &[&both_mvn, "test"],
    );
    let passed = toltrim_in(
        both_cwd.path(),
        &both_db.db(),
        both_tools.path(),
        &[&gcloud, "info"],
    );
    assert_eq!(
        passed.stdout.as_slice(),
        chatter,
        "a Passthrough Command must keep its bytes"
    );
    let both = text_of(&toltrim_in(
        both_cwd.path(),
        &both_db.db(),
        both_tools.path(),
        &["gain"],
    ));

    // Counted separately, and shown: their Estimated Tokens appear, unchanged.
    must_contain(&both, "Commands proxied: 1 (trimmed 1, preserved 0)");
    must_contain(
        &both,
        "Passthrough Commands: 1 (excluded from the Savings Rate)",
    );
    must_contain(
        &both,
        &format!(
            "Passthrough Estimated Tokens in: {}",
            estimated_tokens(chatter.len())
        ),
    );
    assert_eq!(
        table_row(&both, "passthrough"),
        vec![
            1,
            estimated_tokens(chatter.len()),
            estimated_tokens(chatter.len()),
            0
        ],
        "view:\n{both}"
    );

    // Excluded from the rate: every Proxied figure is unchanged by a Passthrough
    // Command that saved nothing.
    let proxied_lines = |text: &str| {
        [
            line_starting_with(text, "Commands proxied:").to_string(),
            line_starting_with(text, "Estimated Tokens in:").to_string(),
            line_starting_with(text, "Estimated Tokens out:").to_string(),
            line_starting_with(text, "Estimated Tokens saved:").to_string(),
            line_starting_with(text, "Savings Rate:").to_string(),
        ]
    };
    assert_eq!(proxied_lines(&both), proxied_lines(&solo));
}

#[test]
fn a_lone_passthrough_command_leaves_the_rate_not_yet_measurable() {
    let db = TempDir::new("lone-db");
    let cwd = TempDir::new("lone-cwd");
    let tools = TempDir::new("lone-tools");
    let chatter: &[u8] = b"gcloud: chatter\n";
    let gcloud = stand_in(tools.path(), "gcloud", chatter, 0);

    toltrim_in(cwd.path(), &db.db(), tools.path(), &[&gcloud, "info"]);
    let text = text_of(&toltrim_in(cwd.path(), &db.db(), tools.path(), &["gain"]));

    must_contain(&text, "Commands proxied: 0 (trimmed 0, preserved 0)");
    must_contain(
        &text,
        "Passthrough Commands: 1 (excluded from the Savings Rate)",
    );
    must_contain(
        &text,
        "Savings Rate: not yet measurable (no Proxied Commands recorded)",
    );
    assert!(!text.contains("NaN"), "view:\n{text}");
    assert_eq!(
        table_row(&text, "passthrough"),
        vec![
            1,
            estimated_tokens(chatter.len()),
            estimated_tokens(chatter.len()),
            0
        ],
        "view:\n{text}"
    );
}

#[test]
fn the_per_command_table_reports_runs_and_estimated_tokens_per_command() {
    let db = TempDir::new("table-db");
    let cwd = TempDir::new("table-cwd");
    let tools = TempDir::new("table-tools");
    let body = maven_pass_fixture();
    let chatter: &[u8] = b"gcloud: chatter\n";
    let program = stand_in(tools.path(), "mvn", &body, 0);
    let gcloud = stand_in(tools.path(), "gcloud", chatter, 0);

    let trimmed = text_of(&toltrim_in(
        cwd.path(),
        &db.db(),
        tools.path(),
        &[&program, "test"],
    ));
    toltrim_in(cwd.path(), &db.db(), tools.path(), &[&program, "test"]);
    toltrim_in(cwd.path(), &db.db(), tools.path(), &[&gcloud, "info"]);

    let text = text_of(&toltrim_in(cwd.path(), &db.db(), tools.path(), &["gain"]));
    let tokens_in = estimated_tokens(body.len());
    let tokens_out = estimated_tokens(trimmed.len());

    must_contain(&text, "Commands proxied: 2 (trimmed 2, preserved 0)");
    must_contain(
        &text,
        "Passthrough Commands: 1 (excluded from the Savings Rate)",
    );
    for header in ["COMMAND", "RUNS", "TOKENS IN", "TOKENS OUT", "SAVED"] {
        must_contain(&text, header);
    }
    assert_eq!(
        table_row(&text, "maven test"),
        vec![
            2,
            2 * tokens_in,
            2 * tokens_out,
            2 * (tokens_in - tokens_out)
        ],
        "view:\n{text}"
    );
    assert_eq!(
        table_row(&text, "passthrough"),
        vec![
            1,
            estimated_tokens(chatter.len()),
            estimated_tokens(chatter.len()),
            0
        ],
        "view:\n{text}"
    );
}

#[test]
fn here_counts_only_records_from_that_working_directory() {
    let db = TempDir::new("here-db");
    let tools = TempDir::new("here-tools");
    let project_a = TempDir::new("here-a");
    let project_b = TempDir::new("here-b");
    let body = maven_pass_fixture();
    let program = stand_in(tools.path(), "mvn", &body, 0);

    toltrim_in(
        project_a.path(),
        &db.db(),
        tools.path(),
        &[&program, "test"],
    );
    toltrim_in(
        project_b.path(),
        &db.db(),
        tools.path(),
        &[&program, "test"],
    );

    let everywhere = text_of(&toltrim_in(
        project_a.path(),
        &db.db(),
        tools.path(),
        &["gain"],
    ));
    must_contain(&everywhere, "Commands proxied: 2 (trimmed 2, preserved 0)");
    assert_eq!(table_row(&everywhere, "maven test")[0], 2);

    let here_a = text_of(&toltrim_in(
        project_a.path(),
        &db.db(),
        tools.path(),
        &["gain", "--here"],
    ));
    must_contain(&here_a, "Commands proxied: 1 (trimmed 1, preserved 0)");
    assert_eq!(table_row(&here_a, "maven test")[0], 1, "view:\n{here_a}");

    // The other project sees its own record, not this one's.
    let here_b = text_of(&toltrim_in(
        project_b.path(),
        &db.db(),
        tools.path(),
        &["gain", "--here"],
    ));
    must_contain(&here_b, "Commands proxied: 1 (trimmed 1, preserved 0)");

    // `--all` names the default scope explicitly.
    let explicit = text_of(&toltrim_in(
        project_a.path(),
        &db.db(),
        tools.path(),
        &["gain", "--all"],
    ));
    assert_eq!(explicit, everywhere);
}

#[test]
fn clear_removes_the_records_in_scope_and_the_view_is_then_empty() {
    let db = TempDir::new("clear-db");
    let tools = TempDir::new("clear-tools");
    let project_a = TempDir::new("clear-a");
    let project_b = TempDir::new("clear-b");
    let body = maven_pass_fixture();
    let program = stand_in(tools.path(), "mvn", &body, 0);

    toltrim_in(
        project_a.path(),
        &db.db(),
        tools.path(),
        &[&program, "test"],
    );
    toltrim_in(
        project_b.path(),
        &db.db(),
        tools.path(),
        &[&program, "test"],
    );

    // Only this project's records go.
    let cleared_here = toltrim_in(
        project_a.path(),
        &db.db(),
        tools.path(),
        &["gain", "--clear", "--here"],
    );
    assert_eq!(cleared_here.status.code(), Some(0));
    let confirmation = text_of(&cleared_here);
    assert_eq!(
        confirmation.lines().count(),
        1,
        "the confirmation is one line:\n{confirmation}"
    );
    must_contain(&confirmation, "Cleared 1 Gain Record for ");
    must_contain(
        &confirmation,
        &project_a
            .path()
            .file_name()
            .expect("the temp directory has a name")
            .to_string_lossy(),
    );
    // On Unix the process reports the symlink-resolved path (`/private/var` for
    // `/var`), so the whole directory is checked against the canonical one.
    #[cfg(unix)]
    must_contain(
        &confirmation,
        &fs::canonicalize(project_a.path())
            .expect("canonical project path")
            .to_string_lossy(),
    );

    let here_after = text_of(&toltrim_in(
        project_a.path(),
        &db.db(),
        tools.path(),
        &["gain", "--here"],
    ));
    must_contain(&here_after, "Commands proxied: 0 (trimmed 0, preserved 0)");
    must_contain(&here_after, "By command: no Gain Records yet.");

    let everywhere_after = text_of(&toltrim_in(
        project_a.path(),
        &db.db(),
        tools.path(),
        &["gain"],
    ));
    must_contain(
        &everywhere_after,
        "Commands proxied: 1 (trimmed 1, preserved 0)",
    );

    // Then everything goes.
    let cleared_all = toltrim_in(
        project_a.path(),
        &db.db(),
        tools.path(),
        &["gain", "--clear"],
    );
    assert_eq!(cleared_all.status.code(), Some(0));
    assert_eq!(text_of(&cleared_all).trim_end(), "Cleared 1 Gain Record.");

    let empty = text_of(&toltrim_in(
        project_a.path(),
        &db.db(),
        tools.path(),
        &["gain"],
    ));
    must_contain(&empty, "Commands proxied: 0 (trimmed 0, preserved 0)");
    must_contain(&empty, "Savings Rate: not yet measurable");
    must_contain(&empty, "By command: no Gain Records yet.");
}

#[test]
fn gain_records_persist_across_process_runs_until_cleared() {
    let db = TempDir::new("persist-db");
    let cwd = TempDir::new("persist-cwd");
    let tools = TempDir::new("persist-tools");
    let body = maven_pass_fixture();
    let program = stand_in(tools.path(), "mvn", &body, 0);

    toltrim_in(cwd.path(), &db.db(), tools.path(), &[&program, "test"]);
    assert!(db.db().exists(), "the Gain Record was never persisted");

    // Two further processes read the same user-level database.
    let first = text_of(&toltrim_in(cwd.path(), &db.db(), tools.path(), &["gain"]));
    let second = text_of(&toltrim_in(cwd.path(), &db.db(), tools.path(), &["gain"]));
    assert_eq!(first, second, "reading the view must not consume records");
    must_contain(&first, "Commands proxied: 1 (trimmed 1, preserved 0)");

    // A later Invocation accumulates rather than replacing.
    toltrim_in(cwd.path(), &db.db(), tools.path(), &[&program, "test"]);
    let third = text_of(&toltrim_in(cwd.path(), &db.db(), tools.path(), &["gain"]));
    must_contain(&third, "Commands proxied: 2 (trimmed 2, preserved 0)");
}

#[test]
fn a_trimmed_invocation_appends_one_gain_record_with_every_column() {
    let db = TempDir::new("record-db");
    let cwd = TempDir::new("record-cwd");
    let tools = TempDir::new("record-tools");
    let body = maven_pass_fixture();
    let program = stand_in(tools.path(), "mvn", &body, 7);

    let proxied = toltrim_in(cwd.path(), &db.db(), tools.path(), &[&program, "test"]);
    assert_eq!(
        proxied.status.code(),
        Some(7),
        "the exit code is forwarded verbatim"
    );
    let trimmed = text_of(&proxied);

    let rows = count_rows(&db.db());
    assert_eq!(rows, 1, "one Invocation must be exactly one Gain Record");

    let conn = rusqlite::Connection::open(db.db()).expect("the database opens");
    let (tool, subcommand, mode): (String, Option<String>, String) = conn
        .query_row(
            "SELECT tool, subcommand, mode FROM gain_record",
            rusqlite::params![],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("the Gain Record is readable");
    assert_eq!(tool, "maven");
    assert_eq!(subcommand.as_deref(), Some("test"));
    assert_eq!(mode, "trimmed");

    let (exit_code, bytes_in, bytes_out, tokens_in, tokens_out): (i64, i64, i64, i64, i64) = conn
        .query_row(
            "SELECT exit_code, bytes_in, bytes_out, tokens_in, tokens_out FROM gain_record",
            rusqlite::params![],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .expect("the Gain Record is readable");
    assert_eq!(exit_code, 7);
    assert_eq!(bytes_in, body.len() as i64);
    assert_eq!(bytes_out, trimmed.len() as i64);
    assert_eq!(tokens_in, estimated_tokens(body.len()));
    assert_eq!(tokens_out, estimated_tokens(trimmed.len()));

    let (rules_version, cwd_recorded, ts): (i64, String, i64) = conn
        .query_row(
            "SELECT rules_version, cwd, ts FROM gain_record",
            rusqlite::params![],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("the Gain Record is readable");
    assert_eq!(rules_version, RULES_VERSION);
    // The working directory the entry point saw: absolute, and the one the test
    // gave the Invocation. (A `gain --here` view over the same directory is the
    // behavioural proof that the two strings agree; see the filter test.)
    let recorded = PathBuf::from(&cwd_recorded);
    assert!(recorded.is_absolute(), "cwd recorded as {cwd_recorded}");
    assert_eq!(recorded.file_name(), cwd.path().file_name());
    #[cfg(unix)]
    assert_eq!(
        recorded,
        fs::canonicalize(cwd.path()).expect("canonical project path")
    );
    // Unix seconds, close to now.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the clock is after 1970")
        .as_secs() as i64;
    assert!(ts > 1_700_000_000 && ts <= now + 60, "ts was {ts}");
}

#[test]
fn a_database_that_cannot_be_written_leaves_output_and_exit_code_bit_for_bit_unaffected() {
    let body = maven_pass_fixture();
    let cwd = TempDir::new("safe-cwd");

    for exit_code in [0, 1] {
        // A writable, isolated database: the Proxied Command is fully accounted.
        let good = TempDir::new("safe-good");
        let good_tools = TempDir::new("safe-good-tools");
        let good_program = stand_in(good_tools.path(), "mvn", &body, exit_code);
        let good_run = toltrim_in(
            cwd.path(),
            &good.db(),
            good_tools.path(),
            &[&good_program, "test"],
        );

        // A regular file where the database's parent directory should be, so
        // neither the directory nor the database can be created.
        let broken = TempDir::new("safe-broken");
        let broken_tools = TempDir::new("safe-broken-tools");
        let broken_program = stand_in(broken_tools.path(), "mvn", &body, exit_code);
        fs::write(broken.path().join("data"), b"not a directory\n").expect("blocker is writable");
        let broken_run = toltrim_in(
            cwd.path(),
            &broken.db(),
            broken_tools.path(),
            &[&broken_program, "test"],
        );

        assert!(
            count_rows(&good.db()) >= 1,
            "the writable database recorded nothing, so the comparison is vacuous"
        );
        assert_eq!(
            broken_run.stdout, good_run.stdout,
            "stdout changed at exit code {exit_code}"
        );
        assert_eq!(
            broken_run.stderr, good_run.stderr,
            "stderr changed at exit code {exit_code}"
        );
        assert_eq!(
            broken_run.status.code(),
            Some(exit_code),
            "the exit code was not forwarded at exit code {exit_code}"
        );

        // No accounting line is ever injected into a Proxied Command's output.
        let emitted = text_of(&good_run);
        for absent in ["Gain Record", "Savings Rate", "Estimated Tokens"] {
            assert!(
                !emitted.contains(absent),
                "{absent:?} leaked into the Proxied Command's stdout:\n{emitted}"
            );
        }
    }
}

#[test]
fn a_view_over_an_unwritable_database_fails_loudly_instead_of_lying() {
    let db = TempDir::new("unwritable-db");
    let cwd = TempDir::new("unwritable-cwd");
    let tools = TempDir::new("unwritable-tools");
    fs::write(db.path().join("data"), b"not a directory\n").expect("blocker is writable");

    let view = toltrim_in(cwd.path(), &db.db(), tools.path(), &["gain"]);
    assert_ne!(view.status.code(), Some(0));
    assert!(text_of(&view).is_empty(), "stdout: {}", text_of(&view));
    assert_eq!(
        stderr_of(&view).lines().count(),
        1,
        "stderr: {}",
        stderr_of(&view)
    );
}

#[test]
fn an_unknown_option_is_one_explanatory_line_and_a_non_zero_exit() {
    let db = TempDir::new("option-db");
    let cwd = TempDir::new("option-cwd");
    let tools = TempDir::new("option-tools");

    let out = toltrim_in(cwd.path(), &db.db(), tools.path(), &["gain", "--verbose"]);
    assert_ne!(out.status.code(), Some(0));
    assert!(text_of(&out).is_empty(), "stdout: {}", text_of(&out));
    let stderr = stderr_of(&out);
    assert_eq!(stderr.lines().count(), 1, "stderr:\n{stderr}");
    must_contain(&stderr, "--verbose");

    // `--here` and `--all` are contradictory scopes, not silently combined.
    let clash = toltrim_in(
        cwd.path(),
        &db.db(),
        tools.path(),
        &["gain", "--here", "--all"],
    );
    assert_ne!(clash.status.code(), Some(0));
    assert_eq!(stderr_of(&clash).lines().count(), 1);
}

/// How many Gain Records the database holds, read directly.
fn count_rows(db: &Path) -> i64 {
    let conn = rusqlite::Connection::open(db).unwrap_or_else(|error| {
        panic!("cannot open {}: {error}", db.display());
    });
    conn.query_row(
        "SELECT COUNT(*) FROM gain_record",
        rusqlite::params![],
        |row| row.get(0),
    )
    .unwrap_or_else(|error| panic!("cannot count Gain Records in {}: {error}", db.display()))
}

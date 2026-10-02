//! Gain Records: one persisted row per Invocation, and the `toltrim gain` view.
//!
//! Ticket 03 owns this module. The interface below is the contract `main.rs`
//! relies on: [`record`] is called once per Invocation and must never be able to
//! change the proxied command's output or exit code, and [`run`] implements the
//! `toltrim gain` verb.
//!
//! Schema (ADR-0006):
//!
//! ```text
//! gain_record
//!   ts            INTEGER  -- unix seconds
//!   tool          TEXT     -- maven | git | listing | passthrough
//!   subcommand    TEXT     -- e.g. "test", "diff"
//!   mode          TEXT     -- trimmed | preserved | passthrough
//!   exit_code     INTEGER
//!   bytes_in      INTEGER
//!   bytes_out     INTEGER
//!   tokens_in     INTEGER  -- bytes_in / 4, rounded up
//!   tokens_out    INTEGER
//!   rules_version INTEGER
//!   cwd           TEXT
//! ```
//!
//! One user-level database holds every Invocation's Gain Record: `TOLTRIM_DB`
//! when set, else `~/.local/share/toltrim/history.db` on Unix and
//! `%LOCALAPPDATA%\toltrim\history.db` on Windows. Records are kept
//! indefinitely and removed only by `toltrim gain --clear`.

use std::ffi::OsString;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, params};

use crate::trim::RULES_VERSION;

/// The table every Gain Record is appended to. `IF NOT EXISTS` is the whole
/// migration story: the view only ever reads what `record` writes.
const SCHEMA: &str = "\
CREATE TABLE IF NOT EXISTS gain_record (
    ts            INTEGER NOT NULL,
    tool          TEXT    NOT NULL,
    subcommand    TEXT,
    mode          TEXT    NOT NULL,
    exit_code     INTEGER NOT NULL,
    bytes_in      INTEGER NOT NULL,
    bytes_out     INTEGER NOT NULL,
    tokens_in     INTEGER NOT NULL,
    tokens_out    INTEGER NOT NULL,
    rules_version INTEGER NOT NULL,
    cwd           TEXT    NOT NULL
);
CREATE INDEX IF NOT EXISTS gain_record_cwd ON gain_record (cwd);
";

/// One Invocation's accounting, as the entry point observed it. `record` derives
/// timestamp, Estimated Tokens, and rule version from these.
pub struct Record<'a> {
    pub tool: &'a str,
    pub subcommand: Option<&'a str>,
    pub mode: &'a str,
    pub exit_code: i32,
    pub bytes_in: usize,
    pub bytes_out: usize,
    pub cwd: &'a str,
}

/// Estimated Tokens: bytes divided by four, rounded up (ADR-0007). Never
/// presented as billed usage.
pub fn tokens(bytes: usize) -> i64 {
    bytes.div_ceil(4) as i64
}

/// Append one Gain Record. Best effort: a database that cannot be written must
/// leave the command's output and exit code bit-for-bit unaffected, so this
/// returns nothing, writes to neither stream, and never panics.
pub fn record(record: &Record<'_>) {
    let _ = quietly(|| {
        if let Some(path) = db_path()
            && let Ok(conn) = open(&path)
        {
            let _ = append(&conn, record);
        }
    });
}

/// The `toltrim gain` verb. Returns the process exit code.
///
/// `--here` scopes the view to the current working directory, `--clear` deletes
/// the records in scope and confirms in one line, and `--all` names the default
/// scope explicitly.
pub fn run(args: &[String]) -> i32 {
    let mut here = false;
    let mut clear = false;
    let mut all = false;
    for arg in args {
        match arg.as_str() {
            "--here" => here = true,
            "--clear" => clear = true,
            "--all" => all = true,
            unknown => {
                eprintln!(
                    "toltrim gain: unknown option `{unknown}`; usage: toltrim gain [--here] [--clear] [--all]"
                );
                return 2;
            }
        }
    }
    if here && all {
        eprintln!("toltrim gain: `--here` and `--all` cannot be combined; `--all` is the default");
        return 2;
    }

    let Some(path) = db_path() else {
        eprintln!("toltrim gain: no database path; set TOLTRIM_DB or HOME");
        return 1;
    };
    let conn = match open(&path) {
        Ok(conn) => conn,
        Err(error) => {
            eprintln!("toltrim gain: cannot open {}: {error}", path.display());
            return 1;
        }
    };
    let scope = if here {
        match std::env::current_dir() {
            Ok(cwd) => Some(cwd.to_string_lossy().into_owned()),
            Err(error) => {
                eprintln!("toltrim gain: cannot read the working directory: {error}");
                return 1;
            }
        }
    } else {
        None
    };

    if clear {
        let deleted = match conn.execute(
            "DELETE FROM gain_record WHERE (?1 IS NULL OR cwd = ?1)",
            params![scope],
        ) {
            Ok(deleted) => deleted,
            Err(error) => {
                eprintln!("toltrim gain: cannot clear Gain Records: {error}");
                return 1;
            }
        };
        let plural = if deleted == 1 { "" } else { "s" };
        match &scope {
            Some(cwd) => emit(&format!("Cleared {deleted} Gain Record{plural} for {cwd}.")),
            None => emit(&format!("Cleared {deleted} Gain Record{plural}.")),
        }
        return 0;
    }

    let summary = match summarize(&conn, scope.as_deref()) {
        Ok(summary) => summary,
        Err(error) => {
            eprintln!("toltrim gain: cannot read Gain Records: {error}");
            return 1;
        }
    };
    let rows = match command_rows(&conn, scope.as_deref()) {
        Ok(rows) => rows,
        Err(error) => {
            eprintln!("toltrim gain: cannot read Gain Records: {error}");
            return 1;
        }
    };
    emit(&view(&summary, &rows));
    0
}

/// Write one view to stdout, ignoring a closed pipe rather than panicking on it.
fn emit(text: &str) {
    use std::io::Write as _;
    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(text.as_bytes());
    let _ = stdout.write_all(b"\n");
    let _ = stdout.flush();
}

/// Run `f` with the panic hook silenced and any panic caught, so an accounting
/// write can contribute nothing at all — not an exit code, not a byte on either
/// stream. `record` runs after the proxied command's streams have been
/// collected and the binary is single-threaded by then; the lock only keeps two
/// library callers from swapping the hook out from under each other.
fn quietly<T>(f: impl FnOnce() -> T) -> Option<T> {
    static HOOK: Mutex<()> = Mutex::new(());
    let _guard = HOOK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    std::panic::set_hook(previous);
    outcome.ok()
}

/// Where Gain Records live: `TOLTRIM_DB` when set and non-empty, else the one
/// user-level database of ADR-0006.
fn db_path() -> Option<PathBuf> {
    override_db_path(std::env::var_os("TOLTRIM_DB")).or_else(default_db_path)
}

/// `TOLTRIM_DB` is an override only when it names something: an empty value is
/// the same as unset, which is what an exported-but-empty variable means.
fn override_db_path(value: Option<OsString>) -> Option<PathBuf> {
    value.filter(|value| !value.is_empty()).map(PathBuf::from)
}

/// ADR-0006: `$XDG_DATA_HOME/toltrim/history.db` when the user set one, else
/// `~/.local/share/toltrim/history.db` — the same directory under its default.
#[cfg(unix)]
fn default_db_path() -> Option<PathBuf> {
    unix_db_path(std::env::var_os("XDG_DATA_HOME"), std::env::var_os("HOME"))
}

#[cfg(unix)]
fn unix_db_path(xdg_data_home: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    let base = xdg_data_home
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            home.filter(|value| !value.is_empty())
                .map(|home| PathBuf::from(home).join(".local").join("share"))
        })?;
    Some(base.join("toltrim").join("history.db"))
}

/// ADR-0006: `%LOCALAPPDATA%\toltrim\history.db`.
#[cfg(windows)]
fn default_db_path() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA").filter(|value| !value.is_empty())?;
    Some(PathBuf::from(base).join("toltrim").join("history.db"))
}

#[cfg(not(any(unix, windows)))]
fn default_db_path() -> Option<PathBuf> {
    None
}

/// Open the database, creating its parent directory and its table when missing.
fn open(path: &Path) -> rusqlite::Result<Connection> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        let _ = std::fs::create_dir_all(parent);
    }
    let conn = Connection::open(path)?;
    // Concurrent Invocations may write at the same time; wait briefly rather
    // than failing the record. ADR-0006 claims no latency budget, but a hang
    // would still be a risk, so the wait is bounded.
    conn.busy_timeout(Duration::from_secs(1))?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

/// Append one row. Only [`record`] calls this, and it is the no-fail face.
fn append(conn: &Connection, record: &Record<'_>) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO gain_record
             (ts, tool, subcommand, mode, exit_code, bytes_in, bytes_out,
              tokens_in, tokens_out, rules_version, cwd)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            unix_seconds(),
            record.tool,
            record.subcommand,
            record.mode,
            record.exit_code,
            record.bytes_in as i64,
            record.bytes_out as i64,
            tokens(record.bytes_in),
            tokens(record.bytes_out),
            RULES_VERSION,
            record.cwd,
        ],
    )?;
    Ok(())
}

fn unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

/// The summary block's aggregates.
struct Summary {
    trimmed: i64,
    preserved: i64,
    passthrough: i64,
    /// Estimated Tokens over Proxied Commands only, so the Savings Rate can be
    /// re-derived from the numbers beside it.
    tokens_in: i64,
    tokens_out: i64,
    /// Passthrough Commands are still shown, separately: they save nothing.
    passthrough_tokens_in: i64,
}

/// One row of the per-command table: a command identity and its aggregates.
struct Row {
    tool: String,
    subcommand: Option<String>,
    runs: i64,
    tokens_in: i64,
    tokens_out: i64,
}

/// `cwd` filters the view; `None` means every record. The `?1 IS NULL` form
/// keeps one statement for both scopes.
fn summarize(conn: &Connection, cwd: Option<&str>) -> rusqlite::Result<Summary> {
    conn.query_row(
        "SELECT
             COALESCE(SUM(mode = 'trimmed'), 0),
             COALESCE(SUM(mode = 'preserved'), 0),
             COALESCE(SUM(mode = 'passthrough'), 0),
             COALESCE(SUM(CASE WHEN mode = 'passthrough' THEN 0 ELSE tokens_in END), 0),
             COALESCE(SUM(CASE WHEN mode = 'passthrough' THEN 0 ELSE tokens_out END), 0),
             COALESCE(SUM(CASE WHEN mode = 'passthrough' THEN tokens_in ELSE 0 END), 0)
         FROM gain_record
         WHERE (?1 IS NULL OR cwd = ?1)",
        params![cwd],
        |row| {
            Ok(Summary {
                trimmed: row.get(0)?,
                preserved: row.get(1)?,
                passthrough: row.get(2)?,
                tokens_in: row.get(3)?,
                tokens_out: row.get(4)?,
                passthrough_tokens_in: row.get(5)?,
            })
        },
    )
}

fn command_rows(conn: &Connection, cwd: Option<&str>) -> rusqlite::Result<Vec<Row>> {
    let mut statement = conn.prepare(
        "SELECT tool, subcommand, COUNT(*),
                COALESCE(SUM(tokens_in), 0), COALESCE(SUM(tokens_out), 0)
         FROM gain_record
         WHERE (?1 IS NULL OR cwd = ?1)
         GROUP BY tool, subcommand
         ORDER BY (COALESCE(SUM(tokens_in), 0) - COALESCE(SUM(tokens_out), 0)) DESC,
                  COUNT(*) DESC, tool ASC, subcommand ASC",
    )?;
    let rows = statement.query_map(params![cwd], |row| {
        Ok(Row {
            tool: row.get(0)?,
            subcommand: row.get(1)?,
            runs: row.get(2)?,
            tokens_in: row.get(3)?,
            tokens_out: row.get(4)?,
        })
    })?;
    rows.collect()
}

/// The whole `toltrim gain` view: summary first, then the per-command table.
/// Pure over its inputs, so the numbers an agent reads can be unit-tested.
fn view(summary: &Summary, rows: &[Row]) -> String {
    let proxied = summary.trimmed + summary.preserved;
    let saved = summary.tokens_in - summary.tokens_out;

    let mut out = String::new();
    let _ = writeln!(
        out,
        "Commands proxied: {proxied} (trimmed {}, preserved {})",
        summary.trimmed, summary.preserved
    );
    let _ = writeln!(
        out,
        "Passthrough Commands: {} (excluded from the Savings Rate)",
        summary.passthrough
    );
    let _ = writeln!(out, "Estimated Tokens in: {}", summary.tokens_in);
    let _ = writeln!(out, "Estimated Tokens out: {}", summary.tokens_out);
    let _ = writeln!(out, "Estimated Tokens saved: {saved}");
    let _ = writeln!(
        out,
        "{}",
        savings_rate(proxied, summary.tokens_in, summary.tokens_out)
    );
    let _ = writeln!(
        out,
        "Passthrough Estimated Tokens in: {} (unchanged, so they save nothing)",
        summary.passthrough_tokens_in
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Estimated Tokens are an estimate, not billed usage: output bytes / 4, rounded up."
    );
    let _ = writeln!(
        out,
        "The Savings Rate is the figure to trust; the absolute Estimated Tokens above are approximate."
    );
    let _ = writeln!(out);
    if rows.is_empty() {
        let _ = write!(out, "By command: no Gain Records yet.");
    } else {
        let _ = writeln!(out, "By command:");
        let _ = write!(out, "{}", table(rows));
    }
    out
}

/// Estimated Tokens removed as a share of the original, over Proxied Commands
/// only. With none recorded there is nothing to divide by, so the view says so
/// instead of printing `NaN` or a division by zero.
fn savings_rate(proxied: i64, tokens_in: i64, tokens_out: i64) -> String {
    if proxied == 0 {
        return "Savings Rate: not yet measurable (no Proxied Commands recorded)".to_string();
    }
    if tokens_in <= 0 {
        return "Savings Rate: not yet measurable (no Estimated Tokens in for any Proxied Command)"
            .to_string();
    }
    let rate = (tokens_in - tokens_out) as f64 / tokens_in as f64 * 100.0;
    format!("Savings Rate: {rate:.1}% of the original, over Proxied Commands only")
}

const HEADERS: [&str; 5] = ["COMMAND", "RUNS", "TOKENS IN", "TOKENS OUT", "SAVED"];

/// A plain fixed-width table, so the view needs no formatting crate.
fn table(rows: &[Row]) -> String {
    let cells: Vec<[String; 5]> = rows
        .iter()
        .map(|row| {
            [
                command_label(row),
                row.runs.to_string(),
                row.tokens_in.to_string(),
                row.tokens_out.to_string(),
                (row.tokens_in - row.tokens_out).to_string(),
            ]
        })
        .collect();

    let mut widths = HEADERS.map(|header| header.len());
    for row in &cells {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.len());
        }
    }

    let mut lines = Vec::with_capacity(cells.len() + 1);
    lines.push(format_row(&HEADERS.map(String::from), &widths));
    for row in &cells {
        lines.push(format_row(row, &widths));
    }
    lines.join("\n")
}

fn format_row(cells: &[String; 5], widths: &[usize; 5]) -> String {
    let mut line = String::new();
    for (index, cell) in cells.iter().enumerate() {
        if index > 0 {
            line.push_str("  ");
        }
        if index == 0 {
            let _ = write!(line, "{cell:<width$}", width = widths[index]);
        } else {
            let _ = write!(line, "{cell:>width$}", width = widths[index]);
        }
    }
    line
}

/// How one command is named in the table: `maven test`, `git diff`, or just the
/// tool when the Invocation named no subcommand (`passthrough`).
fn command_label(row: &Row) -> String {
    match row.subcommand.as_deref().filter(|sub| !sub.is_empty()) {
        Some(subcommand) => format!("{} {subcommand}", row.tool),
        None => row.tool.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(
        trimmed: i64,
        preserved: i64,
        passthrough: i64,
        tokens_in: i64,
        tokens_out: i64,
        passthrough_tokens_in: i64,
    ) -> Summary {
        Summary {
            trimmed,
            preserved,
            passthrough,
            tokens_in,
            tokens_out,
            passthrough_tokens_in,
        }
    }

    fn row(
        tool: &str,
        subcommand: Option<&str>,
        runs: i64,
        tokens_in: i64,
        tokens_out: i64,
    ) -> Row {
        Row {
            tool: tool.to_string(),
            subcommand: subcommand.map(str::to_string),
            runs,
            tokens_in,
            tokens_out,
        }
    }

    #[test]
    fn estimated_tokens_are_bytes_over_four_rounded_up() {
        assert_eq!(tokens(0), 0);
        assert_eq!(tokens(1), 1);
        assert_eq!(tokens(4), 1);
        assert_eq!(tokens(5), 2);
        assert_eq!(tokens(8), 2);
        assert_eq!(tokens(9), 3);
    }

    #[test]
    fn an_empty_toltrim_db_is_not_an_override() {
        assert_eq!(override_db_path(None), None);
        assert_eq!(override_db_path(Some(OsString::new())), None);
        assert_eq!(
            override_db_path(Some(OsString::from("/tmp/isolated.db"))),
            Some(PathBuf::from("/tmp/isolated.db"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_default_database_is_the_one_user_level_database_of_adr_0006() {
        assert_eq!(
            unix_db_path(None, Some(OsString::from("/home/dev"))),
            Some(PathBuf::from("/home/dev/.local/share/toltrim/history.db"))
        );
        // XDG_DATA_HOME names the same directory, and wins when it is set.
        assert_eq!(
            unix_db_path(
                Some(OsString::from("/data")),
                Some(OsString::from("/home/dev"))
            ),
            Some(PathBuf::from("/data/toltrim/history.db"))
        );
        assert_eq!(unix_db_path(None, None), None);
        assert_eq!(
            unix_db_path(Some(OsString::new()), Some(OsString::from("/home/dev"))),
            Some(PathBuf::from("/home/dev/.local/share/toltrim/history.db"))
        );
    }

    #[test]
    fn the_savings_rate_is_one_decimal_and_counts_proxied_commands_only() {
        // 1000 in, 127 out: 873 removed, 87.3%. The passthrough command's
        // tokens are shown but never enter the rate.
        let view = view(&summary(1, 0, 1, 1000, 127, 400), &[]);
        assert!(
            view.contains("Savings Rate: 87.3% of the original, over Proxied Commands only"),
            "got:\n{view}"
        );
        assert!(view.contains("Estimated Tokens in: 1000"), "got:\n{view}");
        assert!(view.contains("Passthrough Commands: 1 (excluded from the Savings Rate)"));
        assert!(view.contains("Passthrough Estimated Tokens in: 400"));
    }

    #[test]
    fn a_preserved_invocation_counts_as_a_proxied_command() {
        let view = view(&summary(0, 2, 0, 200, 100, 0), &[]);
        assert!(
            view.contains("Commands proxied: 2 (trimmed 0, preserved 2)"),
            "got:\n{view}"
        );
        assert!(view.contains("Savings Rate: 50.0%"), "got:\n{view}");
    }

    #[test]
    fn an_empty_view_says_the_rate_is_not_yet_measurable() {
        let view = view(&summary(0, 0, 0, 0, 0, 0), &[]);
        assert!(
            view.contains("Commands proxied: 0 (trimmed 0, preserved 0)"),
            "got:\n{view}"
        );
        assert!(
            view.contains("Savings Rate: not yet measurable (no Proxied Commands recorded)"),
            "got:\n{view}"
        );
        assert!(!view.contains("NaN"), "got:\n{view}");
        assert!(
            view.contains("By command: no Gain Records yet."),
            "got:\n{view}"
        );
    }

    #[test]
    fn every_estimate_is_labelled_as_an_estimate_and_states_the_method() {
        let view = view(&summary(1, 0, 0, 100, 50, 0), &[]);
        assert!(view.contains("not billed usage"), "got:\n{view}");
        assert!(
            view.contains("output bytes / 4, rounded up"),
            "got:\n{view}"
        );
        assert!(
            view.contains("The Savings Rate is the figure to trust"),
            "got:\n{view}"
        );
        assert!(
            !view.to_lowercase().contains("billed usage: 50"),
            "got:\n{view}"
        );
    }

    #[test]
    fn the_table_reports_runs_tokens_and_savings_per_command() {
        let rows = vec![
            row("maven", Some("test"), 2, 1000, 127),
            row("passthrough", None, 1, 400, 400),
        ];
        let rendered = table(&rows);
        let lines: Vec<&str> = rendered.lines().collect();
        assert_eq!(lines[0], "COMMAND      RUNS  TOKENS IN  TOKENS OUT  SAVED");
        assert_eq!(lines[1], "maven test      2       1000         127    873");
        assert_eq!(lines[2], "passthrough     1        400         400      0");
        assert_eq!(lines.len(), 3);
    }
}

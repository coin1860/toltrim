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

    // Colour is a property of the destination, not of the data: a terminal gets
    // the Savings figure in green, a pipe gets plain text. The view itself is
    // built from numbers only.
    use std::io::IsTerminal as _;
    let color = std::io::stdout().is_terminal();
    emit(&view(scope.is_some(), &summary, &rows, color));
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

/// Green when the view goes to a terminal, the identity map when it does not.
fn paint(color: bool, text: &str) -> String {
    if color {
        format!("\u{1b}[32m{text}\u{1b}[0m")
    } else {
        text.to_string()
    }
}

/// `76279` reads as `76.3K`; a million and up reads as `M`. One decimal place,
/// dropped when it is zero, so `80000` is `80K` and not `80.0K`. Pure display:
/// the Gain Records keep their exact integers.
fn human(bytes_tokens: i64) -> String {
    let (value, suffix) = if bytes_tokens >= 1_000_000 {
        (bytes_tokens as f64 / 1_000_000.0, "M")
    } else if bytes_tokens >= 1_000 {
        (bytes_tokens as f64 / 1_000.0, "K")
    } else {
        return bytes_tokens.to_string();
    };
    let rounded = (value * 10.0).round() / 10.0;
    let text = if rounded.fract() < f64::EPSILON {
        format!("{rounded:.0}")
    } else {
        format!("{rounded:.1}")
    };
    format!("{text}{suffix}")
}

/// Twenty cells, filled in proportion to the Savings Rate: 82.4% is sixteen
/// filled cells and four empty ones.
fn meter(rate_pct: f64) -> String {
    const CELLS: usize = 20;
    let filled = ((rate_pct / 100.0 * CELLS as f64).round() as usize).min(CELLS);
    let mut bar = String::with_capacity(CELLS);
    for cell in 0..CELLS {
        bar.push(if cell < filled { '█' } else { '░' });
    }
    bar
}

/// The whole `toltrim gain` view. `here` names the scope in the title; `color`
/// decides whether the Savings figure is painted.
fn view(here: bool, summary: &Summary, rows: &[CommandRow], color: bool) -> String {
    let proxied = summary.trimmed + summary.preserved;
    let saved = summary.tokens_in - summary.tokens_out;
    let scope_name = if here { "Here Scope" } else { "Global Scope" };

    // The Savings Rate is a share of the original, over Proxied Commands only;
    // with nothing to divide by it is zero rather than NaN.
    let rate = if summary.tokens_in > 0 {
        (saved as f64 / summary.tokens_in as f64) * 100.0
    } else {
        0.0
    };

    let mut out = String::new();
    let _ = writeln!(out, "Toltrim Token Savings ({scope_name})");
    let _ = writeln!(
        out,
        "════════════════════════════════════════════════════════════"
    );
    let _ = writeln!(
        out,
        "{:<18}{} (trimmed {}, preserved {})",
        "Commands proxied:",
        human(proxied),
        summary.trimmed,
        summary.preserved
    );
    let _ = writeln!(
        out,
        "{:<18}{} (excluded from the Savings Rate)",
        "Passthrough:",
        human(summary.passthrough)
    );
    let _ = writeln!(out, "{:<18}{}", "Input tokens:", human(summary.tokens_in));
    let _ = writeln!(out, "{:<18}{}", "Output tokens:", human(summary.tokens_out));
    let _ = writeln!(
        out,
        "{:<18}{} ({rate:.1}%)",
        "Tokens saved:",
        paint(color, &human(saved))
    );
    let _ = writeln!(
        out,
        "{:<18}{} {rate:.1}%",
        "Savings meter:",
        paint(color, &meter(rate))
    );
    if proxied == 0 {
        let _ = writeln!(
            out,
            "Savings Rate: not yet measurable (no Proxied Commands recorded)"
        );
    } else {
        let _ = writeln!(
            out,
            "Savings Rate: {rate:.1}% of the original, over Proxied Commands only"
        );
    }
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
        let _ = writeln!(
            out,
            "No Gain Records in scope yet; run a Proxied Command first."
        );
        return out;
    }

    let _ = writeln!(out, "By Command");
    let _ = writeln!(out, "{}", "─".repeat(72));
    // The header names five columns; the sixth (the Impact bar) is self-evident
    // from the rows, so it carries no label of its own.
    let _ = writeln!(
        out,
        "{:<4}{:<18}{:>6}{:>9}{:>5}  ",
        "#", "Command", "Count", "Saved", "Avg%"
    );

    // Saved, largest first; the Impact bar is a share of the top row's Saved, so
    // the biggest spender reads as a full bar and the rest scale down.
    let mut ranked: Vec<&CommandRow> = rows.iter().collect();
    ranked.sort_by(|a, b| {
        (b.saved)
            .partial_cmp(&a.saved)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.label.cmp(&b.label))
    });
    let top_saved = ranked.first().map(|row| row.saved).unwrap_or(0.0);

    for (index, row) in ranked.iter().take(10).enumerate() {
        let index = index + 1; // ranks are one-based
        let impact = if top_saved > 0.0 {
            meter(row.saved / top_saved * 100.0)
        } else {
            meter(0.0)
        };
        let _ = writeln!(
            out,
            "{:<4}{:<18}{:>6}{:>9}{:>7.1}%  {}",
            format!("{index}."),
            row.label,
            human(row.count),
            paint(color, &human(row.saved as i64)),
            row.avg_pct,
            impact
        );
    }
    out
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
}

/// One row of the per-command table: a command identity and its aggregates.
/// `saved` and `avg_pct` are derived at read time, so the view can rank and
/// scale without re-querying.
struct CommandRow {
    label: String,
    count: i64,
    saved: f64,
    avg_pct: f64,
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
             COALESCE(SUM(CASE WHEN mode = 'passthrough' THEN 0 ELSE tokens_out END), 0)
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
            })
        },
    )
}

fn command_rows(conn: &Connection, cwd: Option<&str>) -> rusqlite::Result<Vec<CommandRow>> {
    let mut statement = conn.prepare(
        "SELECT tool, subcommand, COUNT(*),
                COALESCE(SUM(tokens_in), 0), COALESCE(SUM(tokens_out), 0)
         FROM gain_record
         WHERE (?1 IS NULL OR cwd = ?1)
         GROUP BY tool, subcommand",
    )?;
    let rows = statement.query_map(params![cwd], |row| {
        let tool: String = row.get(0)?;
        let subcommand: Option<String> = row.get(1)?;
        let count: i64 = row.get(2)?;
        let tokens_in: i64 = row.get(3)?;
        let tokens_out: i64 = row.get(4)?;
        // A Passthrough Command's bytes are excluded from the Savings Rate, so
        // its row saves nothing by definition — mirroring `summarize`.
        let passthrough_row = tool == "passthrough";
        let saved = if passthrough_row {
            0.0
        } else {
            (tokens_in - tokens_out) as f64
        };
        let avg_pct = if passthrough_row || tokens_in == 0 {
            0.0
        } else {
            saved / tokens_in as f64 * 100.0
        };
        Ok(CommandRow {
            label: match subcommand.as_deref().filter(|sub| !sub.is_empty()) {
                Some(subcommand) => format!("{tool} {subcommand}"),
                None => tool,
            },
            count,
            saved,
            avg_pct,
        })
    })?;
    rows.collect()
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
    ) -> Summary {
        Summary {
            trimmed,
            preserved,
            passthrough,
            tokens_in,
            tokens_out,
        }
    }

    fn command_row(label: &str, count: i64, saved: f64, avg_pct: f64) -> CommandRow {
        CommandRow {
            label: label.to_string(),
            count,
            saved,
            avg_pct,
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
        // An empty XDG_DATA_HOME is the same as unset.
        assert_eq!(
            unix_db_path(Some(OsString::new()), Some(OsString::from("/home/dev"))),
            Some(PathBuf::from("/home/dev/.local/share/toltrim/history.db"))
        );
    }

    #[test]
    fn the_view_keeps_the_rate_over_proxied_commands_only() {
        // 1000 in, 127 out: 873 removed, 87.3%. The passthrough command's
        // tokens never enter the rate.
        let view = view(false, &summary(1, 0, 1, 1000, 127), &[], false);
        assert!(
            view.contains("Savings Rate: 87.3% of the original, over Proxied Commands only"),
            "got:\n{view}"
        );
        // The ADR-0007 disclaimer survives the restyle, and no billed figure is
        // published (the disclaimer's own colon is the only "billed usage:" in view).
        assert!(view.contains("not billed usage"), "got:\n{view}");
    }

    #[test]
    fn an_empty_view_reports_not_yet_measurable_and_an_empty_meter() {
        let view = view(false, &summary(0, 0, 1, 0, 0), &[], false);
        assert!(
            view.contains("Savings Rate: not yet measurable (no Proxied Commands recorded)"),
            "got:\n{view}"
        );
        assert!(!view.contains("NaN"), "got:\n{view}");
        assert!(view.contains("░░░░░░░░░░░░░░░░░░░░ 0.0%"), "got:\n{view}");
        assert!(
            view.contains("No Gain Records in scope yet; run a Proxied Command first."),
            "got:\n{view}"
        );
    }

    #[test]
    fn every_estimate_is_labelled_as_an_estimate_and_states_the_method() {
        let view = view(false, &summary(1, 0, 0, 100, 50), &[], false);
        assert!(view.contains("not billed usage"), "got:\n{view}");
        assert!(
            view.contains("output bytes / 4, rounded up"),
            "got:\n{view}"
        );
        assert!(
            view.contains("The Savings Rate is the figure to trust"),
            "got:\n{view}"
        );
    }

    #[test]
    fn the_by_command_table_ranks_by_saved_and_scales_the_impact_bar() {
        let rows = vec![
            command_row("maven test", 2, 873.0, 87.3),
            command_row("git log", 1, 1577.0, 74.0),
            command_row("listing", 2, 9.0, 2.7),
        ];
        let view = view(false, &summary(5, 0, 0, 10000, 8000), &rows, false);
        // Saved, largest first: git log (1577) before maven test (873).
        let git = view.find("git log").expect("git log row");
        let maven = view.find("maven test").expect("maven test row");
        assert!(git < maven, "got:\n{view}");
        // The top row's Impact bar is full; the smallest is a sliver.
        assert!(view.contains("██████████"), "got:\n{view}");
        // 9 against a top of 1577 is less than half a cell, so it rounds to empty.
        let listing_line = view.lines().find(|line| line.contains("listing")).unwrap();
        assert!(
            listing_line.contains("░░░░░░░░░░░░░░░░░░░░"),
            "got: {listing_line}"
        );
        // K/M abbreviations in the Saved column.
        assert!(view.contains("1.6K"), "got:\n{view}");
    }

    #[test]
    fn the_view_is_plain_text_without_a_terminal_and_green_with_one() {
        let rows = vec![command_row("maven test", 2, 873.0, 87.3)];
        let plain = view(false, &summary(2, 2, 0, 1000, 127), &rows, false);
        assert!(
            !plain.contains("\u{1b}"),
            "plain view must carry no escapes:\n{plain}"
        );
        let colored = view(false, &summary(2, 2, 0, 1000, 127), &rows, true);
        assert!(
            colored.contains("\u{1b}[32m"),
            "colored view must paint green:\n{colored}"
        );
    }

    #[test]
    fn human_abbreviates_thousands_and_millions() {
        assert_eq!(human(999), "999");
        assert_eq!(human(1000), "1K");
        assert_eq!(human(76279), "76.3K");
        assert_eq!(human(80000), "80K");
        assert_eq!(human(1_500_000), "1.5M");
    }

    #[test]
    fn the_meter_fills_in_proportion_to_the_rate() {
        assert_eq!(meter(0.0), "░".repeat(20));
        assert_eq!(meter(100.0), "█".repeat(20));
        let half = meter(50.0);
        assert_eq!(half, format!("{}{}", "█".repeat(10), "░".repeat(10)));
    }
}

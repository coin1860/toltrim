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
/// returns nothing and never panics.
pub fn record(_record: &Record<'_>) {
    // Implemented by ticket 03.
}

/// The `toltrim gain` verb. Returns the process exit code.
pub fn run(_args: &[String]) -> i32 {
    eprintln!("toltrim gain: not implemented yet");
    1
}

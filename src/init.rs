//! `toltrim init`: write the command mapping into the project's `AGENTS.md`.
//!
//! Ticket 02 owns this module. The interface below is the contract `main.rs`
//! relies on: [`run`] implements the `toltrim init` verb and returns the process
//! exit code. Nothing outside the project may be written.

/// The `toltrim init` verb. `args` is everything after `init`. Returns the
/// process exit code.
pub fn run(_args: &[String]) -> i32 {
    eprintln!("toltrim init: not implemented yet");
    1
}

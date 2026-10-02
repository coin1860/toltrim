//! toltrim: a command proxy that trims tool output for coding agents and counts
//! the tokens saved.
//!
//! The crate is split so that every trimming rule is a pure function over text:
//! `trim` never spawns a process, and `exec` never looks at output. `trim::render`
//! is the single seam every rule is reached through.

pub mod ansi;
pub mod exec;
pub mod gain;
pub mod init;
pub mod trim;

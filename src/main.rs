//! toltrim entry point: decide whether this Invocation may be trimmed, run the
//! command, emit the result, and forward the exit code untouched.

use std::ffi::OsString;
use std::io::{IsTerminal, Write};

use toltrim::trim::{self, Tool};

fn main() {
    let mut raw = false;
    let mut argv: Vec<OsString> = std::env::args_os().skip(1).collect();
    // `--raw` belongs to toltrim, not to the command it proxies.
    argv.retain(|arg| {
        if arg == "--raw" {
            raw = true;
            false
        } else {
            true
        }
    });

    let Some(first) = argv.first().map(|a| a.to_string_lossy().into_owned()) else {
        usage();
        std::process::exit(2);
    };
    match first.as_str() {
        "-h" | "--help" => {
            usage();
            std::process::exit(0);
        }
        "-V" | "--version" => {
            println!("toltrim {}", env!("CARGO_PKG_VERSION"));
            std::process::exit(0);
        }
        _ => {}
    }

    let program = argv[0].clone();
    let rest = &argv[1..];
    let tool = trim::detect(&first);
    let flags: Vec<String> = rest
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();

    let preserve = raw
        || trim::requests_machine_readable(tool, &flags)
        || trim::is_human_pipe(
            std::io::stdout().is_terminal(),
            std::io::stdin().is_terminal(),
        );

    let outcome = toltrim::exec::run(&program, rest);
    if let Some(error) = outcome.spawn_error {
        eprintln!("toltrim: {error}");
        std::process::exit(127);
    }

    let rendered = if preserve {
        match String::from_utf8(outcome.stdout) {
            Ok(text) => toltrim::ansi::strip(&text).into_bytes(),
            Err(e) => e.into_bytes(),
        }
    } else if tool == Tool::Maven {
        match String::from_utf8(outcome.stdout) {
            Ok(text) => trim::maven::trim(&text, outcome.code).into_bytes(),
            Err(e) => e.into_bytes(),
        }
    } else {
        // Anything we do not know how to trim is passed through byte for byte:
        // an unproxied command must behave exactly as it would alone.
        outcome.stdout
    };

    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(&rendered);
    let _ = stdout.flush();
    drop(stdout);

    let mut stderr = std::io::stderr().lock();
    let _ = stderr.write_all(&outcome.stderr);
    let _ = stderr.flush();
    drop(stderr);

    // Verbatim: `git diff` exits 1 when there are differences, and rewriting
    // that would turn "there are changes" into "the command failed".
    std::process::exit(outcome.code);
}

fn usage() {
    eprintln!(
        "toltrim {version} — trim command output for coding agents

USAGE:
    toltrim <command> [args...]

Proxied commands (output is trimmed):
    mvn test|verify|package|compile|install|clean|test-compile

Everything else is executed unchanged, byte for byte.

OPTIONS:
    --raw        run the command and emit its output untrimmed
    -h, --help   show this message
    -V, --version
",
        version = env!("CARGO_PKG_VERSION")
    );
}

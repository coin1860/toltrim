//! toltrim entry point. It keeps only what needs a process — parsing arguments,
//! starting the command, writing the streams, forwarding the exit code — and
//! reaches every trimming rule through `trim::render`.

use std::ffi::OsString;
use std::io::{IsTerminal, Write};

use toltrim::gain::Record;
use toltrim::trim;

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
        // toltrim's own verbs, which proxy nothing.
        "init" => std::process::exit(toltrim::init::run(&strings(&argv[1..]))),
        "gain" => std::process::exit(toltrim::gain::run(&strings(&argv[1..]))),
        _ => {}
    }

    let program = argv[0].clone();
    let rest = &argv[1..];
    let flags = strings(rest);
    let tool = trim::detect(&first);

    // The preservation decision is made here, where the streams are, but it is a
    // pure function: `--raw`, a Machine-Readable Request, and the human-pipe case
    // are all testable without a process.
    let preserve = trim::should_preserve(
        raw,
        tool,
        &flags,
        std::io::stdout().is_terminal(),
        std::io::stdin().is_terminal(),
    );

    let outcome = toltrim::exec::run(&program, rest);
    if let Some(error) = outcome.spawn_error {
        eprintln!("toltrim: {error}");
        std::process::exit(127);
    }

    let rendered = trim::render(tool, &flags, &outcome.stdout, outcome.code, preserve);

    // Accounting is best effort and always after the result is known; it may
    // never alter what the command emitted or how it exited.
    let subcommand = trim::subcommand(tool, &flags);
    let cwd = std::env::current_dir().unwrap_or_default();
    toltrim::gain::record(&Record {
        tool: tool.as_str(),
        subcommand: subcommand.as_deref(),
        mode: trim::classify(tool, &flags, preserve).as_str(),
        exit_code: outcome.code,
        bytes_in: outcome.stdout.len(),
        bytes_out: rendered.len(),
        cwd: &cwd.to_string_lossy(),
    });

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

fn strings(args: &[OsString]) -> Vec<String> {
    args.iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect()
}

fn usage() {
    eprintln!(
        "toltrim {version} — trim command output for coding agents

USAGE:
    toltrim <command> [args...]
    toltrim init [--dry-run]
    toltrim gain [--here|--all] [--clear]

Proxied commands (output is trimmed):
    mvn test|verify|package|compile|install|clean|test-compile|integration-test
    git status|diff|log|show|branch
    ls, dir, tree

Everything else is executed unchanged, byte for byte.

OPTIONS:
    --raw        run the command and emit its output untrimmed
    -h, --help   show this message
    -V, --version
",
        version = env!("CARGO_PKG_VERSION")
    );
}

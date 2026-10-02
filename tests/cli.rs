//! The published interface: `--help`, `--version`, the usage error, a
//! fresh-install check, and a drift guard between the Proxied Command list the
//! README advertises and the list the code really implements.
//!
//! The help and version tests run the binary cargo built for this test run
//! (`CARGO_BIN_EXE_toltrim`). It is the same code the release profile builds;
//! only the optimisation level differs, so what these tests observe about
//! startup — no database, no JDK, no Rust toolchain on `PATH` — is what a fresh
//! install observes.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use toltrim::trim::{self, Tool};

/// The binary under test.
const BIN: &str = env!("CARGO_BIN_EXE_toltrim");

/// The checkout these tests read `README.md`, `src/main.rs` and `src/init.rs`
/// from.
const MANIFEST_ROOT: &str = env!("CARGO_MANIFEST_DIR");

/// The command families `--help` must name. This is written out rather than
/// derived so that removing one from the usage text fails here.
const FAMILIES: &[&str] = &[
    "mvn test",
    "mvn verify",
    "mvn package",
    "mvn compile",
    "mvn install",
    "mvn clean",
    "mvn test-compile",
    "mvn integration-test",
    "git status",
    "git diff",
    "git log",
    "git show",
    "git branch",
    "ls",
    "dir",
    "tree",
];

/// State-changing git subcommands: always Passthrough Commands, never bounded by
/// a trimming rule (ADR-0008).
const ALWAYS_PASSTHROUGH: &[&str] = &["add", "commit", "checkout", "pull", "push", "merge", "rebase"];

fn run(args: &[&str]) -> Output {
    let mut cmd = Command::new(BIN);
    cmd.args(args);
    cmd.stdin(Stdio::null());
    cmd.env_remove("TOLTRIM_DB");
    cmd.output()
        .unwrap_or_else(|e| panic!("cannot start {BIN}: {e}"))
}

fn stdout_of(out: &Output) -> String {
    String::from_utf8(out.stdout.clone()).expect("the output is UTF-8")
}

fn stderr_of(out: &Output) -> String {
    String::from_utf8(out.stderr.clone()).expect("the output is UTF-8")
}

#[test]
fn help_goes_to_stdout_and_names_every_proxied_command_family() {
    let out = run(&["--help"]);

    assert_eq!(out.status.code(), Some(0), "`--help` must succeed");
    assert!(
        out.stderr.is_empty(),
        "help the caller asked for is output, not diagnostics: {}",
        stderr_of(&out)
    );
    let usage = stdout_of(&out);
    assert!(usage.contains("USAGE:"), "got:\n{usage}");
    assert!(usage.contains("toltrim"), "got:\n{usage}");

    for family in FAMILIES {
        for word in family.split_whitespace() {
            assert!(
                usage.contains(word),
                "`--help` does not name the `{family}` family: {word} is missing from:\n{usage}"
            );
        }
    }

    // The verbs toltrim implements itself are part of the published surface too.
    for verb in ["init", "gain", "--raw", "--version"] {
        assert!(usage.contains(verb), "`--help` omits `{verb}`:\n{usage}");
    }
    assert!(
        usage.contains("Everything else is executed unchanged"),
        "the usage must say that unproxied commands are untouched:\n{usage}"
    );
}

#[test]
fn version_prints_the_crate_version() {
    for flag in ["--version", "-V"] {
        let out = run(&[flag]);
        assert_eq!(out.status.code(), Some(0), "`{flag}` must succeed");
        assert!(
            out.stderr.is_empty(),
            "`{flag}` wrote to stderr: {}",
            stderr_of(&out)
        );
        assert_eq!(
            stdout_of(&out),
            format!("toltrim {}\n", env!("CARGO_PKG_VERSION")),
            "`{flag}` must print `toltrim <version>`"
        );
    }
}

#[test]
fn no_arguments_is_a_usage_error_on_stderr() {
    let out = run(&[]);

    assert_eq!(out.status.code(), Some(2), "no arguments is a usage error");
    assert!(
        out.stdout.is_empty(),
        "a usage error must not write usage to stdout: {}",
        stdout_of(&out)
    );
    let message = stderr_of(&out);
    assert!(message.contains("USAGE:"), "got:\n{message}");
    assert!(message.contains("toltrim"), "got:\n{message}");
}

/// A directory that outlives one test and is removed when it ends.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let base = std::env::var_os("CARGO_TARGET_TMPDIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|since| since.as_nanos())
            .unwrap_or_default();
        let path = base.join(format!(
            "toltrim-cli-{label}-{}-{nanos}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path)
            .unwrap_or_else(|e| panic!("cannot create {}: {e}", path.display()));
        TempDir { path }
    }

    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Where a Gain Record would land for this environment, mirroring ADR-0006.
fn fresh_db(dir: &TempDir) -> PathBuf {
    #[cfg(unix)]
    {
        dir.join("xdg").join("toltrim").join("history.db")
    }
    #[cfg(windows)]
    {
        dir.join("local").join("toltrim").join("history.db")
    }
    #[cfg(not(any(unix, windows)))]
    {
        dir.join("unused").join("history.db")
    }
}

/// The binary as a fresh install finds it: no `TOLTRIM_DB`, an empty data
/// directory, and a `PATH` with no toolchain on it, so neither a database nor a
/// JDK nor Rust can be involved in starting it.
fn fresh_env(dir: &TempDir) -> Command {
    let empty_path = dir.join("empty-path");
    fs::create_dir_all(&empty_path).expect("the empty PATH directory");

    let mut cmd = Command::new(BIN);
    cmd.env_remove("TOLTRIM_DB");
    cmd.env("HOME", dir.join("home"));
    cmd.env("XDG_DATA_HOME", dir.join("xdg"));
    cmd.env("LOCALAPPDATA", dir.join("local"));
    cmd.env("PATH", &empty_path);
    cmd.stdin(Stdio::null());
    cmd
}

#[test]
fn a_fresh_install_starts_without_a_database_or_a_toolchain() {
    let dir = TempDir::new("fresh-install");
    let db = fresh_db(&dir);

    for args in [["--help"], ["--version"]] {
        let out = fresh_env(&dir)
            .args(args)
            .output()
            .unwrap_or_else(|e| panic!("cannot start {BIN}: {e}"));
        assert_eq!(
            out.status.code(),
            Some(0),
            "`{}` must succeed on a fresh install: {}",
            args[0],
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!out.stdout.is_empty(), "`{}` printed nothing", args[0]);
        assert!(
            !db.exists(),
            "`{}` created a Gain Record database; starting toltrim must not need one",
            args[0]
        );
    }

    // The accounting view is the part that needs the database, and it creates
    // one in the fresh install's own data directory — never the developer's.
    let out = fresh_env(&dir)
        .arg("gain")
        .output()
        .unwrap_or_else(|e| panic!("cannot start {BIN}: {e}"));
    assert_eq!(
        out.status.code(),
        Some(0),
        "`toltrim gain` failed on a fresh install: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let view = String::from_utf8(out.stdout).expect("the gain view is UTF-8");
    assert!(
        view.contains("Commands proxied: 0"),
        "a fresh install has no Gain Records:\n{view}"
    );
    assert!(
        db.exists(),
        "`toltrim gain` did not create its database at {}",
        db.display()
    );
}

// ---------------------------------------------------------------------------
// The README drift guard.
// ---------------------------------------------------------------------------

/// Read a file from the checkout.
fn checkout_file(relative: &str) -> String {
    let path = Path::new(MANIFEST_ROOT).join(relative);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// The Proxied Command list a document publishes with `toltrim init`'s bullet
/// form: `- \`toltrim <command>\``.
fn bullet_commands(document: &str) -> BTreeSet<String> {
    document
        .lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("- `toltrim ")?;
            Some(
                rest.strip_suffix('`')
                    .unwrap_or_else(|| panic!("a bullet does not end its code span: {line:?}"))
                    .to_string(),
            )
        })
        .collect()
}

/// The Proxied Command families the binary's own usage text advertises, parsed
/// out of `src/main.rs` so the two published lists can be compared.
fn usage_commands() -> BTreeSet<String> {
    let source = checkout_file("src/main.rs");
    let marker = "Proxied commands (output is trimmed):";
    let start = source
        .find(marker)
        .unwrap_or_else(|| panic!("src/main.rs no longer has {marker:?}"))
        + marker.len();
    let rest = &source[start..];
    let end = rest
        .find("Everything else is executed unchanged")
        .unwrap_or_else(|| panic!("src/main.rs no longer says what happens to other commands"));

    let mut commands = BTreeSet::new();
    for line in rest[..end].lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.contains('|') {
            let (program, list) = line
                .split_once(' ')
                .unwrap_or_else(|| panic!("a usage line lists no program: {line:?}"));
            for subcommand in list.split('|') {
                commands.insert(format!("{program} {}", subcommand.trim()));
            }
        } else if line.contains(", ") {
            for program in line.split(", ") {
                commands.insert(program.trim().to_string());
            }
        } else {
            commands.insert(line.to_string());
        }
    }
    assert!(!commands.is_empty(), "the usage text advertises nothing");
    commands
}

/// Every documented command must be one the code really proxies.
fn assert_commands_are_proxied(commands: &BTreeSet<String>, source: &str) {
    for command in commands {
        let words: Vec<String> = command.split_whitespace().map(str::to_owned).collect();
        let (program, args) = words.split_first().expect("a documented command has a program");
        let tool = trim::detect(program);
        assert_ne!(
            tool,
            Tool::Unknown,
            "{source} advertises `{command}`, which the code does not detect"
        );
        assert!(
            trim::is_proxied(tool, args),
            "{source} advertises `{command}`, which is not a Proxied Command"
        );
    }
}

#[test]
fn the_readme_proxied_commands_are_really_proxied() {
    let readme = checkout_file("README.md");
    let commands = bullet_commands(&readme);

    assert!(
        !commands.is_empty(),
        "README.md advertises no Proxied Commands in the `- `toltrim …`` form"
    );
    assert_commands_are_proxied(&commands, "README.md");

    // Every family the test names must be there, and nothing extra.
    let expected: BTreeSet<String> = FAMILIES.iter().map(|f| (*f).to_string()).collect();
    assert_eq!(
        commands, expected,
        "README.md's Proxied Command list drifted from the shipped command set"
    );
}

#[test]
fn the_readme_the_usage_text_and_the_init_mapping_publish_the_same_list() {
    let readme = bullet_commands(&checkout_file("README.md"));
    let usage = usage_commands();
    let mapping = bullet_commands(&checkout_file("src/init.rs"));
    assert!(
        !mapping.is_empty(),
        "src/init.rs no longer has a bullet mapping to compare"
    );
    assert_eq!(
        usage, readme,
        "`toltrim --help` and README.md advertise different Proxied Commands"
    );
    assert_eq!(
        mapping, readme,
        "`toltrim init`'s AGENTS.md mapping and README.md advertise different Proxied Commands"
    );
}

#[test]
fn the_readme_keeps_gradle_deferred_and_the_state_changing_git_commands_passthrough() {
    let readme = checkout_file("README.md");
    let commands = bullet_commands(&readme);

    // Gradle is deferred (ADR-0008): the README must say so, and must not
    // advertise it or its wrapper as a Proxied Command.
    assert!(
        readme.contains("Gradle is deferred"),
        "README.md no longer states the Gradle deferral"
    );
    assert!(
        readme.contains("gradle") && readme.contains("gradlew"),
        "README.md must name gradle and gradlew when it defers them"
    );
    for deferred in ["gradle", "gradlew"] {
        assert_eq!(
            trim::detect(deferred),
            Tool::Unknown,
            "{deferred} must not be detected as a Proxied tool"
        );
        assert!(
            !commands.contains(deferred),
            "README.md advertises `{deferred}`, which is deferred"
        );
    }

    // The always-Passthrough git subcommands are published as one machine-
    // readable line, and the code must agree: none may be proxied.
    let line = readme
        .lines()
        .find(|line| line.starts_with("Always Passthrough Commands: "))
        .unwrap_or_else(|| {
            panic!("README.md no longer publishes an `Always Passthrough Commands:` line")
        });
    // The published line ends in a full stop; the entries themselves do not.
    let listed: Vec<&str> = line["Always Passthrough Commands: ".len()..]
        .split(", ")
        .map(|entry| entry.trim_end_matches('.'))
        .collect();
    let expected: Vec<String> = ALWAYS_PASSTHROUGH
        .iter()
        .map(|sub| format!("git {sub}"))
        .collect();
    assert_eq!(
        listed, expected,
        "README.md's always-Passthrough git list drifted"
    );
    assert!(
        readme.contains("never interpreted"),
        "README.md must say these commands are never interpreted"
    );

    for subcommand in ALWAYS_PASSTHROUGH {
        let args = vec![(*subcommand).to_string()];
        assert!(
            !trim::is_proxied(Tool::Git, &args),
            "README.md calls `git {subcommand}` a Passthrough Command but the code proxies it"
        );
        assert!(
            !commands.contains(&format!("git {subcommand}")),
            "README.md advertises `git {subcommand}` as a Proxied Command"
        );
    }
}

/// The git subcommands `is_proxied` itself enumerates, parsed out of
/// `src/trim/mod.rs`. Git is the one family the code lists explicitly — Maven
/// and the listings proxy every argument — so this is where a new rule can be
/// added to the code without touching any published list.
fn proxied_git_subcommands() -> BTreeSet<String> {
    let source = checkout_file("src/trim/mod.rs");
    let marker = "Tool::Git => matches!(";
    let start = source
        .find(marker)
        .unwrap_or_else(|| panic!("src/trim/mod.rs no longer has `{marker}`"))
        + marker.len();
    let rest = &source[start..];
    let some = rest
        .find("Some(")
        .unwrap_or_else(|| panic!("the git arm no longer matches on `Some(...)`"))
        + "Some(".len();
    let list = &rest[some..];
    let end = list
        .find(')')
        .unwrap_or_else(|| panic!("the git subcommand list is not closed"));

    let mut subcommands = BTreeSet::new();
    let mut parts = list[..end].split('"');
    parts.next();
    while let Some(subcommand) = parts.next() {
        if !subcommand.is_empty() {
            subcommands.insert(subcommand.to_string());
        }
        parts.next();
    }
    assert!(
        !subcommands.is_empty(),
        "no git subcommand was parsed out of src/trim/mod.rs; the parser is stale"
    );
    subcommands
}

#[test]
fn every_git_subcommand_the_code_proxies_is_documented() {
    let readme = bullet_commands(&checkout_file("README.md"));

    for subcommand in proxied_git_subcommands() {
        assert!(
            trim::is_proxied(Tool::Git, std::slice::from_ref(&subcommand)),
            "`git {subcommand}` is in the code's list but is not proxied"
        );
        assert!(
            readme.contains(&format!("git {subcommand}")),
            "the code proxies `git {subcommand}` but README.md does not advertise it"
        );
    }
}

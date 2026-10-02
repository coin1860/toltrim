//! `toltrim init` end to end. Init edits the project's `AGENTS.md`, so these
//! tests run the built binary in a project directory of their own and read the
//! file an agent would read. Each test creates a temporary project under
//! `std::env::temp_dir()` and removes it when it ends, even on panic.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const BIN: &str = env!("CARGO_BIN_EXE_toltrim");
const BEGIN: &str = "<!-- toltrim:begin -->";
const END: &str = "<!-- toltrim:end -->";

/// The Proxied Commands in scope for the MVP (ADR-0008). Every one of them must
/// be named in the mapping, because the mapping is how an agent learns the
/// prefix. Gradle is deliberately absent: the ADR defers it.
const PROXIED: &[&str] = &[
    "toltrim mvn test",
    "toltrim mvn verify",
    "toltrim mvn package",
    "toltrim mvn compile",
    "toltrim mvn install",
    "toltrim mvn clean",
    "toltrim mvn test-compile",
    "toltrim mvn integration-test",
    "toltrim git status",
    "toltrim git diff",
    "toltrim git log",
    "toltrim git show",
    "toltrim git branch",
    "toltrim ls",
    "toltrim dir",
    "toltrim tree",
];

/// A directory of this test's own, removed when the test ends.
struct Project(PathBuf);

impl Project {
    fn new(label: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let unique = format!(
            "toltrim-init-{label}-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("the clock is past the epoch")
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        );
        let root = std::env::temp_dir().join(unique);
        fs::create_dir_all(&root).expect("create the test project directory");
        Project(root)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn agents_md(&self) -> PathBuf {
        self.0.join("AGENTS.md")
    }

    fn read_agents_md(&self) -> String {
        fs::read_to_string(self.agents_md()).expect("AGENTS.md is readable")
    }

    /// Run `toltrim init` inside this project, exactly as an agent would.
    fn init(&self, args: &[&str]) -> Output {
        Command::new(BIN)
            .arg("init")
            .args(args)
            .current_dir(&self.0)
            .output()
            .expect("run the toltrim binary")
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn occurrences(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

/// Exactly one mapping block, so re-running init can never duplicate it.
fn assert_one_mapping_block(text: &str) {
    assert_eq!(
        occurrences(text, BEGIN),
        1,
        "expected exactly one `{BEGIN}`:\n{text}"
    );
    assert_eq!(
        occurrences(text, END),
        1,
        "expected exactly one `{END}`:\n{text}"
    );
}

/// What a reader of AGENTS.md must be able to see.
fn assert_mapping(text: &str) {
    for command in PROXIED {
        assert!(
            text.contains(command),
            "the mapping does not mention `{command}`:\n{text}"
        );
    }
    assert!(
        text.contains("prefix"),
        "the mapping must name the prefix:\n{text}"
    );
    assert!(
        text.contains("runs unchanged"),
        "the mapping must say everything else runs unchanged:\n{text}"
    );
    assert!(
        !text.to_ascii_lowercase().contains("gradle"),
        "Gradle is deferred and must not appear:\n{text}"
    );
}

/// The result a normal (non-preview) run leaves behind.
fn init_mapping(project: &Project) -> String {
    let output = project.init(&[]);
    assert!(
        output.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = project.read_agents_md();
    assert_mapping(&text);
    assert_one_mapping_block(&text);
    text
}

#[test]
fn init_creates_agents_md_with_the_proxied_command_mapping() {
    let project = Project::new("create");
    let text = init_mapping(&project);
    assert!(
        text.ends_with('\n'),
        "the created file should end in a newline:\n{text:?}"
    );
}

#[test]
fn init_appends_to_an_existing_agents_md_without_disturbing_it() {
    let project = Project::new("append");
    let existing = "# Project guidance\n\n- Always run the formatter.\n";
    fs::write(project.agents_md(), existing).expect("seed AGENTS.md");

    let text = init_mapping(&project);
    assert!(
        text.starts_with(existing),
        "existing content was disturbed:\n{text}"
    );
}

#[test]
fn init_twice_on_a_fresh_project_leaves_one_block_and_the_same_bytes() {
    let project = Project::new("twice-fresh");
    init_mapping(&project);
    let first = fs::read(project.agents_md()).expect("read after the first init");

    let output = project.init(&[]);
    assert!(output.status.success());
    let second = fs::read(project.agents_md()).expect("read after the second init");

    assert_eq!(first, second, "the second init changed the file");
    assert_one_mapping_block(&String::from_utf8(first).expect("AGENTS.md is UTF-8"));
}

#[test]
fn init_twice_with_existing_content_keeps_it_and_the_same_bytes() {
    let project = Project::new("twice-existing");
    let existing = "# Project guidance\n\nkeep this line\n";
    fs::write(project.agents_md(), existing).expect("seed AGENTS.md");
    init_mapping(&project);
    let first = fs::read(project.agents_md()).expect("read after the first init");

    let output = project.init(&[]);
    assert!(output.status.success());
    let second = fs::read(project.agents_md()).expect("read after the second init");

    assert_eq!(first, second, "the second init changed the file");
    let text = String::from_utf8(first).expect("AGENTS.md is UTF-8");
    assert!(
        text.starts_with(existing),
        "surrounding content moved:\n{text}"
    );
    assert_one_mapping_block(&text);
}

#[test]
fn init_replaces_a_stale_mapping_block_in_place() {
    let project = Project::new("stale");
    let before = "before the mapping\n";
    let after = "after the mapping\n";
    let seeded = format!("{before}{BEGIN}\nstale mapping\n{END}\n{after}");
    fs::write(project.agents_md(), seeded).expect("seed AGENTS.md");

    let text = init_mapping(&project);
    assert!(
        text.starts_with(before),
        "content before the block was disturbed:\n{text}"
    );
    assert!(
        text.ends_with(after),
        "content after the block was disturbed:\n{text}"
    );
    assert!(
        !text.contains("stale mapping"),
        "the old mapping survived:\n{text}"
    );
}

#[test]
fn dry_run_reports_the_mapping_and_changes_nothing() {
    let project = Project::new("dry-run");
    let existing = "# Project guidance\n\nkeep this line\n";
    fs::write(project.agents_md(), existing).expect("seed AGENTS.md");

    let output = project.init(&["--dry-run"]);
    assert!(
        output.status.success(),
        "init --dry-run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let preview = String::from_utf8(output.stdout).expect("the preview is UTF-8");
    assert_mapping(&preview);
    assert_one_mapping_block(&preview);
    assert!(
        preview.contains(existing.trim_end()),
        "the preview must show the whole would-be file:\n{preview}"
    );
    assert_eq!(
        fs::read_to_string(project.agents_md()).expect("AGENTS.md is readable"),
        existing,
        "--dry-run must not touch the file"
    );

    // With no AGENTS.md at all, the preview still creates nothing.
    let empty = Project::new("dry-run-empty");
    let output = empty.init(&["--dry-run"]);
    assert!(output.status.success());
    assert_mapping(&String::from_utf8(output.stdout).expect("the preview is UTF-8"));
    assert!(
        !empty.agents_md().exists(),
        "--dry-run must not create AGENTS.md"
    );
}

#[test]
fn init_writes_nothing_outside_the_project() {
    let sandbox = Project::new("sandbox");
    let project = sandbox.path().join("project");
    let home = sandbox.path().join("home");
    fs::create_dir_all(&project).expect("create the project");
    fs::create_dir_all(&home).expect("create the fake home");

    let output = Command::new(BIN)
        .arg("init")
        .current_dir(&project)
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("APPDATA", home.join("AppData/Roaming"))
        .output()
        .expect("run the toltrim binary");
    assert!(
        output.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(project.join("AGENTS.md").is_file());
    let written: Vec<PathBuf> = fs::read_dir(&home)
        .expect("read the fake home")
        .map(|entry| entry.expect("a directory entry").path())
        .collect();
    assert!(
        written.is_empty(),
        "init wrote global agent configuration: {written:?}"
    );
}

#[test]
fn init_fails_with_one_line_when_agents_md_cannot_be_written() {
    let project = Project::new("blocked");
    fs::create_dir(project.agents_md()).expect("block AGENTS.md with a directory");

    let output = project.init(&[]);
    assert!(
        !output.status.success(),
        "init must fail rather than write a partial mapping"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr is UTF-8");
    let lines: Vec<&str> = stderr.lines().collect();
    assert_eq!(
        lines.len(),
        1,
        "expected exactly one explanatory line, got: {stderr:?}"
    );
    assert!(
        lines[0].starts_with("toltrim init: ") && lines[0].len() > "toltrim init: ".len(),
        "got: {stderr:?}"
    );
    assert!(
        project.agents_md().is_dir(),
        "a failed init must not replace the path"
    );
}

#[cfg(unix)]
#[test]
fn init_fails_with_one_line_when_the_project_directory_is_read_only() {
    use std::os::unix::fs::PermissionsExt;

    let project = Project::new("read-only");
    fs::set_permissions(project.path(), fs::Permissions::from_mode(0o555))
        .expect("make the project read-only");

    // A test runner that ignores the write bit (for example root) cannot
    // produce this failure, so there is nothing to assert there.
    let probe = project.path().join("probe");
    if fs::write(&probe, b"x").is_ok() {
        let _ = fs::remove_file(&probe);
        fs::set_permissions(project.path(), fs::Permissions::from_mode(0o755))
            .expect("restore the project permissions");
        return;
    }

    let output = project.init(&[]);
    fs::set_permissions(project.path(), fs::Permissions::from_mode(0o755))
        .expect("restore the project permissions");

    assert!(
        !output.status.success(),
        "init must fail in a directory it cannot write"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr is UTF-8");
    let lines: Vec<&str> = stderr.lines().collect();
    assert_eq!(
        lines.len(),
        1,
        "expected exactly one explanatory line, got: {stderr:?}"
    );
    assert!(lines[0].starts_with("toltrim init: "), "got: {stderr:?}");
    assert!(
        !project.agents_md().exists(),
        "a failed init must not leave a partial file"
    );
}

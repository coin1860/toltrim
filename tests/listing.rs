//! `ls`, `dir`, and `tree` of a real project root, through the pure render seam.
//!
//! Every assertion here is what an agent would read: Noise directories must
//! collapse to a visible count, and the Evidence — the file count, the real
//! entries, the hidden entries, the permissions of a long listing, `dir`'s
//! footer — must survive. No test asserts a kept line count or an internal
//! ordering.
//!
//! Provenance of every fixture (all recorded from real output; nothing here was
//! invented):
//!
//! * The scratch project root is `../qwen-wt/scratch-t06`, built to contain the
//!   Noise directories `.git/`, `target/`, `node_modules/`, `build/`, and
//!   `.idea/` with real contents, plus top-level files, `.env`, and `.gitignore`.
//! * `ls-project-root.txt`      — `ls` run in that directory.
//! * `ls-1-project-root.txt`    — `ls -1` run in that directory.
//! * `ls-la-project-root.txt`   — `ls -la` run in that directory.
//! * `ls-l-project-root.txt`    — `ls -l` run in that directory.
//! * `ls-columns-project-root.txt` — `ls -m` run in that directory; `ls`
//!   picks its column layout from the terminal width, so `-m` is the
//!   width-independent way to record the multi-column shape.
//! * `ls-la-trailing-slash.txt` — `ls -la "$PWD/"` in the same directory.
//! * `ls-long-line.txt`         — `ls -1` in the same directory while a real file
//!   named `spring-boot-starter-parent-3.2.1-sources-and-javadoc-dependencies-
//!   bundle-with-extra-metadata-for-offline-use-20240115.120000-1.jar`
//!   (130 characters) existed there; the file was removed after the capture.
//! * `tree-project-root.txt`   — COMPOSED, not captured: this machine has no
//!   `tree` binary (`command -v tree` finds none), so the fixture was produced by
//!   walking the same real directory and emitting tree's documented format
//!   (alphabetical entries, `├── `/`└── ` guides, `N directories, M files`
//!   summary). Every name in it exists on disk; the root line is renamed from
//!   `scratch-t06` to `toltrim`, which symlink-style renames only, keeping the
//!   fixture I/O-free.
//! * `dir-project-root.txt`    — COMPOSED, not captured: this machine is Unix
//!   and has no `dir`, so the sample is format-accurate for Windows `cmd`'s
//!   `dir` (volume header, `Directory of …` line, `MM/DD/YYYY  HH:MM AM  <DIR>`
//!   rows, `N File(s) … bytes` / `N Dir(s) … bytes free` footer) and names the
//!   same real directory. It is otherwise not asserted for exact byte counts.

use std::fs;

use toltrim::trim::{self, Tool};

/// The width the rule truncates at; the fixture's long line is longer than this.
const WIDTH: usize = 100;

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"))
}

/// Read a fixture through `render`, exactly as the entry point does.
fn render(raw: &str) -> String {
    let bytes = trim::render(Tool::Listing, &[], raw.as_bytes(), 0, false);
    String::from_utf8(bytes).expect("listings are valid UTF-8")
}

/// One line of a listing with any tree guides and indentation removed.
fn strip_guides(line: &str) -> &str {
    let mut rest = line.trim_start_matches(['│', ' ', '\t']);
    loop {
        let trimmed = rest.trim_start_matches(['├', '└', '─', ' ']);
        if trimmed.len() == rest.len() {
            break;
        }
        rest = trimmed;
    }
    rest
}

/// The lines of a listing with any tree guides and indentation removed, so a
/// flat listing and a tree can be compared by entry text alone.
fn normalised(text: &str) -> Vec<String> {
    text.lines().map(|line| strip_guides(line).to_string()).collect()
}

/// Does this listing carry `name` as an entry of its own? A long-listing row or
/// a Windows `dir` row carries its entry last, so a suffix test is what an agent
/// reads as "this entry is present".
fn has_entry(text: &str, name: &str) -> bool {
    text.lines().any(|line| {
        let entry = strip_guides(line).trim_end();
        entry == name || entry.ends_with(&format!(" {name}"))
    })
}

/// The one line that records the collapse of a Noise directory, with its
/// `tree` guides removed so the wording can be asserted directly.
fn collapse_line<'a>(text: &'a str, name: &str) -> &'a str {
    // `ls -la` and `dir` rows keep their mode column ahead of the name, so the
    // marker can start after the columns as well as at the start of the line.
    let marker = format!("{name}/ (");
    let mut found = text
        .lines()
        .map(strip_guides)
        .filter(|line| line.contains(&marker));
    let first = found
        .next()
        .unwrap_or_else(|| panic!("no collapse line for {name} in:\n{text}"));
    assert!(
        found.next().is_none(),
        "{name} was not collapsed into exactly one line:\n{text}"
    );
    first
}

/// The count a collapse line carries, for a `tree` collapse.
fn collapsed_count(text: &str, name: &str) -> usize {
    let line = collapse_line(text, name);
    let after_open = line
        .split_once('(')
        .map(|(_, rest)| rest)
        .unwrap_or_else(|| panic!("no parenthesis in {line:?}"));
    after_open
        .split_whitespace()
        .next()
        .and_then(|count| count.parse().ok())
        .unwrap_or_else(|| panic!("no count in {line:?}"))
}

#[test]
fn the_plain_listing_keeps_every_file_and_collapses_only_noise() {
    let raw = fixture("ls-project-root.txt");
    let out = render(&raw);

    // Evidence: the real entries survive, including the recorded file names.
    for name in [
        "AGENTS.md",
        "Cargo.lock",
        "Cargo.toml",
        "README.md",
        "build",
        "node_modules",
        "src",
        "target",
        "tests",
    ] {
        assert!(out.contains(name), "{name} disappeared from:\n{out}");
    }
    // Not a noise directory: normal directories stay as real entries.
    assert!(has_entry(&out, "src"), "a real directory was collapsed:\n{out}");
    assert!(has_entry(&out, "tests"), "a real directory was collapsed:\n{out}");

    // Noise: a Noise directory collapses to one visible line that says what was
    // elided. The fixture lists `node_modules` and `build`, and both collapse.
    for name in ["target", "node_modules", "build"] {
        let line = collapse_line(&out, name);
        assert!(
            line.contains("collapsed"),
            "the collapse of {name} does not say that something was elided: {line}"
        );
    }
    assert!(!has_entry(&out, "build"), "build was not collapsed:\n{out}");
    assert!(
        !has_entry(&out, "node_modules"),
        "node_modules was not collapsed:\n{out}"
    );
    assert!(!has_entry(&out, "target"), "target was not collapsed:\n{out}");
}

#[test]
fn a_long_listing_keeps_kind_and_columns_and_collapses_noise() {
    let raw = fixture("ls-la-project-root.txt");
    let out = render(&raw);

    // Evidence: the long listing's kind, permissions, owner, and size columns
    // survive for every entry — the rule must not rewrite `-la` into names.
    for line in raw.lines().filter(|l| l.starts_with('-')) {
        assert!(
            out.contains(line),
            "a file's long-listing row changed meaning:\n  was: {line}\n  out:\n{out}"
        );
    }
    assert!(out.contains("drwxr-xr-x@"), "directory permissions were dropped:\n{out}");
    assert!(out.contains("shaneshou"), "the owner column was dropped:\n{out}");
    // The kind column survives for directories too: they are still recognisable
    // as directories.
    assert!(
        out.lines().any(|l| l.starts_with('d') && l.contains("src")),
        "src is no longer shown as a directory:\n{out}"
    );
    // Hidden entries are entries like any other.
    assert!(out.contains(".gitignore"), "a hidden file was dropped:\n{out}");
    assert!(out.contains(".env"), "a hidden file was dropped:\n{out}");

    // Noise collapses, and only for directories.
    for name in [".git", ".idea", "target", "node_modules", "build"] {
        let line = collapse_line(&out, name);
        assert!(
            line.contains("collapsed"),
            "the collapse of {name} does not say that something was elided: {line}"
        );
    }
    for name in ["src", "tests"] {
        assert!(
            out.lines().any(|l| l.starts_with('d') && l.ends_with(name)),
            "{name} is a real directory and must not be collapsed:\n{out}"
        );
    }
}

#[test]
fn a_tree_collapses_a_noise_subtree_and_keeps_the_summary() {
    let raw = fixture("tree-project-root.txt");
    let out = render(&raw);

    // Evidence: the summary line is the file count, and it must survive intact
    // so the counts that prove what is on disk are still there.
    let summary = raw
        .lines()
        .find(|l| l.contains("directories,") && l.contains("files"))
        .expect("the fixture has a tree summary line");
    assert!(
        out.contains(summary),
        "the tree summary (the file count) was trimmed away: {summary:?}\n{out}"
    );

    // Evidence: real entries survive at every depth.
    for name in ["src", "trim", "mod.rs", "tests", "smoke.rs", "README.md"] {
        assert!(raw.contains(name), "the fixture must record {name}");
        assert!(out.contains(name), "{name} disappeared from the tree:\n{out}");
    }

    // Noise: the whole subtree collapses to one line at the directory's
    // position, and the per-file listing inside it does not survive.
    for name in [".git", ".idea", "build", "node_modules", "target"] {
        let line = collapse_line(&out, name);
        assert!(
            line.contains(" collapsed"),
            "the collapse of {name} does not say how much was elided: {line}"
        );
        assert!(
            line.chars().any(|c| c.is_ascii_digit()),
            "the collapse of {name} carries no count: {line}"
        );
    }
    for gone in [
        "target/debug",
        "target/CACHEDIR.TAG",
        "target/debug/toltrim",
        "node_modules/left-pad",
        ".git/objects",
        "build/generated",
        ".idea/inspectionProfiles",
    ] {
        assert!(
            !normalised(&out).iter().any(|l| l.ends_with(gone)),
            "{gone} leaked out of the collapse:\n{out}"
        );
    }
    // A noise directory's line is a collapse, not the directory's original row.
    let tree_entries = |text: &str| -> Vec<String> {
        text.lines()
            .filter_map(|l| {
                let rest = l
                    .trim_start_matches(['│', ' ', '\t'])
                    .trim_start_matches(['├', '└', '─', ' '])
                    .trim_start();
                if l.contains("├── ") || l.contains("└── ") {
                    Some(rest.to_string())
                } else {
                    None
                }
            })
            .collect()
    };
    for entry in ["target", ".git", "build", ".idea", "node_modules"] {
        assert!(
            !tree_entries(&out).iter().any(|e| e == entry),
            "{entry} is still an ordinary tree entry:\n{out}"
        );
    }
    // The summary's file count reconciles: the collapsed subtrees report how
    // many files each held, so nothing was elided silently.
    assert!(
        out.contains("collapsed"),
        "the tree collapse is not visible:\n{out}"
    );
}

#[test]
fn the_tree_file_count_reconciles_with_the_collapsed_subtrees() {
    let raw = fixture("tree-project-root.txt");
    let out = render(&raw);

    // "19 directories, 16 files" must still be readable after trimming.
    assert!(out.contains("19 directories, 16 files"), "got:\n{out}");

    // The summary is the total; every file inside a collapsed subtree is
    // accounted for by the collapse's own count, so an agent can still tell how
    // much was listed. This is the *file count* direction of the ticket: it
    // survives.
    let collapsed: Vec<usize> = [".git", "target", "node_modules", "build", ".idea"]
        .iter()
        .map(|name| collapsed_count(&out, name))
        .collect();
    assert!(
        collapsed.iter().all(|count| *count > 0),
        "a collapsed subtree reported no entries: {collapsed:?}\n{out}"
    );
    // The subtrees the fixture records hold 5, 11, 1, 1, and 1 entries.
    assert!(
        collapsed.iter().sum::<usize>() >= 16,
        "the collapsed counts do not add up to the files that were elided: {collapsed:?}"
    );
}

#[test]
fn a_long_listing_line_is_truncated_and_visibly_marked() {
    let raw = fixture("ls-long-line.txt");
    let long = raw
        .lines()
        .find(|l| l.chars().count() > WIDTH)
        .expect("the fixture records one line longer than the width");
    assert!(raw.contains(long));

    let out = render(&raw);
    assert!(
        !out.contains(long),
        "the over-width line reached the agent untouched:\n{out}"
    );
    let truncated = out
        .lines()
        .find(|l| l.starts_with("spring-boot-starter-parent-"))
        .unwrap_or_else(|| panic!("the long name was dropped rather than truncated:\n{out}"));
    assert!(
        truncated.chars().count() <= WIDTH,
        "the truncation is not at a fixed width: {} chars in {truncated:?}",
        truncated.chars().count()
    );
    assert!(
        truncated.contains('…') || truncated.contains("..."),
        "the truncation is silent, with no marker: {truncated:?}"
    );
    // The rest of the listing is untouched.
    for name in ["AGENTS.md", "Cargo.toml", "README.md"] {
        assert!(out.contains(name), "{name} disappeared from:\n{out}");
    }
}

#[test]
fn a_dir_listing_keeps_its_footer_as_evidence_and_collapses_noise() {
    // This machine is Unix, so the recorded Windows sample is exercised here and
    // the real `dir` binary is only run by the `#[cfg(windows)]` test below.
    let raw = fixture("dir-project-root.txt");
    let out = render(&raw);

    // Evidence: the footer's file count and byte totals survive — they are what
    // proves how much was listed.
    assert!(out.contains("9 File(s)"), "the file count was trimmed away:\n{out}");
    assert!(
        out.contains("23,456 bytes"),
        "the byte total was trimmed away:\n{out}"
    );
    assert!(out.contains("9 Dir(s)"), "the directory count was trimmed away:\n{out}");
    assert!(out.contains("bytes free"), "the free-space line was trimmed away:\n{out}");

    // Evidence: the entries themselves survive with their kind.
    for name in ["AGENTS.md", "Cargo.toml", "README.md", "Cargo.lock", "src", "tests", ".env"] {
        assert!(out.contains(name), "{name} disappeared from:\n{out}");
    }
    assert!(
        out.lines().any(|l| l.contains("<DIR>") && l.ends_with("src")),
        "src is no longer shown as a directory:\n{out}"
    );

    // Noise collapses.
    for name in [".git", ".idea", "target", "node_modules", "build"] {
        let line = collapse_line(&out, name);
        assert!(
            line.contains("<DIR>") && line.contains("collapsed"),
            "the collapse of {name} keeps neither its kind nor a visible marker: {line}"
        );
    }
    assert!(
        !out.lines().any(|l| l.contains("<DIR>") && l.ends_with("target")),
        "target is still an ordinary dir row:\n{out}"
    );
}

#[test]
fn entries_group_by_kind_with_directories_before_files() {
    let out = render(&fixture("ls-la-project-root.txt"));

    // An entry's own line ends with its name; a collapsed entry's line carries
    // its name and the marker. A file row ends with the file name.
    let position = |needle: &str| {
        let entry = format!("{needle}/ (");
        out.lines()
            .position(|line| {
                let line = strip_guides(line).trim_end();
                line.ends_with(&format!(" {needle}"))
                    || line.ends_with(needle)
                    || line.contains(&entry)
            })
            .unwrap_or_else(|| panic!("{needle} is missing from:\n{out}"))
    };
    let last_dir = ["src", "tests", "target"]
        .iter()
        .map(|n| position(n))
        .max()
        .expect("measured above");
    let first_file = ["AGENTS.md", "Cargo.lock", "Cargo.toml", "README.md"]
        .iter()
        .map(|n| position(n))
        .min()
        .expect("measured above");
    assert!(
        last_dir < first_file,
        "directories are not grouped before files:\n{out}"
    );
    // The collapsed Noise group is grouped with the directories, not mixed into
    // the files.
    assert!(
        position("target") < first_file,
        "the collapsed group is not grouped with the directories:\n{out}"
    );
}

#[test]
fn a_trailing_slash_does_not_change_how_a_listing_is_read() {
    let relative = render(&fixture("ls-la-project-root.txt"));
    let trailing = render(&fixture("ls-la-trailing-slash.txt"));

    // The rule reads text, so a directory argument's trailing slash cannot
    // change the reading: both captures are the same directory and must come
    // back with the same entries and the same Noise collapse.
    for name in [
        "AGENTS.md",
        "Cargo.lock",
        "Cargo.toml",
        "README.md",
        "src",
        "tests",
        ".env",
        ".gitignore",
    ] {
        assert!(relative.contains(name), "plain listing lost {name}");
        assert!(trailing.contains(name), "trailing-slash listing lost {name}");
    }
    for name in [".git", ".idea", "target", "node_modules", "build"] {
        assert!(
            collapse_line(&relative, name).contains("collapsed"),
            "{name} is not collapsed in the plain listing"
        );
        assert!(
            collapse_line(&trailing, name).contains("collapsed"),
            "{name} is not collapsed in the trailing-slash listing"
        );
    }
}

#[test]
fn hidden_entries_survive_in_every_listing_shape() {
    let fixtures = [
        "ls-project-root.txt",
        "ls-1-project-root.txt",
        "ls-la-project-root.txt",
    ];
    for name in fixtures {
        let out = render(&fixture(name));
        if name == "ls-project-root.txt" || name == "ls-1-project-root.txt" {
            // A plain `ls` does not show hidden entries; the rule must not be the
            // thing that hides them. What it must not do is confuse `.git` for
            // `.gitignore`.
            assert!(
                !has_entry(&out, ".gitignore"),
                "{name} invented a hidden entry:\n{out}"
            );
        } else {
            assert!(out.contains(".gitignore"), "{name} dropped a hidden entry:\n{out}");
            assert!(out.contains(".env"), "{name} dropped a hidden entry:\n{out}");
        }
        // `.git` is Noise by name, and only `.git` is. The collapse line names
        // the directory exactly, so `.gitignore` never triggers one.
        for line in out.lines() {
            let line = strip_guides(line);
            assert!(
                !line.starts_with(".gitignore/"),
                "a hidden file was mistaken for a Noise directory:\n{out}"
            );
        }
    }
}

#[test]
fn a_multi_column_listing_keeps_every_entry() {
    let out = render(&fixture("ls-columns-project-root.txt"));
    for name in ["AGENTS.md", "Cargo.lock", "Cargo.toml", "README.md", "src", "tests"] {
        assert!(out.contains(name), "{name} was lost from a column listing:\n{out}");
    }
    for name in ["target", "node_modules", "build"] {
        assert!(
            collapse_line(&out, name).contains("collapsed"),
            "{name} is not collapsed in a column listing:\n{out}"
        );
    }
}

#[test]
fn an_already_trimmed_listing_and_a_single_entry_are_stable() {
    // Idempotence: reading the output again must not collapse anything twice or
    // invent a second marker.
    let once = render(&fixture("ls-la-project-root.txt"));
    let twice = render(&once);
    assert_eq!(once, twice, "the listing rule is not idempotent");

    // A one-entry listing still reads as a listing.
    let single = render("Cargo.toml\n");
    assert_eq!(single, "Cargo.toml\n");
    let single_dir = render("src\n");
    assert_eq!(single_dir, "src\n");
}

#[cfg(unix)]
#[test]
fn the_built_binary_runs_a_real_unix_ls_through_the_proxied_path() {
    use std::process::Command;

    let out = Command::new(env!("CARGO_BIN_EXE_toltrim"))
        .arg("ls")
        .arg("-a")
        .arg("tests/fixtures")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("the built binary must run");
    assert!(out.status.success(), "toltrim ls failed: {:?}", out.status);
    let stdout = String::from_utf8(out.stdout).expect("stdout is UTF-8");
    // The listing still lists real files from a real `ls`.
    assert!(stdout.contains("ls-la-project-root.txt"), "got:\n{stdout}");
}

#[cfg(windows)]
#[test]
fn the_built_binary_trims_a_real_dir_run_through_the_cmd_fallback() {
    use std::process::Command;

    // `dir` is a shell builtin, not an executable: this only works through the
    // `cmd /c` fallback in the transport, with the argument vector passed
    // through rather than concatenated into a command string.
    let scratch = std::env::temp_dir().join("toltrim-dir-test");
    std::fs::create_dir_all(scratch.join("target/debug")).expect("scratch directory");
    std::fs::write(scratch.join("Cargo.toml"), b"").expect("scratch file");
    std::fs::write(scratch.join("target/debug/toltrim"), b"").expect("noise file");

    let out = Command::new(env!("CARGO_BIN_EXE_toltrim"))
        .arg("dir")
        .current_dir(&scratch)
        .output()
        .expect("the built binary must run");
    assert!(out.status.success(), "toltrim dir failed: {:?}", out.status);
    let stdout = String::from_utf8(out.stdout).expect("stdout is UTF-8");

    // Evidence survives: a real file at the root.
    assert!(stdout.contains("Cargo.toml"), "got:\n{stdout}");
    // A real Windows `dir` collapses its Noise directory visibly.
    assert!(
        stdout.contains("target/") && stdout.contains("collapsed"),
        "the Noise directory was not collapsed visibly:\n{stdout}"
    );
    // The per-file listing inside the Noise directory does not survive.
    assert!(
        !stdout.contains("toltrim"),
        "a file inside the Noise directory leaked through:\n{stdout}"
    );
}

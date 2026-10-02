//! `ls`, `dir`, and `tree` share one rule set: a Noise directory collapses to
//! one visible counted line, entries group by kind, and long lines are truncated
//! with a marker.
//!
//! The rule is a pure function of the listing text, so it never touches the
//! filesystem and never starts a process. `render` receives text that has already
//! had ANSI removed and is valid UTF-8.
//!
//! Everything the rule cannot recognise as an entry — `ls -l`'s `total` line, a
//! `tree` header or summary, `dir`'s volume header and byte footer, a `dir`
//! `<no extension>` row, an `ls -R` header — is Evidence and is emitted
//! unchanged. That is the Under-trimming Floor applied per line: an unrecognised
//! line survives rather than disappearing, so a listing can never lose more than
//! its Noise.

use std::fmt::Write as _;

/// Directories whose contents an agent never needs one line at a time. A
/// directory is Noise by its entry name alone, whatever produced the listing.
pub const NOISE_DIRECTORIES: &[&str] = &[".git", "target", "node_modules", "build", ".idea"];

/// The fixed width a listing line is truncated to. A width-independent constant
/// keeps `ls`, `dir`, and `tree` comparable.
pub const WIDTH: usize = 100;

/// The marker appended to a line that was shortened, so truncation is never
/// silent.
const ELLIPSIS: char = '…';

/// The guide a `tree` uses before a child entry.
const TREE_CHILD: &str = "├── ";
/// The guide a `tree` uses before its last child entry.
const TREE_LAST_CHILD: &str = "└── ";
/// The wording a flat `ls`/`dir` row uses for a Noise directory: the row does not
/// enumerate what is inside, so the collapse states that its contents were not
/// listed rather than inventing a count.
const NOISE_CONTENTS: &str = "contents collapsed";

/// `text` has already had ANSI removed and is valid UTF-8.
pub fn render(text: &str) -> String {
    let had_trailing_newline = text.ends_with('\n');
    let lines = listing_lines(text);
    if lines.is_empty() {
        // Empty input has no listing to read and no convention to preserve.
        return String::new();
    }

    let is_tree = lines.iter().any(|line| tree_entry(line).is_some());
    let entries: Vec<Entry> = lines
        .iter()
        .enumerate()
        .map(|(index, line)| classify(index, line, is_tree))
        .collect();

    // A listing that prints a long row — `ls -l`, `ls -la` — is read as rows: its
    // bare lines are headers, not entries. Every other listing is read as names,
    // so a directory name in a plain `ls` or a `dir` is never mistaken for a
    // header, and a one-entry listing is never dropped for looking like a root.
    let has_rows = entries.iter().any(|entry| entry.kind == Kind::Long);

    // Every `tree` line the Noise collapse swallows, and the one line each of
    // them collapses to.
    let mut suppressed = vec![false; lines.len()];
    let mut collapses: Vec<Collapse> = Vec::new();
    for entry in &entries {
        if let Some(subtree) = tree_noise_subtree(&entries, entry) {
            for slot in suppressed
                .iter_mut()
                .take(subtree.last + 1)
                .skip(entry.index + 1)
            {
                *slot = true;
            }
            let detail = format!(
                "{} {} collapsed",
                subtree.entry_count,
                count_label(subtree.entry_count)
            );
            collapses.push(Collapse {
                index: entry.index,
                text: format!("{}{}/ ({detail})", entry.guides, entry.name),
            });
        }
    }

    let mut evidence: Vec<String> = Vec::new();
    let mut collapsed: Vec<String> = Vec::new();
    let mut directories: Vec<String> = Vec::new();
    let mut files: Vec<String> = Vec::new();
    let mut rows: Vec<(usize, String)> = Vec::new();
    let mut out: Vec<String> = Vec::new();
    for entry in &entries {
        if suppressed[entry.index] {
            continue;
        }
        if matches!(entry.kind, Kind::Root | Kind::Unparsed) {
            // The `tree` root, its summary, `dir`'s volume header and byte
            // footer, and anything else the rule cannot read are Evidence: kept
            // exactly as printed, in place, and never dropped.
            let line = truncate(&entry.raw).0;
            out.push(line.clone());
            evidence.push(line);
            continue;
        }
        if let Some(collapse) = collapses.iter().find(|c| c.index == entry.index) {
            // A Noise collapse keeps the position the Noise directory had.
            out.push(truncate(&collapse.text).0);
            continue;
        }
        match entry.kind {
            // A `tree`'s root, its summary, `ls -l`'s `total` line, `dir`'s
            // volume header and byte footer, and anything else the rule cannot
            // read: Evidence, kept where it was.
            Kind::Root | Kind::Unparsed => unreachable!("handled above"),
            Kind::Long => rows.push((entry.index, truncate(&entry.raw).0)),
            // A `tree` line keeps the position its guides gave it: the hierarchy
            // is already a labelled grouping, and rebuilding the guides would
            // change the listing's meaning.
            Kind::Tree => out.push(truncate(&entry.raw).0),
            Kind::Name => {
                // A column of plain words with no name marker anywhere — no dot,
                // dash, underscore, or capital — is a summary or a sentence, not
                // a list of entries. The line is Evidence and is kept as it is.
                if !has_rows && entry.names_are_entries {
                    for name in &entry.names {
                        if is_noise_name(name) {
                            collapsed.push(truncate(&format!("{name}/ ({NOISE_CONTENTS})")).0);
                        } else {
                            files.push(truncate(name).0);
                        }
                    }
                } else {
                    // A row listing can still open with a header line, such as
                    // `ls -R`'s `./dir:`, and a column of plain words can be a
                    // summary. Either way it is Evidence: kept as it is, never
                    // dropped.
                    out.push(truncate(&entry.raw).0);
                    evidence.push(truncate(&entry.raw).0);
                }
            }
            Kind::Dir => {
                if is_noise_name(&entry.name) {
                    // The row's kind is Evidence, so the collapse keeps its mode
                    // column and drops only the repeated metadata.
                    let marker = format!("{}{}/ ({NOISE_CONTENTS})", entry.mode, entry.name);
                    collapsed.push(truncate(&marker).0);
                } else {
                    directories.push(truncate(&entry.raw).0);
                }
            }
        }
    }

    if !is_tree {
        // Flat listings group by kind: the Noise collapse first, then
        // directories, then files. Each kept row stays byte for byte, so a long
        // listing never has its meaning rewritten.
        rows.sort_by_key(|(index, _)| entry_kind_rank(&lines[*index]));
        let mut grouped: Vec<String> = evidence;
        grouped.extend(collapsed);
        grouped.extend(directories);
        grouped.extend(rows.into_iter().map(|(_, line)| line));
        grouped.extend(files);
        out = regroup_names(grouped);
    }

    let mut joined = out.join("\n");
    if had_trailing_newline {
        joined.push('\n');
    }
    joined
}

// ---------------------------------------------------------------------------
// Reading the listing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
    /// A `tree` line: a connector, an indent, and an entry name.
    Tree,
    /// A long-listing row (`ls -la`, `ls -l`) or a Windows `dir` file row.
    Long,
    /// A Windows `dir` row for a directory.
    Dir,
    /// A bare entry name, as plain `ls` and `ls -1` print.
    Name,
    /// A `tree`'s root line: a bare name above the first connector.
    Root,
    /// Anything the rule does not recognise. Emitted unchanged.
    Unparsed,
}

#[derive(Debug, Clone)]
struct Entry {
    /// The line's position in the input, so a subtree can be measured forward.
    index: usize,
    kind: Kind,
    /// The line exactly as the listing printed it.
    raw: String,
    /// The entry name, when the line is an entry.
    name: String,
    /// A `tree` line's depth; a `tree`'s root is depth 0 and has no connector.
    depth: usize,
    /// A `tree` line's guides as printed, from the start of the line through the
    /// connector, so a collapse keeps the structure an agent reads.
    guides: String,
    /// The names on a bare-name line, in printed order.
    names: Vec<String>,
    /// A row's leading column, kept so a collapse can retain the entry's kind.
    mode: String,
    /// Is a bare-name line a listing of entries, rather than a sentence that
    /// merely looks like names?
    names_are_entries: bool,
}

struct Collapse {
    index: usize,
    text: String,
}

impl Entry {
    fn unparsed(index: usize, raw: &str) -> Self {
        Self {
            index,
            kind: Kind::Unparsed,
            raw: raw.to_string(),
            name: String::new(),
            depth: 0,
            guides: String::new(),
            names: Vec::new(),
            mode: String::new(),
            names_are_entries: false,
        }
    }
}

fn classify(index: usize, line: &str, is_tree: bool) -> Entry {
    // A collapse this rule already wrote is Evidence on a second pass, so
    // rendering a rendered listing changes nothing.
    if is_collapse_marker(line) {
        return Entry::unparsed(index, line);
    }
    // A bare line with no guide above the tree's first connector names the tree's
    // root. It is Evidence, not an entry, and it must survive.
    if is_tree {
        let is_root = index == 0 && bare_names(line).len() == 1;
        if is_root {
            return Entry {
                index,
                kind: Kind::Root,
                raw: line.to_string(),
                name: line.trim().to_string(),
                depth: 0,
                guides: String::new(),
                names: bare_names(line),
                mode: String::new(),
                names_are_entries: false,
            };
        }
    }
    if let Some((depth, guides, name)) = tree_entry(line) {
        // The bare line above the first connector, with no guide of its own,
        // names the tree's root. A top-level entry — `└── target` — has a guide
        // and is an entry, not the root.
        let kind = if guides.is_empty() {
            Kind::Root
        } else {
            Kind::Tree
        };
        return Entry {
            index,
            kind,
            raw: line.to_string(),
            name: name.to_string(),
            depth,
            guides: guides.to_string(),
            names: Vec::new(),
            mode: String::new(),
            names_are_entries: false,
        };
    }
    if let Some((mode, name, is_dir)) = long_entry(line) {
        return Entry {
            index,
            kind: if is_dir { Kind::Dir } else { Kind::Long },
            raw: line.to_string(),
            name,
            depth: 0,
            guides: String::new(),
            names: Vec::new(),
            mode: format!("{mode} "),
            names_are_entries: false,
        };
    }
    let names = bare_names(line);
    if !names.is_empty() {
        // Is this line a listing of bare names, or a sentence that happens to be
        // spelled like names?
        //
        // * A single name on a line is an entry: one name per line is how `ls -1`
        //   and a one-entry `ls` read.
        // * A comma is `ls -m`'s entry separator, so a comma-separated line whose
        //   every segment is exactly one name is a listing by construction.
        //   Deciding this from the raw line, rather than from the names that
        //   survived token filtering, is what keeps `target, tests` from being
        //   read as a sentence just because `target` carries no dot or dash —
        //   while `3 directories, 4 files` and `3 File(s) 1,024 bytes` stay
        //   Evidence, because there each segment still carries a count beside its
        //   word.
        // * Several space- or tab-separated words with no name marker anywhere —
        //   no dot, dash, underscore, tilde, or `@` — are a summary, a `dir`
        //   footer, or a sentence. They stay Evidence.
        let names_are_entries = names.len() == 1
            || comma_line_of_entries(line)
            || names
                .iter()
                .any(|name| name.contains(['.', '-', '_', '~', '@']));
        if names_are_entries {
            return Entry {
                index,
                kind: Kind::Name,
                raw: line.to_string(),
                name: names[0].clone(),
                depth: 0,
                guides: String::new(),
                names,
                mode: String::new(),
                names_are_entries: true,
            };
        }
    }
    Entry::unparsed(index, line)
}

/// The listing's lines, without the trailing empty one and without the `\r` a
/// CRLF capture leaves behind, so no stray carriage return ever reaches an agent.
fn listing_lines(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = text
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line).to_string())
        .collect();
    if lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines
}

/// A `tree` line: optional ancestors, a connector, then the entry name.
///
/// A `tree` draws every level as one four-column segment: `\u{2502}   ` for an
/// ancestor that has children below it, four spaces for one that does not, and
/// `\u{251c}\u{2500}\u{2500} `/`\u{2514}\u{2500}\u{2500} ` for the entry itself. Consuming those segments
/// from the start of the line is what keeps the guides, the depth, and the name
/// exact; searching for a connector anywhere in the line confuses `\u{251c}` with
/// `\u{2514}`.
fn tree_entry(line: &str) -> Option<(usize, &str, &str)> {
    let mut at = 0;
    let mut depth = 0;
    while let Some(rest) = line.get(at..) {
        if let Some(rest) = rest
            .strip_prefix("\u{2502}   ")
            .or_else(|| rest.strip_prefix("    "))
        {
            at = line.len() - rest.len();
            depth += 1;
            continue;
        }
        let name = rest
            .strip_prefix(TREE_CHILD)
            .or_else(|| rest.strip_prefix(TREE_LAST_CHILD))?;
        at = line.len() - name.len();
        depth += 1;
        // The connector is part of the guides; the name runs to the end.
        let guides = line.get(..at)?;
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        return Some((depth, guides, name));
    }
    None
}

/// A long-listing row: `ls -l`, `ls -la`, or a Windows `dir` row.
///
/// Returns the leading column, the entry name, and whether the entry is a
/// directory.
fn long_entry(line: &str) -> Option<(String, String, bool)> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 4 {
        return None;
    }
    // `drwxr-xr-x  5 user staff  160 Jan  2 12:00 src`
    if is_ls_mode(tokens[0]) {
        let time = tokens.iter().position(|token| is_clock(token))?;
        let name = tokens.get(time + 1)?.to_string();
        return Some((tokens[0].to_string(), name, tokens[0].starts_with('d')));
    }
    // `01/02/2024  12:00 PM    <DIR>          target`
    if is_dir_date(tokens[0]) && is_clock(tokens[1]) {
        // `dir` prints `12:00 PM` on a 12-hour locale and `12:00` on a 24-hour
        // one; the name follows the size column either way.
        let after_time = if matches!(tokens.get(2), Some(&"AM" | &"PM")) {
            3
        } else {
            2
        };
        let column = tokens.get(after_time)?;
        let is_dir = *column == "<DIR>";
        if !is_dir && column.parse::<u64>().is_err() {
            return None;
        }
        let name = tokens.get(after_time + 1)?.to_string();
        return Some((column.to_string(), name, is_dir));
    }
    None
}

/// Is this token an `ls` mode column? `dir`'s `<DIR>` is deliberately not one.
fn is_ls_mode(token: &str) -> bool {
    let mut chars = token.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    matches!(first, '-' | 'd' | 'l')
        && token.len() >= 10
        && chars.all(|c| matches!(c, 'r' | 'w' | 'x' | '-' | 's' | 'S' | 't' | 'T' | '@' | '+'))
}

fn is_dir_date(token: &str) -> bool {
    token.len() == 10
        && token.as_bytes()[2] == b'/'
        && token.as_bytes()[5] == b'/'
        && token
            .bytes()
            .enumerate()
            .all(|(i, b)| i == 2 || i == 5 || b.is_ascii_digit())
}

fn is_clock(token: &str) -> bool {
    token.len() >= 4 && token.contains(':') && token.chars().all(|c| c.is_ascii_digit() || c == ':')
}

/// The bare names on a line, as plain `ls`, `ls -1`, and `ls -m` print them.
///
/// `whole_line` is set for a listing that prints no rows at all: a one-entry
/// listing is then read as its entry whatever it is named. In a row listing the
/// words of a summary or a footer are never read as entries.
fn bare_names(line: &str) -> Vec<String> {
    let tokens: Vec<&str> = line
        .split([',', '\t', ' '])
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .collect();
    tokens
        .into_iter()
        .filter(|token| possible_name(token))
        .map(str::to_string)
        .collect()
}

fn possible_name(token: &str) -> bool {
    if token.len() > WIDTH * 2 || token.contains("://") || token.contains('\\') {
        return false;
    }
    if token.chars().any(|c| c.is_whitespace() || c == ':') {
        return false;
    }
    let acceptable = token
        .chars()
        .all(|c| c.is_alphanumeric() || "-_./@+,()[]{}'!#$%^&*~=;".contains(c));
    if !acceptable {
        return false;
    }
    // A name is at least two characters and carries a letter; a single-letter
    // column, a size, or a count is not a name. Whether a line of these tokens is
    // a list of entries or a sentence is decided when the listing is rendered.
    token.chars().count() >= 2 && token.chars().any(|c| c.is_alphabetic())
}

// ---------------------------------------------------------------------------
// Trimming
// ---------------------------------------------------------------------------

fn truncate(text: &str) -> (String, bool) {
    let count = text.chars().count();
    if count <= WIDTH {
        return (text.to_string(), false);
    }
    let mut kept: String = text.chars().take(WIDTH - 1).collect();
    kept.push(ELLIPSIS);
    (kept, true)
}

/// Is this line `ls -m`'s comma-separated list of entries?
///
/// `ls -m` separates entries with `, ` and wraps after the separator, so a line
/// may end with a bare comma. A comma alone is not enough to call a line a
/// listing: `tree` summarises as `3 directories, 4 files` and `dir` footers as
/// `3 File(s) 1,024 bytes`, and both must stay Evidence. The difference is that
/// in `ls -m` every comma-separated segment is exactly one entry name, while in a
/// summary each segment still carries a count beside its word.
fn comma_line_of_entries(line: &str) -> bool {
    let body = line.trim_end().trim_end_matches(',');
    if !body.contains(',') {
        return false;
    }
    let mut names = 0;
    for segment in body.split(',') {
        let words: Vec<&str> = segment.split_whitespace().collect();
        if words.len() != 1 || !possible_name(words[0]) {
            return false;
        }
        names += 1;
    }
    names > 0
}

/// Is this the display name of a Noise directory?
///
/// The comparison is by whole name, so `target/` is Noise while `.gitignore` and
/// `target-something` are not.
pub fn is_noise_name(name: &str) -> bool {
    NOISE_DIRECTORIES.contains(&name.trim_end_matches('/'))
}

// ---------------------------------------------------------------------------
// Collapsing a `tree` subtree
// ---------------------------------------------------------------------------

struct Subtree {
    /// Every entry the subtree holds, directories and files alike.
    entry_count: usize,
    /// The index of the last line the subtree owns.
    last: usize,
}

/// The whole subtree under `entry`, as far as the `tree` text describes it.
fn tree_noise_subtree(entries: &[Entry], entry: &Entry) -> Option<Subtree> {
    if entry.kind != Kind::Tree || !is_noise_name(&entry.name) {
        return None;
    }
    let mut last = entry.index;
    let mut entry_count = 0;
    for candidate in entries.iter().filter(|e| e.index > entry.index) {
        if candidate.kind != Kind::Tree || candidate.depth <= entry.depth {
            break;
        }
        entry_count += 1;
        last = candidate.index;
    }
    Some(Subtree { entry_count, last })
}

// ---------------------------------------------------------------------------
// Rendering helpers
// ---------------------------------------------------------------------------

/// The one visible line that records a collapse, in every listing shape:
/// `<guides><name>/ (<what was elided>)`.
///
/// The line is rebuilt from the entry's own text so a `tree` keeps its
/// connectors and a `dir` row keeps its columns.
fn count_label(count: usize) -> &'static str {
    if count == 1 { "entry" } else { "entries" }
}

/// A collapse this rule already wrote, in either of its two shapes.
fn is_collapse_marker(line: &str) -> bool {
    let line = line.trim_end();
    if line.ends_with(&format!("({NOISE_CONTENTS})")) {
        return true;
    }
    let Some(inner) = line
        .strip_suffix(" collapsed)")
        .and_then(|rest| rest.rsplit_once(" (").map(|(_, inner)| inner))
    else {
        return false;
    };
    let mut words = inner.split_whitespace();
    let count = words.next().unwrap_or_default();
    count.parse::<usize>().is_ok() && matches!(words.next(), Some("entry" | "entries"))
}

/// Where a kept line belongs in a flat listing's grouping. `0` is an entry that
/// leads a listing — a directory row or the implicit `.`/`..` rows — and `1` is a
/// file row.
fn entry_kind_rank(line: &str) -> u8 {
    match long_entry(line) {
        Some((_, _, true)) => 0,
        Some((_, _, false)) => 1,
        // Anything else was not a row, so it is already in the evidence group.
        None => 0,
    }
}

/// Join an `ls -m` continuation line onto the line it continues, so the columns
/// read as one list. Entries are never dropped either way.
fn regroup_names(lines: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    for line in lines {
        let continues = out
            .last()
            .is_some_and(|prev: &String| prev.trim_end().ends_with(','));
        if continues {
            if let Some(prev) = out.last_mut() {
                let _ = write!(prev, " {}", line.trim_start());
            }
        } else {
            out.push(line);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_lines(lines: &[&str]) -> Vec<String> {
        render(&format!("{}\n", lines.join("\n")))
            .lines()
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn empty_input_is_empty_output() {
        assert_eq!(render(""), "");
        assert_eq!(render("\n"), "\n");
    }

    #[test]
    fn a_single_file_is_untouched_and_keeps_its_newline_convention() {
        assert_eq!(render("Cargo.toml\n"), "Cargo.toml\n");
        assert_eq!(render("Cargo.toml"), "Cargo.toml");
        assert_eq!(render("src\n"), "src\n");
    }

    #[test]
    fn crlf_input_does_not_emit_a_stray_carriage_return() {
        let out = render(
            "total 1\r\ndrwxr-xr-x  5 u g  160 Jan  2 12:00 src\r\n-rw-r--r--  1 u g    0 Jan  2 12:00 a.txt\r\n",
        );
        assert!(
            !out.contains('\r'),
            "a stray carriage return reached the agent: {out:?}"
        );
        assert!(out.contains("src"));
        assert!(out.contains("a.txt"));
    }

    #[test]
    fn a_crlf_noise_row_still_collapses() {
        let out = render("-rw-r--r--  1 u g 0 Jan  2 12:00 keep.txt\r\n");
        assert!(out.contains("keep.txt"));
        let out = render("drwxr-xr-x  5 u g 160 Jan  2 12:00 target\r\n");
        assert!(out.contains("target/ (contents collapsed)"), "got: {out:?}");
    }

    #[test]
    fn already_trimmed_input_is_unchanged() {
        let trimmed = render("Cargo.toml\nsrc\n");
        assert_eq!(render(&trimmed), trimmed);
        let collapsed = render(&format!("drwxr-xr-x target/ ({NOISE_CONTENTS})\nsrc\n"));
        assert_eq!(render(&collapsed), collapsed);
    }

    #[test]
    fn a_noise_directory_is_the_display_name_not_a_prefix() {
        assert!(is_noise_name("target"));
        assert!(is_noise_name("target/"));
        assert!(!is_noise_name("target-something"));
        assert!(!is_noise_name(".gitignore"));
        assert!(!is_noise_name("buildkite"));
    }

    #[test]
    fn noise_inside_a_deep_tree_is_collapsed_at_its_own_position() {
        let out = render_lines(&[
            "root",
            "├── src",
            "│   ├── main.rs",
            "│   └── target",
            "│       └── debug",
            "└── README.md",
        ]);
        assert_eq!(
            out,
            vec![
                "root",
                "├── src",
                "│   ├── main.rs",
                "│   └── target/ (1 entry collapsed)",
                "└── README.md",
            ],
            "the connector and the ancestor guide are kept, and the subtree is gone"
        );
    }

    #[test]
    fn a_nested_noise_subtree_collapses_once_at_the_outer_directory() {
        let out = render_lines(&[
            "root",
            "└── target",
            "    ├── debug",
            "    │   └── build",
            "    │       └── deep.bin",
            "    └── CACHEDIR.TAG",
        ]);
        assert_eq!(
            out,
            vec!["root", "└── target/ (4 entries collapsed)"],
            "the outer Noise directory swallows the nested one"
        );
    }

    #[test]
    fn a_comma_list_of_names_is_a_listing_but_a_counted_summary_is_not() {
        // `ls -m` separates entries with `, ` and wraps after the separator, so a
        // line may end with a bare comma. Every segment is exactly one name.
        assert!(comma_line_of_entries("alpha, beta, gamma"));
        assert!(comma_line_of_entries("alpha, beta, gamma, "));
        assert!(comma_line_of_entries("target, tests"));
        // A summary and a `dir` footer keep a count beside their word, and a count
        // is not a name: both must stay Evidence rather than becoming entries.
        assert!(!comma_line_of_entries("3 directories, 4 files"));
        assert!(!comma_line_of_entries(
            "               3 File(s)          1,024 bytes"
        ));
        // A comma used as a thousands separator is not an entry separator.
        assert!(!comma_line_of_entries("3, 4"));
        assert!(!comma_line_of_entries("no comma here"));
    }

    #[test]
    fn a_wrapped_comma_listing_collapses_its_noise_on_the_continuation_line() {
        // `ls -m` wrapped after `src, `: `target` sits alone on the next line and
        // carries no dot or dash, so a per-token reading would keep the line as a
        // sentence and leave the Noise directory uncollapsed.
        let out = render("AGENTS.md, Cargo.lock, build, src, \ntarget, tests\n");
        assert!(
            out.contains("target/ (contents collapsed)"),
            "the wrapped Noise directory was not collapsed: {out:?}"
        );
        assert!(
            out.contains("build/ (contents collapsed)"),
            "the first-line Noise directory was not collapsed: {out:?}"
        );
        for name in ["AGENTS.md", "Cargo.lock", "src", "tests"] {
            assert!(out.contains(name), "{name} was dropped from: {out:?}");
        }
    }

    #[test]
    fn the_tree_summary_and_a_root_line_are_evidence() {
        let out = render("root\n└── src\n\n3 directories, 4 files\n");
        assert!(out.contains("root"));
        assert!(out.contains("3 directories, 4 files"));
        // The summary is a sentence, not a list of entries.
        assert!(!out.contains("directories/, "));
        assert!(!out.contains("files/"));
    }

    #[test]
    fn a_long_line_is_cut_to_the_width_with_a_marker() {
        let long = "a".repeat(WIDTH + 40);
        let (out, truncated) = truncate(&long);
        assert!(truncated);
        assert_eq!(out.chars().count(), WIDTH);
        assert!(out.ends_with(ELLIPSIS));
    }

    #[test]
    fn a_line_at_the_width_is_not_touched() {
        let exact = "b".repeat(WIDTH);
        let (out, truncated) = truncate(&exact);
        assert!(!truncated);
        assert_eq!(out, exact);
    }

    #[test]
    fn truncation_counts_characters_not_bytes() {
        let name = format!("é{}", "c".repeat(WIDTH + 10));
        let (out, truncated) = truncate(&name);
        assert!(truncated);
        assert_eq!(out.chars().count(), WIDTH);
    }

    #[test]
    fn unrecognised_lines_are_evidence() {
        // A Windows `dir` volume header and byte footer, plus a Noise directory:
        // the footer carries counts an agent needs, so it must survive while only
        // the Noise row collapses.
        let out = render_lines(&[
            " Volume in drive C is OS",
            "01/02/2024  12:00 PM    <DIR>          src",
            "01/02/2024  12:00 PM    <DIR>          target",
            "               3 File(s)          1,024 bytes",
        ]);
        assert!(out.iter().any(|l| l.contains("Volume in drive C is OS")));
        assert!(
            out.iter()
                .any(|l| l.contains("3 File(s)") && l.contains("1,024 bytes")),
            "the byte footer is Evidence and must survive: {out:?}"
        );
        assert!(out.iter().any(|l| l.contains("src")));
        assert!(
            out.iter()
                .any(|l| l.contains("target/ (contents collapsed)")),
            "got: {out:?}"
        );
    }

    #[test]
    fn a_dir_row_with_no_extension_keeps_its_place() {
        let out = render_lines(&[
            "01/02/2024  12:00 PM                 0 LICENSE",
            "01/02/2024  12:00 PM                 0 <no extension>",
        ]);
        assert!(out.iter().any(|l| l.contains("LICENSE")));
        assert!(
            out.iter().any(|l| l.contains("<no extension>")),
            "a row the rule cannot read must survive: {out:?}"
        );
    }

    #[test]
    fn long_listing_rows_keep_their_columns() {
        let out = render_lines(&[
            "total 8",
            "drwxr-xr-x  5 u g 160 Jan  2 12:00 target",
            "-rw-r--r--  1 u g   0 Jan  2 12:00 a.txt",
        ]);
        assert_eq!(out[0], "total 8");
        assert!(
            out.iter()
                .any(|l| l == "-rw-r--r--  1 u g   0 Jan  2 12:00 a.txt"),
            "{out:?}"
        );
        assert!(
            out.iter()
                .any(|l| l == "drwxr-xr-x target/ (contents collapsed)"),
            "{out:?}"
        );
    }

    #[test]
    fn a_bare_name_line_splits_columns_without_losing_one() {
        // Two space-separated columns and a tab-separated one, as `ls -C` emits.
        let out = render("index.html  README.md\ttarget  Cargo.toml\n");
        for name in ["index.html", "README.md", "Cargo.toml"] {
            assert!(out.contains(name), "{name} was lost from: {out:?}");
        }
        assert!(
            out.contains("target/ (contents collapsed)"),
            "a Noise directory in a column must still collapse: {out:?}"
        );
    }

    #[test]
    fn a_column_of_plain_words_is_kept_as_it_is() {
        // With no dot, dash, underscore, or capital anywhere, a line of words is a
        // summary rather than a list of entries. Keeping it is the safe reading:
        // nothing is invented and nothing is dropped.
        let out = render("alpha  beta  gamma\n");
        assert_eq!(out, "alpha  beta  gamma\n");
    }

    #[test]
    fn a_recursive_header_line_is_evidence() {
        let out = render_lines(&["./src:", "lib.rs  main.rs", "./tests:", "smoke.rs"]);
        for line in ["./src:", "./tests:", "lib.rs", "smoke.rs"] {
            assert!(
                out.iter().any(|each| each.contains(line)),
                "{line} was dropped from: {out:?}"
            );
        }
    }
}

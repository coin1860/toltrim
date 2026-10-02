//! `git status`, `git log`, and `git branch`.
//!
//! Three rules that share a shape: git prints what an agent needs next to a lot
//! of what it does not. `status` repeats one line per file inside an untracked
//! build directory; `log` prints each commit as a six-line block with field
//! labels and a body; `branch` decorates every name with a hash, a tracking
//! status, and a subject. Each rule below keeps the Evidence and drops the
//! Noise, and preserves whether the input ended with a newline.

/// How many commits `git log` shows when the caller did not ask for a count.
/// `render_log` receives text rather than arguments, so this is a cap rather
/// than a `-n` flag: the most recent commits are the ones kept.
const LOG_DEFAULT_LIMIT: usize = 20;

/// `git status`. `text` has already had ANSI removed and is valid UTF-8.
///
/// The section headers and every changed path with its status code survive;
/// git's `(use "git …")` advice and the trailing "no changes added" line do not.
/// An untracked directory's files collapse into one line that names the
/// directory and counts what it swallowed, so a `target/` with 130 files costs
/// one line instead of 130. A single untracked file is never collapsed.
pub fn render(text: &str, _exit_code: i32) -> String {
    if text.is_empty() {
        return String::new();
    }
    let (lines, trailing_newline) = split_lines(text);
    let mut out: Vec<String> = Vec::new();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if line.trim() == "Untracked files:" {
            out.push(line.to_string());
            i += 1;
            // The section runs to the blank line that ends it. Advice lines sit
            // inside it, so they are filtered per entry rather than by stopping
            // the scan.
            let start = i;
            while i < lines.len() && !lines[i].trim().is_empty() && !is_section_header(lines[i]) {
                i += 1;
            }
            let entries: Vec<&str> = lines[start..i]
                .iter()
                .copied()
                .filter(|entry| !entry.trim().is_empty() && !is_advice(entry.trim()))
                .collect();
            collapse_untracked(&entries, &mut out);
            continue;
        }
        if is_advice(line.trim()) {
            i += 1;
            continue;
        }
        out.push(line.to_string());
        i += 1;
    }

    finish(out, trailing_newline)
}

/// `git log`. Keeps hash, author, date, and subject for the most recent
/// [`LOG_DEFAULT_LIMIT`] commits, states how many older commits were omitted,
/// and drops bodies and the `Merge:`/`Author:`/`Date:` block decoration.
pub fn render_log(text: &str, _exit_code: i32) -> String {
    if text.is_empty() {
        return String::new();
    }
    let (lines, trailing_newline) = split_lines(text);
    // The default block shape announces each commit with `commit <hash>`. Input
    // that does not (a `--oneline`-style listing) is kept defensively, one line
    // per commit, and still capped rather than corrupted.
    let commits: Vec<String> = if lines.iter().any(|line| commit_hash(line).is_some()) {
        parse_block_log(&lines)
    } else {
        lines
            .iter()
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.trim_end().to_string())
            .collect()
    };

    let mut out: Vec<String> = commits.iter().take(LOG_DEFAULT_LIMIT).cloned().collect();
    let omitted = commits.len().saturating_sub(LOG_DEFAULT_LIMIT);
    if omitted > 0 {
        out.push(format!(
            "... {omitted} older commit{} omitted",
            if omitted == 1 { "" } else { "s" }
        ));
    }

    finish(out, trailing_newline)
}

/// `git branch`. Names only, in the order git printed them, with git's own
/// `* ` marker left on the current branch. Tracking decoration (`[ahead 1]`,
/// `[behind 2]`), the verbose hash and subject, and any indentation beyond the
/// marker are dropped.
pub fn render_branch(text: &str, _exit_code: i32) -> String {
    if text.is_empty() {
        return String::new();
    }
    let (lines, trailing_newline) = split_lines(text);
    let mut out: Vec<String> = Vec::new();
    for line in lines {
        let content = line.trim();
        if content.is_empty() {
            continue;
        }
        let current = content.starts_with("* ");
        // `+ ` marks a branch checked out in another worktree; it is not the
        // current branch, so the marker is not carried over.
        let rest = content
            .strip_prefix("* ")
            .or_else(|| content.strip_prefix("+ "))
            .unwrap_or(content);
        let name = if rest.starts_with('(') {
            // `(HEAD detached at abc1234)` is a whole answer, not decoration.
            rest.trim_end().to_string()
        } else {
            match rest.split_whitespace().next() {
                Some(name) => name.to_string(),
                None => continue,
            }
        };
        out.push(format!("{} {name}", if current { "*" } else { " " }));
    }
    finish(out, trailing_newline)
}

/// One collapsed line per untracked directory, at the position of that
/// directory's first file. A directory with a single file is left as the path
/// git printed, because collapsing it would hide the only name for no gain.
fn collapse_untracked(entries: &[&str], out: &mut Vec<String>) {
    struct Group<'a> {
        dir: &'a str,
        files: usize,
        emitted: bool,
    }

    let mut groups: Vec<Group<'_>> = Vec::new();
    for entry in entries {
        let Some(dir) = directory_key(entry) else {
            continue;
        };
        match groups.iter_mut().find(|group| group.dir == dir) {
            Some(group) => group.files += 1,
            None => groups.push(Group {
                dir,
                files: 1,
                emitted: false,
            }),
        }
    }

    for entry in entries {
        if let Some(dir) = directory_key(entry)
            && let Some(group) = groups.iter_mut().find(|group| group.dir == dir)
            && group.files > 1
        {
            if group.emitted {
                continue;
            }
            group.emitted = true;
            out.push(format!("\t{}/ ({} files)", group.dir, group.files));
            continue;
        }
        out.push((*entry).to_string());
    }
}

/// The top-level directory an untracked file lives under, when git printed a
/// path rather than a directory. `target/classes/Foo.class` yields `target`;
/// `solo.txt` yields nothing (it is a plain file); an entry that already ends in
/// `/` is a directory git collapsed itself, and a quoted path is left alone.
fn directory_key(entry: &str) -> Option<&str> {
    let path = entry.trim();
    if path.is_empty() || path.starts_with('"') || path.ends_with('/') {
        return None;
    }
    match path.split_once('/') {
        Some((dir, _)) if !dir.is_empty() => Some(dir),
        _ => None,
    }
}

/// git's own section headings, in the human-readable format.
fn is_section_header(line: &str) -> bool {
    matches!(
        line.trim(),
        "Changes to be committed:"
            | "Changes not staged for commit:"
            | "Untracked files:"
            | "Unmerged paths:"
            | "Ignored files:"
    )
}

/// Advice that names no project detail: the `(use "git …")` hints inside each
/// section and the trailing "nothing added to commit" line. The clean-tree line
/// (`nothing to commit, working tree clean`) is an answer, not advice, and is
/// deliberately not matched.
fn is_advice(trimmed: &str) -> bool {
    trimmed.starts_with("(use \"")
        || trimmed.starts_with("(commit or discard")
        || trimmed.starts_with("no changes added to commit")
        || trimmed.starts_with("nothing added to commit")
}

/// One parsed commit in the default block shape.
struct BlockCommit {
    hash: String,
    author: Option<String>,
    date: Option<String>,
    subject: Option<String>,
}

impl BlockCommit {
    fn into_line(self) -> String {
        let mut head = self.hash;
        if let Some(date) = self.date {
            head.push(' ');
            head.push_str(&date);
        }
        if let Some(author) = self.author {
            head.push(' ');
            head.push_str(&author);
        }
        match self.subject {
            Some(subject) => format!("{head}  {subject}"),
            None => head,
        }
    }
}

/// The hash on a `commit <hash> …` line, ignoring decorations such as
/// `(HEAD -> main, tag: v1.0)`.
fn commit_hash(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("commit ")?;
    let hash = rest.split_whitespace().next()?;
    if hash.len() >= 4 && hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        Some(hash)
    } else {
        None
    }
}

/// Reduce the block form to one line per commit. Every `commit` line starts a
/// commit, so bodies and `Merge:` lines belong to a commit and are never emitted
/// on their own.
fn parse_block_log(lines: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current: Option<BlockCommit> = None;

    for line in lines {
        if let Some(hash) = commit_hash(line) {
            if let Some(commit) = current.take() {
                out.push(commit.into_line());
            }
            current = Some(BlockCommit {
                hash: hash.to_string(),
                author: None,
                date: None,
                subject: None,
            });
            continue;
        }
        let Some(commit) = current.as_mut() else {
            continue;
        };
        let trimmed = line.trim();
        if commit.author.is_none()
            && let Some(rest) = trimmed.strip_prefix("Author:")
        {
            commit.author = Some(author_name(rest));
        }
        if commit.date.is_none()
            && let Some(rest) = trimmed.strip_prefix("Date:")
        {
            commit.date = Some(short_date(rest));
        }
        // The message is the first indented line; metadata lines sit at column
        // zero, and the body only starts after a blank line, by which point the
        // subject is already taken.
        if commit.subject.is_none()
            && !trimmed.is_empty()
            && (line.starts_with(' ') || line.starts_with('\t'))
        {
            commit.subject = Some(trimmed.to_string());
        }
    }
    if let Some(commit) = current.take() {
        out.push(commit.into_line());
    }
    out
}

/// `Alice Example <alice@example.com>` keeps the name; the address is not what
/// an agent picks a commit by.
fn author_name(rest: &str) -> String {
    let rest = rest.trim();
    match rest.split_once('<') {
        Some((name, _)) => name.trim().to_string(),
        None => rest.to_string(),
    }
}

/// git's default date to `YYYY-MM-DD`, so a log line is stable and sortable. A
/// date git already printed as `YYYY-MM-DD…` is used as-is; anything else is
/// handled from its month name, which is the one field both the default and the
/// RFC 2822 shapes have.
fn short_date(rest: &str) -> String {
    let rest = rest.trim();
    if is_iso_date(rest) {
        return rest[..10].to_string();
    }
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    let month = tokens
        .iter()
        .position(|token| month_number(token).is_some());
    let year = tokens
        .iter()
        .find(|token| token.len() == 4 && token.bytes().all(|b| b.is_ascii_digit()));
    let day = month.and_then(|index| {
        // The day sits next to the month: `Mar 20` in git's default shape,
        // `2 Jan` in the RFC shape.
        index
            .checked_sub(1)
            .and_then(|before| day_number(tokens[before]))
            .or_else(|| tokens.get(index + 1).and_then(|after| day_number(after)))
    });
    match (year, month, day) {
        (Some(year), Some(month), Some(day)) => {
            format!(
                "{year}-{:02}-{day:02}",
                month_number(tokens[month]).unwrap()
            )
        }
        // An unrecognised date is Evidence in its original form.
        _ => rest.to_string(),
    }
}

fn is_iso_date(rest: &str) -> bool {
    let bytes = rest.as_bytes();
    bytes.len() >= 10
        && bytes[..10].iter().enumerate().all(|(i, byte)| match i {
            4 | 7 => *byte == b'-',
            _ => byte.is_ascii_digit(),
        })
}

fn day_number(token: &str) -> Option<u32> {
    let token = token.trim_matches(',');
    if !token.is_empty() && token.len() <= 2 && token.bytes().all(|b| b.is_ascii_digit()) {
        token.parse().ok()
    } else {
        None
    }
}

fn month_number(token: &str) -> Option<u32> {
    let name = token.trim_matches(',').to_ascii_lowercase();
    Some(match name.as_str() {
        "jan" => 1,
        "feb" => 2,
        "mar" => 3,
        "apr" => 4,
        "may" => 5,
        "jun" => 6,
        "jul" => 7,
        "aug" => 8,
        "sep" => 9,
        "oct" => 10,
        "nov" => 11,
        "dec" => 12,
        _ => return None,
    })
}

/// Split into lines, remembering whether the input ended with a newline so the
/// output can keep that convention. A trailing `\r` is dropped, so CRLF output
/// from git on Windows reads as clean lines.
fn split_lines(text: &str) -> (Vec<&str>, bool) {
    let trailing_newline = text.ends_with('\n');
    let mut lines: Vec<&str> = text
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect();
    if trailing_newline {
        lines.pop();
    }
    (lines, trailing_newline)
}

/// Join trimmed lines, restoring the newline the input ended with. Blank lines
/// that only separated a section from advice this rule removed are dropped, so
/// the trimmed output does not end in empty lines.
fn finish(mut lines: Vec<String>, trailing_newline: bool) -> String {
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    if lines.is_empty() {
        return String::new();
    }
    let mut out = lines.join("\n");
    if trailing_newline {
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_stays_empty_for_every_rule() {
        assert_eq!(render("", 0), "");
        assert_eq!(render_log("", 0), "");
        assert_eq!(render_branch("", 0), "");
    }

    #[test]
    fn a_status_without_an_untracked_section_keeps_its_paths() {
        let input = "On branch main\n\
                     Changes not staged for commit:\n\
                     \x20 (use \"git add <file>...\" to update what will be committed)\n\
                     \tmodified:   src/main.rs\n\
                     \n\
                     no changes added to commit (use \"git add\" and/or \"git commit -a\")\n";
        let out = render(input, 0);
        assert!(out.contains("On branch main"));
        assert!(out.contains("Changes not staged for commit:"));
        assert!(out.contains("modified:   src/main.rs"));
        assert!(!out.contains("(use \"git"));
        assert!(!out.contains("no changes added to commit"));
    }

    #[test]
    fn a_clean_working_tree_is_an_answer_not_advice() {
        let input = "On branch main\nnothing to commit, working tree clean\n";
        assert_eq!(render(input, 0), input);
    }

    #[test]
    fn crlf_input_is_collapsed_and_reads_as_clean_lines() {
        let input = "On branch main\r\n\
                     Untracked files:\r\n\
                     \x20 (use \"git add <file>...\" to include in what will be committed)\r\n\
                     \ttarget/classes/Foo.class\r\n\
                     \ttarget/classes/Bar.class\r\n\
                     \tsolo.txt\r\n";
        let out = render(input, 0);
        assert!(out.contains("target/ (2 files)"), "got:\n{out}");
        assert!(out.contains("solo.txt"), "got:\n{out}");
        assert!(!out.contains('\r'), "CRLF leaked into the trimmed output");
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn an_already_collapsed_untracked_directory_is_left_alone() {
        // Plain `git status` prints `target/` without a per-file listing; there
        // is no count to add, and nothing to remove.
        let input = "On branch main\n\
                     Untracked files:\n\
                     \x20 (use \"git add <file>...\" to include in what will be committed)\n\
                     \ttarget/\n\
                     \tsolo.txt\n";
        let out = render(input, 0);
        assert!(out.contains("\ttarget/\n"), "got:\n{out}");
        assert!(out.contains("\tsolo.txt\n"), "got:\n{out}");
    }

    #[test]
    fn a_single_file_inside_a_directory_is_not_counted() {
        let input = "On branch main\n\
                     Untracked files:\n\
                     \tbuild/only.txt\n";
        let out = render(input, 0);
        assert!(out.contains("build/only.txt"), "got:\n{out}");
        assert!(!out.contains("files)"), "got:\n{out}");
    }

    #[test]
    fn a_log_with_exactly_the_limit_states_no_omission() {
        let mut input = String::new();
        for i in 0..LOG_DEFAULT_LIMIT {
            input.push_str(&format!(
                "commit {i:040x}\nAuthor: Alice Example <a@example.com>\n\
                 Date:   Tue Jan 2 09:15:00 2024 +0000\n\n    Subject {i}\n\n"
            ));
        }
        let out = render_log(&input, 0);
        assert!(out.contains("Subject 0"));
        assert!(out.contains("Subject 19"));
        assert!(!out.contains("omitted"), "got:\n{out}");
    }

    #[test]
    fn a_oneline_log_is_kept_defensively() {
        let input = "abc1234 Fix the parser\n\
                     def5678 Add the reader\n\
                     0123456 Drop the writer\n";
        assert_eq!(render_log(input, 0), input);
    }

    #[test]
    fn dates_are_reduced_to_a_stable_day() {
        assert_eq!(short_date("Wed Mar 20 11:05:00 2024 +0000"), "2024-03-20");
        assert_eq!(short_date("Tue Jan 2 09:15:00 2024 +0000"), "2024-01-02");
        assert_eq!(short_date("2024-02-14 17:45:00 +0000"), "2024-02-14");
        assert_eq!(short_date("Wed, 14 Feb 2024 17:45:00 +0000"), "2024-02-14");
        // Unknown shapes stay as printed rather than becoming a wrong day.
        assert_eq!(short_date("1700000000 +0000"), "1700000000 +0000");
    }

    #[test]
    fn a_log_without_a_trailing_newline_keeps_that_convention() {
        let input = "commit abc1234def\nAuthor: Alice Example <a@example.com>\n\
                     Date:   Tue Jan 2 09:15:00 2024 +0000\n\n    Subject\n";
        assert!(render_log(input, 0).ends_with('\n'));
        assert!(!render_log(input.trim_end_matches('\n'), 0).ends_with('\n'));
    }

    #[test]
    fn a_branch_listing_without_markers_keeps_every_name() {
        let input = "main\nfeature/parser\nrelease-1.0\n";
        let out = render_branch(input, 0);
        assert!(out.contains("main"));
        assert!(out.contains("feature/parser"));
        assert!(out.contains("release-1.0"));
    }

    #[test]
    fn a_detached_head_keeps_its_whole_answer() {
        let input = "* (HEAD detached at abc1234)\n  main\n";
        let out = render_branch(input, 0);
        assert!(out.contains("* (HEAD detached at abc1234)"), "got:\n{out}");
    }

    #[test]
    fn trimming_advice_does_not_leave_a_trailing_blank_line() {
        let input = "On branch main\n\
                     Untracked files:\n\
                     \tbuild/a.txt\n\
                     \tbuild/b.txt\n\
                     \n\
                     nothing added to commit but untracked files present (use \"git add\" to track)\n";
        let out = render(input, 0);
        assert!(out.contains("build/ (2 files)"), "got:\n{out}");
        assert_eq!(
            out,
            "On branch main\nUntracked files:\n\tbuild/ (2 files)\n"
        );
    }

    #[test]
    fn trailing_newline_convention_is_preserved() {
        let status = "On branch main\n\tmodified:   a.txt\n";
        assert!(render(status, 0).ends_with('\n'));
        assert!(!render(status.trim_end_matches('\n'), 0).ends_with('\n'));
        let branch = "* main\n";
        assert!(render_branch(branch, 0).ends_with('\n'));
        assert!(!render_branch("* main", 0).ends_with('\n'));
    }
}

//! Maven output trimming.
//!
//! Two paths, and they are not symmetric. A failed build keeps every failure
//! line plus its surrounding context, because the line that explains a failure
//! often carries no marker at all. A successful build keeps one summary line per
//! module, because an agent needs to know the work happened and how much of it,
//! not which plugins ran.

use crate::ansi;

/// Lines that announce a failure.
const MARKERS: &[&str] = &[
    "[ERROR]",
    "[FATAL]",
    "Caused by:",
    "<<< FAILURE!",
    "<<< ERROR!",
    "BUILD FAILURE",
];

/// Advice Maven prints after every failure. It never mentions the project, so it
/// is never evidence.
const BOILERPLATE: &[&str] = &[
    "To see the full stack trace of the errors, re-run Maven with the -e switch",
    "Re-run Maven using the -X switch to enable full debug logging",
    "For more information about the errors and possible solutions, please read the following articles",
    "-> [Help 1]",
    "[Help 1] http",
];

/// Context kept on each side of a failure line.
const CONTEXT: usize = 6;
/// How far a kept line may be extended towards its block boundaries.
const MAX_BLOCK_EXTENSION: usize = 60;
/// Under-trimming Floor: below this share of the original, emit everything.
const FLOOR_PERCENT: usize = 2;
/// How much "everything" may be when the Floor applies.
const FLOOR_MAX_LINES: usize = 400;

/// Trim one Maven run. `exit_code` is the child's, verbatim.
pub fn trim(raw: &str, exit_code: i32) -> String {
    let text = ansi::strip(raw);
    let lines: Vec<&str> = text.lines().map(|l| l.trim_end_matches('\r')).collect();
    let succeeded = exit_code == 0 && lines.iter().any(|l| l.contains("BUILD SUCCESS"));
    let body = if succeeded {
        success_summary(&lines)
    } else {
        failure_extract(&text, &lines)
    };
    if body.is_empty() { body } else { format!("{body}\n") }
}

fn success_summary(lines: &[&str]) -> String {
    let spans = module_spans(lines);
    let total_time = lines
        .iter()
        .rev()
        .find_map(|line| after(line, "Total time:"))
        .map(str::trim);

    if spans.is_empty() {
        let tests = lines.iter().rev().find(|l| is_module_totals(l)).copied();
        let warnings = lines.iter().filter(|l| is_warning(l)).count();
        return summary_line(None, tests, total_time, warnings);
    }

    spans
        .iter()
        .map(|(name, start, end)| {
            let tests = lines[*start..*end]
                .iter()
                .rev()
                .find(|l| is_module_totals(l))
                .copied();
            let warnings = lines[*start..*end].iter().filter(|l| is_warning(l)).count();
            summary_line(Some(name), tests, total_time, warnings)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn summary_line(
    module: Option<&str>,
    tests: Option<&str>,
    total_time: Option<&str>,
    warnings: usize,
) -> String {
    let mut out = String::new();
    if let Some(module) = module {
        out.push_str(module);
        out.push_str(": ");
    }
    out.push_str("BUILD SUCCESS");
    if let Some(tests) = tests {
        out.push_str(" | ");
        out.push_str(drop_prefix(tests));
    }
    if let Some(time) = total_time {
        out.push_str(" | ");
        out.push_str(time);
    }
    out.push_str(&format!(" | W:{warnings}"));
    out
}

/// Module spans as `(artifact_id, first_line, past_last_line)`.
///
/// A module header looks like `[INFO] -----< demo:demo >-----`. Treating a
/// module as a span rather than a single line is what lets one module's test
/// totals be told apart from the next module's.
fn module_spans<'a>(lines: &[&'a str]) -> Vec<(&'a str, usize, usize)> {
    let mut headers: Vec<(&str, usize)> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if let Some(name) = module_name(line) {
            headers.push((name, i));
        }
    }
    headers
        .iter()
        .enumerate()
        .map(|(n, (name, start))| {
            let end = headers.get(n + 1).map(|(_, i)| *i).unwrap_or(lines.len());
            (*name, *start, end)
        })
        .collect()
}

fn module_name(line: &str) -> Option<&str> {
    let start = line.find("< ")? + 2;
    let rest = &line[start..];
    let end = rest.find(" >")?;
    let inner = rest[..end].trim();
    if inner.is_empty() || inner.contains(' ') {
        return None;
    }
    inner.rsplit(':').next()
}

fn is_module_totals(line: &str) -> bool {
    line.contains("Tests run:") && !line.contains(" -- in ")
}

fn is_warning(line: &str) -> bool {
    line.trim_start().starts_with("[WARNING]")
}

fn after<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    line.find(key).map(|i| &line[i + key.len()..])
}

fn drop_prefix(line: &str) -> &str {
    line.trim_start()
        .strip_prefix("[ERROR]")
        .or_else(|| line.trim_start().strip_prefix("[INFO]"))
        .or_else(|| line.trim_start().strip_prefix("[WARNING]"))
        .map(str::trim)
        .unwrap_or_else(|| line.trim())
}

/// A hole in the middle of a stack trace is worse than a slightly longer stack
/// trace, so a kept line is extended to the boundaries of its block. The stop
/// conditions matter more than the extension: without them the first marker's
/// window would walk back through the download block and drag the whole run in.
fn expand_to_block(lines: &[&str], keep: &mut [bool]) {
    let mut i = 0;
    while i < lines.len() {
        if !keep[i] {
            i += 1;
            continue;
        }
        let mut start = i;
        let mut steps = 0;
        while start > 0 && steps < MAX_BLOCK_EXTENSION && is_block_body(lines[start - 1]) {
            start -= 1;
            steps += 1;
        }
        let mut end = i;
        let mut steps = 0;
        while end + 1 < lines.len() && steps < MAX_BLOCK_EXTENSION && is_block_body(lines[end + 1]) {
            end += 1;
            steps += 1;
        }
        for slot in keep.iter_mut().take(end + 1).skip(start) {
            *slot = true;
        }
        i = end + 1;
    }
}

fn is_block_body(line: &str) -> bool {
    !line.trim().is_empty()
        && line.trim() != "[INFO]"
        && !is_transfer_noise(line)
        && !is_boilerplate(line)
}

fn failure_extract(text: &str, lines: &[&str]) -> String {
    let mut keep = vec![false; lines.len()];
    for (i, line) in lines.iter().enumerate() {
        if is_marker(line) {
            let from = i.saturating_sub(CONTEXT);
            let to = (i + CONTEXT + 1).min(lines.len());
            for slot in keep.iter_mut().take(to).skip(from) {
                *slot = true;
            }
        }
    }
    expand_to_block(lines, &mut keep);

    let kept: Vec<&str> = lines
        .iter()
        .enumerate()
        .filter(|(i, line)| keep[*i] && !is_boilerplate(line) && !is_transfer_noise(line))
        .map(|(_, line)| *line)
        .collect();

    if below_floor(&kept, text.len()) {
        return lines
            .iter()
            .take(FLOOR_MAX_LINES)
            .copied()
            .collect::<Vec<_>>()
            .join("\n");
    }
    kept.join("\n")
}

fn is_boilerplate(line: &str) -> bool {
    BOILERPLATE.iter().any(|advice| line.contains(advice))
}

fn is_marker(line: &str) -> bool {
    MARKERS.iter().any(|marker| line.contains(marker))
}

/// Transfer progress can land inside a context window, but it can never explain
/// a failure: the reason is always stated in a marker line nearby.
fn is_transfer_noise(line: &str) -> bool {
    let trimmed = line.trim_start();
    line.contains("Progress (")
        || trimmed.starts_with("Downloaded from ")
        || trimmed.starts_with("Downloading from ")
}

/// Integer arithmetic on purpose: the Floor must be exactly reproducible.
fn below_floor(kept: &[&str], original_bytes: usize) -> bool {
    if original_bytes == 0 {
        return false;
    }
    let kept_bytes: usize = kept.iter().map(|line| line.len() + 1).sum();
    kept_bytes * 100 < original_bytes * FLOOR_PERCENT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_marker_free_failure_keeps_the_whole_output() {
        // No line carries a marker, so nothing would survive trimming: the Floor
        // has to answer with the original text rather than with silence.
        let raw = "compiling\nsomething went wrong\n";
        let out = trim(raw, 1);
        assert!(out.contains("something went wrong"));
    }

    #[test]
    fn success_without_test_totals_never_claims_tests_passed() {
        let raw = "[INFO] Building demo 1\n[INFO] BUILD SUCCESS\n[INFO] Total time:  1.0 s\n";
        let out = trim(raw, 0);
        assert!(out.contains("BUILD SUCCESS"));
        assert!(!out.contains("Tests run"));
    }

    #[test]
    fn maven_advice_is_dropped() {
        let raw = "[ERROR] Bad thing happened\n[ERROR] -> [Help 1]\n[ERROR] Re-run Maven using the -X switch to enable full debug logging\n";
        let out = trim(raw, 1);
        assert!(out.contains("Bad thing happened"));
        assert!(!out.contains("Help 1"));
        assert!(!out.contains("-X switch"));
    }
}

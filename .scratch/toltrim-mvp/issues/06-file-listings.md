# 06: `ls`, `dir`, and `tree` collapse noise directories

**What to build:** Listing a project root costs a handful of lines instead of hundreds:
build and tool directories collapse to a count, entries group by kind, and long lines are
truncated. On Windows `dir` works even though it is a shell builtin rather than an
executable.

**Blocked by:** 01 (pure render seam).

**Status:** ready-for-agent

- [ ] `ls`, `dir`, and `tree` share one rule set; the same directory produces comparable
      output through all three.
- [ ] Noise directories (`.git`, `target`, `node_modules`, `build`, `.idea`) collapse to a
      count, and the collapse is visible rather than silent.
- [ ] Long lines are truncated at a fixed width.
- [ ] On Windows, `toltrim dir` produces trimmed output via the `cmd /c` fallback, and
      arguments reach it as an argument vector rather than a concatenated command string.
- [ ] A recorded fixture of a project root asserts both directions: the file count
      survives, the per-file listing inside the noise directory does not.
- [ ] A trailing slash, hidden entries, and a `-la`-style long listing are handled without
      changing the meaning of the listing.

## Comments

**Refinement to the collapse criterion (found while smoke-testing the packaged binary).** A
plain `ls` already prints a Noise directory as one line, so the second checkbox's marker
collapsed nothing and cost twenty-one bytes: measured `53 -> 119` bytes for a project root
with `build`, `node_modules` and `target`. The rule was therefore made shape-aware — a
Noise directory collapses where its contents or columns would otherwise be printed, which
is a `tree` (a subtree) and a row listing (`ls -l`, `ls -la`, `dir`, whose size and date
columns are dropped). A flat name listing (`ls`, `ls -1`, `ls -m`) keeps the name. A byte
floor backs the whole rule set: if the listing would end up longer than the one it read,
the original is emitted, so no listing shape can grow its input.

The criterion's substance is unaffected: the collapse is still visible rather than silent,
still counted in a `tree`, and `long lines are truncated at a fixed width` now applies to
the flat fixtures too (previously the markers outweighed the truncation and the whole
listing was reverted). The recorded `ls`, `ls -1` and `ls -m` fixtures therefore shrink by
less, which is why the published Savings Rate moved from 89.3% to 89.5% — `listing` now
records 179 Estimated Tokens saved over 9 Invocations rather than 115.

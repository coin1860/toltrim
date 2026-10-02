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

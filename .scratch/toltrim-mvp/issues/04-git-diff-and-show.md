# 04: `git diff` and `git show` stay complete but stop costing metadata

**What to build:** A reviewer agent looking at a change sees the patch and nothing else.
Context drops from three lines to one, and the per-file metadata that never describes the
change disappears — while every changed file and every hunk survives. This is the command
where trimming must never hide a change.

**Blocked by:** 01 (pure render seam).

**Status:** ready-for-agent

- [ ] Diff context is reduced from three lines to one.
- [ ] `index` lines, `old mode`/`new mode`, similarity percentages, rename metadata, and
      the "no newline at end of file" marker are gone.
- [ ] Every changed file and every hunk header is present; there is no per-file
      truncation and no dropped hunk.
- [ ] A recorded fixture containing a rename, a mode change, and a missing trailing
      newline asserts both directions: the changed lines survive, the metadata does not.
- [ ] `git diff` with differences still exits 1, and `git show` still exits 0 — exit codes
      are never reinterpreted.
- [ ] A Machine-Readable Request (`--stat`, `--name-only`, `--name-status`, `--porcelain`)
      is passed through with ANSI stripped and nothing else removed.

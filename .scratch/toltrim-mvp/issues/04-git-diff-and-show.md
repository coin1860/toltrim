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

## Comments

**Factual correction to the exit-code criterion (found while implementing).** The fifth
checkbox says "`git diff` with differences still exits 1". That is folklore: on git 2.53,
plain `git diff` exits **0** when there are differences — it is `git diff --exit-code`
(and `--quiet`) that exits 1. Measured directly during implementation.

The criterion's substance is unaffected and is what was implemented and tested: the rule
never reads the exit code, and `render(text, 1)` and `render(text, 0)` produce identical
output, so "there are changes" can never be rewritten into "the command failed". ADR-0003
has been annotated with the same correction.

Recorded fixtures (throwaway repo, commands recorded as comments in `tests/git_diff.rs`):
`git-diff-metadata.txt` from `git diff -M --cached` (rename + mode change + a file with no
trailing newline), and `git-show-commit.txt` from `git show -M HEAD`.

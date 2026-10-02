# 05: `git status`, `git log`, and `git branch` stop flooding the context

**What to build:** A repository with a large untracked build directory stops costing
hundreds of lines to inspect: an untracked directory collapses to one line with a count.
History reads as hash, author, date, and subject. Branch listing is names only.

**Blocked by:** 01 (pure render seam).

**Status:** ready-for-agent

- [ ] An untracked directory's files collapse into a single line carrying the directory
      and the number of files, and the collapsed form is distinguishable from a
      single-file entry.
- [ ] Staged, unstaged, and untracked sections remain distinguishable after trimming.
- [ ] `git log` keeps hash, author, date, and subject, and defaults to the most recent
      twenty commits.
- [ ] `git branch` lists names only and marks the current branch.
- [ ] `git status --porcelain` is passed through with ANSI stripped and nothing removed.
- [ ] A recorded fixture of a large untracked directory asserts both directions: the
      section header survives, the per-file listing does not.

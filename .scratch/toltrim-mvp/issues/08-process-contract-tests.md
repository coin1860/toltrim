# 08: The process contract is verified against the real binary

**What to build:** The four promises that a pure function cannot observe are protected by
tests that run the built binary with a stand-in command: exit codes arrive unchanged,
an unproxied command's output is byte-identical, a command with no terminal on stdin
fails fast instead of hanging, and a command that cannot be started is reported as such.

**Blocked by:** 01 (pure render seam) — the entry point must be settled first, because
these tests exercise it.

**Status:** ready-for-agent

- [ ] A command that succeeds but exits non-zero (the `git diff` shape) reports that
      exact code.
- [ ] An unknown command's output is byte-identical to running it without toltrim,
      including a non-zero exit code.
- [ ] With no terminal on stdin, an interactive command fails fast rather than blocking;
      the test completes well inside a short timeout.
- [ ] A command that does not exist exits 127 with a single explanatory line on stderr and
      nothing on stdout.
- [ ] On Unix, a command killed by signal `n` reports `128 + n`.
- [ ] stderr is forwarded verbatim for a Proxied Command, and trimming never removes it.

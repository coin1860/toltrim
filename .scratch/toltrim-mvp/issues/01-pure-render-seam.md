# 01: Make the pure render seam the only way trimming is reached

**What to build:** One pure function becomes the sole boundary for every Trim rule: give
it the tool, the arguments, the raw output bytes, the exit code, and whether this
Invocation may be trimmed at all, and it returns the bytes an agent will read. The entry
point keeps only what needs a process — parsing arguments, starting the command, writing
the streams, forwarding the exit code. No behaviour changes.

**Blocked by:** None (can start immediately).

**Status:** ready-for-agent

- [ ] Maven success, Maven failure, and unknown-command passthrough behave byte for byte
      as they do today (the existing 13 tests stay green, re-pointed at the new seam).
- [ ] The whole preservation decision — `--raw`, a Machine-Readable Request, and the
      human-pipe case — is testable at the seam without starting a process.
- [ ] The entry point contains no tool-specific branch and no rule of its own; adding a
      Tool means adding a rule set, not editing the entry point.
- [ ] Output that is not valid UTF-8 still reaches the caller unchanged.

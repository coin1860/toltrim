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

## Comments

**The seam must consult `is_proxied`, not only `preserve` (found while smoke-testing the
packaged binary).** The first cut gave `render` two ways to skip the rules: the tool is
`Unknown`, or the Invocation is preserved. It never asked whether a rule exists for this
Invocation, so a tool with a rule set had its rules applied to every subcommand of that
tool — `toltrim mvn deploy` and `toltrim mvn dependency:tree` came back as a Maven Success
Summary or Failure Extract although ADR-0008 excludes them. The Gain Record disagreed with
the bytes: `classify` (which does consult `is_proxied`) recorded `passthrough` while the
output a caller received had been rewritten.

`render` now returns the bytes unchanged when `!is_proxied(tool, args)`, ANSI included,
because a Passthrough Command has no rule set to preserve it from. That also settles the
`git add`/`commit`/`checkout` family: their output is byte-identical however the caller
invoked toltrim, which is what the README already promised.

# 03: `toltrim gain` accounts for every Invocation

**What to build:** Every Invocation appends one Gain Record, and `toltrim gain` prints a
summary — commands proxied, split into trimmed and passthrough, Estimated Tokens in, out,
and saved, and the Savings Rate — followed by a per-command table. The view can be
filtered to the current project and cleared explicitly. This is the screen an audience
sees first, so it is the first feature slice after the seam.

**Blocked by:** 01 (pure render seam) — the record needs the mode and the output size,
which only the seam knows.

**Status:** ready-for-agent

- [ ] The `gain_record` schema carries timestamp, tool, subcommand, mode, exit code, bytes
      in and out, Estimated Tokens in and out, rule version, and working directory.
- [ ] After one trimmed Invocation, the summary reports one proxied command with tokens
      in, tokens out, tokens saved, and a Savings Rate.
- [ ] A Passthrough Command is recorded, counted separately, and excluded from the
      Savings Rate.
- [ ] The per-command table reports runs, tokens in, tokens out, and savings per command.
- [ ] Filtering to the current project counts only records from that working directory.
- [ ] Records persist in one user-level database, are kept indefinitely, and are removed
      only by an explicit clear.
- [ ] Estimated Tokens are labelled as an estimate; no figure is presented as billed usage.
- [ ] If the database cannot be written, the proxied command's output and exit code are
      bit-for-bit unaffected.
- [ ] The one-line summary an agent reads is unchanged by the presence of statistics —
      no accounting line is ever injected into a proxied command's output.

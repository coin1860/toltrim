# 02: `toltrim init` teaches the agent the prefix from the repository

**What to build:** `toltrim init` writes a command mapping into the project's `AGENTS.md`
so an agent working in that repository knows to run `toltrim mvn test` instead of
`mvn test`. It is idempotent, previewable, and never touches global agent configuration.

**Blocked by:** None (can start immediately).

**Status:** ready-for-agent

- [ ] In a project with no `AGENTS.md`, running init creates one containing the mapping
      for the Proxied Commands in scope.
- [ ] In a project that already has `AGENTS.md`, init appends the mapping without
      disturbing existing content.
- [ ] Running init twice leaves exactly one mapping block — no duplication, no reordering
      of surrounding content.
- [ ] A preview mode prints what would be written and changes nothing on disk.
- [ ] No file outside the project is written; global agent configuration is untouched.
- [ ] Failures (unwritable directory, unreadable file) exit non-zero with a single
      explanatory line rather than a partial write.

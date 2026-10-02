# Invocation is an explicit prefix: `toltrim <command> <args…>`

The tool is invoked by prefixing the command, instead of shadowing `git`/`mvn` on `PATH`, installing a shell alias, or rewriting an agent's tool calls through a pre-execution hook. Explicit invocation needs no cooperation from any agent runtime, never changes what `git` or `mvn` mean to `PATH`, and degrades safely: an unrecognized command is executed unchanged.

**Considered options**: `PATH` shadowing (better transparency, but one bad shim breaks a user's `git`); agent hooks (what rtk does — transparency then depends on hook support and on the hook being permitted to run); shell function or alias (agent-spawned non-interactive shells rarely read them).

**Consequences**: something must teach the agent to type the prefix. That is `toltrim init`, which writes a command mapping into the project's `AGENTS.md` and touches no global configuration.

# toltrim MVP — a trimming proxy for Java coding agents

Status: ready-for-agent

Source: `/grill-with-docs` session. All vocabulary below is from `GLOSSARY.md`; all
decisions trace to `docs/adr/0001`–`0009`. Contradictions with those ADRs are called
out inline rather than silently overridden.

## Problem Statement

A coding agent that works on a Java project spends most of its context on command
output it cannot use. `mvn test` prints a plugin banner per lifecycle phase, one
line per test class, a transfer-progress bar, and a wall of advice text — nine out
of every ten bytes that reach the agent are Noise. The agent does not need to know
which tests passed; it needs to know that they passed, how many there were, and, when
something failed, exactly why.

The cost is not only money. Noise pushes the Evidence out of the working window, and
an agent that cannot see the assertion that failed re-runs the build, guesses, and
burns turns.

Two things are missing today: nobody trims this output, and nobody can tell the
developer how much was saved. Without the second, a trimming tool is a matter of
faith.

## Solution

`toltrim` is a command proxy. The agent prefixes the command it already runs:

```
toltrim mvn test
toltrim git diff
```

`toltrim` executes the command, trims the result down to Evidence according to that
tool's rules, prints it, and forwards the exit code untouched. Everything it cannot
trim runs unchanged, byte for byte, so an unproxied command behaves exactly as it
would alone.

Every Invocation appends one Gain Record, and `toltrim gain` prints a summary of
Proxied Commands, Pass-through Commands, Estimated Tokens before and after, and the
Savings Rate — overall and per command. `toltrim init` writes the command mapping
into the project's `AGENTS.md` so the agent knows to use the prefix.

## User Stories

**Maven: the success path**

1. As a coding agent, I want a successful `mvn test` to come back as one line per
   module, so that I know the work ran and how much of it, without reading 39 lines
   of plugin banners.
2. As a coding agent, I want that line to include the test counts (run, failures,
   errors, skipped) and elapsed time, so that I can distinguish "the tests passed"
   from "no tests were selected".
3. As a coding agent, I want the warning count rather than the warning text, so that
   warnings reach me only when I ask for them.
4. As a coding agent, I want a successful multi-module reactor to give me one line
   per module, so that I can see which module's tests ran.
5. As a coding agent, I want `mvn install` and `mvn package` to give the same one-line
   treatment as `mvn test`, so that the whole lifecycle is cheap and not just the test
   phase.
6. As a coding agent, I want a successful build that ran no tests to say only that the
   build succeeded, so that toltrim never claims tests passed when they did not run.
7. As a coding agent, I want the dependency-download chatter removed on both success
   and failure, so that a cold local repository does not cost me 90 KB of context.

**Maven: the failure path**

8. As a coding agent, I want every line containing a failure marker, so that I see
   each distinct failure rather than the first one.
9. As a coding agent, I want the assertion line that has no `[ERROR]` prefix, so that
   I can see `expected:<alice[]> but was:<alice[ ]>` and fix the test in one turn.
10. As a coding agent, I want a fixed context window around each failure line, so that
    the line that explains a failure is never separated from the line that announces it.
11. As a coding agent, I want that window extended to the boundaries of its block, so
    that a stack trace never arrives with frames missing from the middle.
12. As a coding agent, I want Maven's re-run advice and help URLs removed, so that I
    never pay tokens for text that mentions no project detail.
13. As a coding agent, I want a compile error to arrive with its file and line, so that
    I can edit the right file immediately.
14. As a coding agent, I want a dependency-resolution failure to keep the missing
    artifact and the reason, so that I can fix the POM or the offline flag myself.
15. As a coding agent, I want a failure whose markers I do not recognise to give me the
    untrimmed output instead of silence, so that I am never worse off than running the
    command alone.

**git**

16. As a coding agent, I want `git status` to stop listing every file under an untracked
    directory, so that a repo with a large build output does not flood my context.
17. As a coding agent, I want `git diff` with one line of context instead of three, so
    that I read the change and not the surroundings.
18. As a coding agent, I want `git diff` to drop index hashes, mode lines, and rename
    percentages, so that I pay for the patch and not for its metadata.
19. As a coding agent, I want `git diff` to keep every changed file and every hunk, so
    that trimming never hides a change I am supposed to review.
20. As a coding agent, I want `git log` reduced to hash, author, date, and subject, so
    that I can find the commit I need without reading bodies.
21. As a coding agent, I want `git show` summarised the same way as `git diff`, so that
    inspecting one commit is cheap.
22. As a coding agent, I want `git branch` to give me the list without decoration, so
    that I can check out the right branch.

**File listings**

23. As a coding agent, I want `ls`, `dir`, and `tree` to collapse noise directories
    (`.git`, `target`, `node_modules`) into a count, so that listing a project root
    costs a few lines rather than hundreds.

**Transparency and safety**

24. As a coding agent, I want a command toltrim does not know (`toltrim gcloud ...`) to
    be executed unchanged with its output byte-identical, so that prefixing a command
    is never a downgrade.
25. As a coding agent, I want `--raw` to give me the untrimmed output for one
    Invocation, so that I can recover when a rule is too aggressive.
26. As a coding agent, I want my exit codes to arrive unchanged, so that `git diff`
    exiting 1 for "there are differences" is not reported as a failure.
27. As a coding agent, I want toltrim to add no lines of its own to stdout, so that the
    output of a Proxied Command stays parseable.
28. As a coding agent, I want a Machine-Readable Request (`--porcelain`, `-z`,
    `--color=always`, `--batch-mode`, `-q`) to be trimmed only of ANSI codes, so that
    the format I asked for is the format I get.
29. As a developer, I want a command I pipe manually in my terminal
    (`toltrim git log | grep`) to stay untrimmed, so that my own pipelines keep working.
30. As a coding agent, I want toltrim never to block on a prompt when my shell has no
    terminal, so that an interactive command fails fast instead of hanging my turn.
31. As a developer, I want interactive commands to still prompt me when I run them in my
    own terminal, so that `toltrim mvn deploy` can ask for a password.
32. As a coding agent, I want a command that cannot be started to exit 127 with a
    one-line message, so that I can tell "not installed" from "failed".

**Accounting**

33. As a developer, I want `toltrim gain` to open with a summary — commands proxied,
    Estimated Tokens in, out, saved, and Savings Rate — so that the value is one glance.
34. As a developer, I want that summary to separate Proxied Commands from Pass-through
    Commands, so that unknown commands do not flatter or dilute the average.
35. As a developer, I want a per-command table with run count, tokens in, tokens out,
    and savings, so that I can see which commands pay for themselves.
36. As a developer, I want records kept indefinitely in one user-level database, so that
    a demo never loses its numbers to a retention policy.
37. As a developer, I want the token estimate labelled as an estimate, so that nobody
    mistakes it for billed usage.
38. As a developer, I want to clear the records explicitly, so that I can reset before a
    measurement.
39. As a developer, I want Gain Records to record the rule version that produced them,
    so that savings measured under different rules are not silently compared.
40. As a maintainer, I want a slow or failing statistics write never to change the
    command's exit code or output, so that statistics can never break a build.

**Adoption and portability**

41. As a developer, I want `toltrim init` to write the command mapping into the project's
    `AGENTS.md`, so that the agent learns the prefix from the repository rather than from
    my global configuration.
42. As a developer, I want `toltrim init` to be idempotent and previewable, so that
    re-running it or trying it out never duplicates or surprises.
43. As a Windows developer, I want the same command surface as on Linux and macOS, so
    that `toltrim dir` works where `dir` is a shell builtin.
44. As a Windows developer, I want `mvn.cmd`, `git.exe`, and `gradle.bat` found the same
    way `PATH` finds them, so that I do not have to spell out suffixes.
45. As a hackathon judge, I want to see a real agent fix a real failing test through
    toltrim, so that the saving is visible in a live transcript and not only in a slide.
46. As a judge, I want the same run replayed from a recorded fixture when the network
    fails, so that the demonstration does not depend on a repository being reachable.

## Implementation Decisions

**Shape.** An explicit-prefix proxy (ADR-0002) named `toltrim` (ADR-0001). Prefixing is
the entire interface; there is no `PATH` shadowing, no shell alias, no agent hook.

**Already built (first increment).** The transport layer, ANSI removal, Maven trimming,
and the fixture assertions exist and pass 13 tests. What follows describes the finished
MVP, including what is still to build.

**Modules.**

- *ANSI removal* — a pure string function. Runs before every trim, and is the only
  transformation applied to a preserved Invocation.
- *Transport* — spawns with an argument vector, never a shell; captures both streams
  concurrently to avoid pipe deadlock; returns stdout, stderr, the exit code verbatim,
  and an optional spawn error. It never inspects output; it only collects it. stderr is
  forwarded verbatim for every tool.
- *Dispatch* — maps a program name to a Tool (Maven, git, listing, unknown), accepting a
  path and Windows executable suffixes, and answers whether an Invocation may be trimmed
  at all.
- *Trimming rules* — one pure rule set per Tool family, with no knowledge of processes.
- *Gain Records* — persists one record per Invocation and renders the `gain` view.
- *Init* — writes the command mapping into a project's `AGENTS.md`.

**Seam 1, the pure render seam.** One function is the boundary for every trimming rule:

```
render(tool, args, stdout_bytes, exit_code, preserve) -> output_bytes
```

`preserve` is computed by the caller so that the preservation decision is testable
without a process. Rules:

- If `preserve`, strip ANSI and remove nothing else. If the bytes are not valid UTF-8,
  emit them unchanged.
- If the tool is unknown, emit the bytes unchanged (ADR-0002).
- Otherwise apply the tool's rule set.

**Preservation predicate.**

```
preserve = --raw
        OR machine_readable(tool, args)
        OR (stdout is not a terminal AND stdin is a terminal)
```

`--raw` is consumed by toltrim and never forwarded to the command. Machine-readable
flags are matched as a global list of unambiguous long flags plus a per-tool list of
short flags, because `-e` means `--errors` to Maven and `--edit` to git. The terminal
test is the corrected form of ADR-0003: an agent capturing output also has a
non-terminal stdout, so "not a terminal" alone must not mean "do not trim".

**Maven rules.**

- Success is `exit_code == 0` and the output contains `BUILD SUCCESS`. The Success
  Summary is one line per module span, where a module span starts at a
  `----< groupId:artifactId >----` header:
  `<artifactId>: BUILD SUCCESS | <module test totals> | <total time> | W:<count>`.
  The test segment is omitted when no module totals line exists, so a build that ran no
  tests never claims they passed. Warning text is never included; only the count is.
- Failure keeps every line matching `[ERROR]`, `[FATAL]`, `Caused by:`, `<<< FAILURE!`,
  `<<< ERROR!`, `BUILD FAILURE`, plus six lines of context either side, extended to the
  boundaries of the block (stopping at a blank line, an `[INFO]` separator, a
  transfer-progress line, or Maven advice), capped at sixty lines per side. Maven's
  re-run advice, the `-> [Help 1]` line, and `[Help 1] http` URLs are dropped. Transfer
  progress (`Progress (`, `Downloaded from `, `Downloading from `) is dropped even
  inside a context window.
- The Under-trimming Floor: if what survives is under 2% of the original, emit the
  original output capped at 400 lines instead. Under-trimming beats silent truncation.

**git rules.**

- `status` — collapse an untracked directory's files into one line with a count.
- `diff`, `show` — context reduced from three lines to one; `index`, `old mode`,
  `new mode`, `similarity index`, `rename from`, `rename to`, and `\ No newline at end of
  file` metadata dropped; every file and hunk header kept. No per-file truncation.
- `log` — hash, author, date, and subject; default the last twenty commits.
- `branch` — names only, current branch marked.
- `add`, `commit`, `checkout`, `pull`, `push`, `merge`, `rebase` are Passthrough
  Commands and are never interpreted (ADR-0008).

**File-listing rules.** `ls`, `dir`, and `tree` share one rule set: noise directories
(`.git`, `target`, `node_modules`, `build`, `.idea`) collapse to a count, entries group
by kind, and long lines are truncated.

**Gain Record schema** (decision-rich, inline from the session's design):

```
gain_record
  ts                INTEGER   -- unix seconds
  tool              TEXT      -- maven | git | listing | passthrough
  subcommand        TEXT      -- e.g. "test", "diff"
  mode              TEXT      -- trimmed | preserved | passthrough
  exit_code         INTEGER
  bytes_in          INTEGER
  bytes_out         INTEGER
  tokens_in         INTEGER   -- bytes_in / 4, rounded up
  tokens_out        INTEGER
  rules_version     INTEGER
  cwd               TEXT
```

One user-level database (ADR-0006): `~/.local/share/toltrim/history.db` on Unix,
`%LOCALAPPDATA%\toltrim\history.db` on Windows. Records are kept indefinitely and
cleared only by an explicit command. Writes are synchronous and best-effort: a failed
write must never alter the command's output or exit code. Pass-through Commands are
recorded with `mode = passthrough` and are excluded from the Savings Rate.

**Estimated Tokens.** Bytes divided by four, rounded up, and presented as an estimate,
never as billed usage (ADR-0007). The Savings Rate is computed over Proxied Commands
only.

**Stdin.** Inherited when toltrim's stdin is a terminal; otherwise an empty stream, so
an agent's Invocation fails fast rather than blocking on a prompt.

**Exit codes.** Forwarded verbatim. Killing by signal `n` reports `128 + n`, matching
shell convention. A command that cannot be started exits 127 with a one-line message on
stderr.

**Windows.** `PATHEXT` resolution to find `mvn.cmd`, `git.exe`, `gradle.bat`; if a direct
spawn fails to find an executable, retry once through `cmd /c` with the original argument
vector. This is the only platform-conditional path (ADR-0009). Command names are handled
as OS strings, not UTF-8 strings, so a non-UTF-8 path is never corrupted.

**Init.** `toltrim init` writes an idempotent mapping block into the project's
`AGENTS.md`, with a preview mode; it does not touch global agent configuration.

**Statistics view.** `toltrim gain` prints a summary block (commands proxied, split into
trimmed and passthrough; tokens in, out, saved; savings rate; the estimate method) and a
per-command table (runs, tokens in, tokens out, savings), with a filter for the current
project.

## Testing Decisions

**What makes a good test here.** A test asserts what an agent would read, not how the
rule works. Every fixture gets two kinds of assertion: something that must survive (the
assertion line, the failing test name, the changed hunk) and something that must not
(the plugin banners, the download chatter, the help URLs). A test that only asserted
"saved bytes" would pass while the tool quietly deleted the reason a build failed — the
recorded Surefire fixture exists precisely because the obvious `[ERROR]`-only rule loses
the assertion, and a size assertion would never have caught it. Tests must not assert
kept line counts, window sizes, or internal ordering.

**Seam 1 (primary): the pure render function.** All rules, all preservation decisions,
and the unknown-command passthrough are tested here, driven by recorded output under
`tests/fixtures/`. Fast, precise, and process-free.

**Seam 2: the built binary with stand-in programs.** Only the process contract, which a
pure function cannot observe: exit code forwarded verbatim (including a non-zero exit
from a successful-looking run), byte-identical passthrough of an unknown command,
stdin empty when there is no terminal, and exit 127 for a command that does not exist.
A small stand-in program replays a fixture and exits with a chosen code.

**Modules tested.** ANSI removal and tool detection have focused unit tests. Maven rules
and (as built) git and listing rules are tested through Seam 1. The transport layer
through Seam 2.

**Prior art.** `tests/fixtures.rs` already demonstrates the pattern with three
assertions on recorded Maven output, including the original noise and comparison cases.
Inline unit tests in the ANSI and dispatch modules demonstrate the focused-unit-test
pattern.

**Fixtures to add.** A multi-module Maven reactor run; a Maven run using `-q`; a
compile-error build; `git status --porcelain` and a plain `git status` with a large
untracked directory; a `git diff` with renames, mode changes, and a missing trailing
newline; a `git log`; `ls -la` and `tree` of a project root; and a CRLF-terminated
Windows Maven run.

**Verification of the claim itself.** The Savings Rate must be reproducible from the
recorded fixtures: the fixture byte counts and the recorded Gain Records must agree, so
the demo's headline number can be re-derived offline.

## Out of Scope

- Gradle and Gradle wrapper trimming: deferred by the revised ADR-0008. A Gradle parser
  is a second parser with its own output shapes and fixture budget, and nothing in this
  spec depends on it.
- `java`, `javac`, `cat`, `head`, `tail`, `docker`, `kubectl`, `curl`, process and port
  inspection (ADR-0008).
- Any interpretation of state-changing git commands (ADR-0008).
- Agent hooks, `PATH` shadowing, shell aliases, MCP servers (ADR-0002).
- A real tokenizer, and any token count presented as billed usage (ADR-0007).
- Async statistics writes, latency budgets, and performance benchmarks — explicitly
  declined, so statistics never become a source of risk.
- Retention policies or automatic pruning of Gain Records.
- A TUI dashboard, multi-repository aggregation, and cost-in-dollars reporting.
- Injecting Maven flags (`-q`, `--batch-mode`) into the proxied command; the caller's
  arguments are passed through untouched.

## Further Notes

- The research on rtk — the closest prior art — is in `rtk-research-report.md`. Three
  findings shaped this spec: rtk estimates tokens as bytes/4 with no tokenizer; it stores
  history in SQLite; and it forwards stderr verbatim and only trims when the exit
  code is 0, which does **not** transfer to Maven, because Maven writes its `[ERROR]`
  lines to stdout. This spec therefore decides failure handling on its own evidence
  rather than inheriting rtk's rule.
- Gradle is deferred, and ADR-0008 now says so. The conflict this spec originally
  flagged is resolved: Gradle was in ADR-0008's Proxied Command list while having no
  rules here, and the ADR was amended rather than the spec expanded.
- The demo is a live agent fixing a real failing test (decided in session, not an ADR).
  The recorded fixtures are the fallback if the network fails, since a cold Maven cache
  changes what is on screen from "errors extracted" to "downloads removed".
- Deliberately not built: any behaviour where toltrim prints a line of its own on stdout.
  Savings are reported by `toltrim gain`, never injected into the proxied command's
  output.
- `toltrim init` intentionally modifies only the project's `AGENTS.md`. Mutating a
  developer's global agent configuration is out of scope on principle, not on effort.

# toltrim

A command proxy that stands between a coding agent and the developer tools it
runs, so the agent receives the Evidence it needs to act and none of the Noise —
and so the saving can be counted.

**The pitch, in one line:** prefix the command you already run with `toltrim` and
the same command comes back with far fewer Estimated Tokens and none of the
Evidence removed.

```
toltrim mvn test
toltrim git diff
```

An Invocation is an explicit prefix (ADR-0002). There is no `PATH` shadowing, no
shell alias, and no agent hook: `toltrim` executes the Proxied Command, trims its
stdout down to Evidence, prints it, and forwards the exit code untouched. Every
other command is a Passthrough Command and is executed unchanged, byte for byte,
so prefixing a command is never a downgrade.

---

## Contents

- [Installation](#installation)
- [Proxied Commands](#proxied-commands)
- [Every other command](#every-other-command)
- [Using toltrim](#using-toltrim)
- [Accounting: `toltrim gain`](#accounting-toltrim-gain)
- [Estimated Tokens and the Savings Rate](#estimated-tokens-and-the-savings-rate)
- [Savings](#savings)
- [Behaviour you can rely on](#behaviour-you-can-rely-on)
- [Platforms](#platforms)
- [Uninstalling](#uninstalling)
- [Licence](#licence)

---

## Installation

Prebuilt archives are produced by the release workflow in
`.github/workflows/release.yml` for the three platform families:

| Platform family | Rust target | Archive |
| --- | --- | --- |
| Linux, x86-64 | `x86_64-unknown-linux-gnu` | `.tar.gz` |
| macOS, Apple silicon | `aarch64-apple-darwin` | `.tar.gz` |
| macOS, Intel | `x86_64-apple-darwin` | `.tar.gz` |
| Windows, x86-64 | `x86_64-pc-windows-msvc` | `.zip` |

Each `v*` tag publishes those archives on the GitHub release and attaches them to
it; a manual `workflow_dispatch` builds the same archives without publishing.
Every archive is named `toltrim-<version>-<target>` and holds the `toltrim`
binary, this `README.md`, and the licence.

The release binary is a `--release` Rust build with SQLite compiled in
(`rusqlite`'s `bundled` feature), so it has **no runtime dependency on Rust and no
runtime dependency on a JDK**. It is a single self-contained executable.

### The install script

macOS and Linux:

```sh
curl -fsSL https://raw.githubusercontent.com/shaneshou/toltrim/main/install.sh | sh
```

The script detects the operating system and architecture, maps them to a release
archive, downloads it, checks that it is a non-empty archive, extracts the binary
into `${TOLTRIM_INSTALL_DIR:-$HOME/.local/bin}`, makes it executable, and prints
exactly what it changed and where. Nothing is left behind on a failure: the
binary is staged and moved into place in one step, and the temporary directory is
removed on every exit.

```sh
sh install.sh                  # the latest release
sh install.sh --version v0.1.0 # a specific tag
sh install.sh --help
```

Windows (PowerShell):

```powershell
irm https://raw.githubusercontent.com/shaneshou/toltrim/main/install.ps1 | iex
```

The PowerShell script installs into `%LOCALAPPDATA%\toltrim\bin`, adds that
directory to the **user** `PATH` when it is not already there, and prints what it
changed.

```powershell
.\install.ps1                  # the latest release
.\install.ps1 -Version v0.1.0  # a specific tag
.\install.ps1 --help
```

Both scripts accept a repository override in `TOLTRIM_REPO` (`owner/repo`), which
is what a fork needs.

### From source

With a Rust toolchain (1.91 or newer):

```sh
cargo install --path .
```

or, in a checkout:

```sh
cargo build --release --offline --locked
# target/release/toltrim
```

This is also the route for a platform the release workflow does not build, such
as Linux on ARM.

---

## Proxied Commands

A Proxied Command is one toltrim knows how to trim. The list is exactly the one
in ADR-0008:

- `toltrim mvn test`
- `toltrim mvn verify`
- `toltrim mvn package`
- `toltrim mvn compile`
- `toltrim mvn install`
- `toltrim mvn clean`
- `toltrim mvn test-compile`
- `toltrim mvn integration-test`
- `toltrim git status`
- `toltrim git diff`
- `toltrim git log`
- `toltrim git show`
- `toltrim git branch`
- `toltrim ls`
- `toltrim dir`
- `toltrim tree`

**Gradle is deferred.** `gradle` and `gradlew` are *not* Proxied Commands. A
Gradle parser is a second parser with its own output shapes and its own fixture
budget, and nothing in this tool depends on it, so the ADR was amended rather
than the scope expanded. `gradle build` through toltrim behaves exactly like
`gradle build` alone.

### Commands that are always Passthrough Commands

Always Passthrough Commands: git add, git commit, git checkout, git pull, git push, git merge, git rebase.

Those change repository state, so they are never interpreted: toltrim runs them
and returns their streams and their exit code verbatim, whatever rules exist for
the rest of git.

### What "Proxied" means in practice

- Maven: a successful build becomes one Success Summary line per module, with
  each module's own test counts, elapsed time, and warning count. A failure
  becomes a Failure Extract: every line bearing a failure marker, the assertion
  or compile error that explains it, and the context that makes those lines
  meaningful — without the plugin banners, download chatter, or advice text. A
  failure whose markers are not recognised yields the untrimmed output rather
  than silence, and the Under-trimming Floor emits the untrimmed output whenever
  trimming would leave almost nothing behind.
- `git status`: an untracked directory's files collapse into one counted line.
- `git diff` and `git show`: one line of context instead of three, with index
  hashes, mode lines, and rename percentages dropped — every changed file and
  every hunk is kept.
- `git log`: hash, author, date, and subject.
- `git branch`: names only, current branch marked.
- `ls`, `dir`, `tree`: Noise directories (`.git`, `target`, `node_modules`,
  `build`, `.idea`) collapse to a count, entries group by kind, and over-long
  lines are truncated with a visible marker.

## Every other command

`toltrim <command> <args...>` for anything that is not a Proxied Command runs the
command unchanged, byte for byte. That includes `java`, `javac`, `cat`, `head`,
`tail`, `docker`, `kubectl`, `curl`, `gradle`/`gradlew`, and process or port
inspection. toltrim still records a Gain Record for the Invocation, marked as a
Passthrough Command, but it never flatters the Savings Rate.

---

## Using toltrim

```
toltrim <command> [args...]   run a command through toltrim
toltrim --raw <command> ...   run the command and emit its stdout untrimmed
toltrim init [--dry-run]      write the command mapping into ./AGENTS.md
toltrim gain [--here|--all] [--clear]
                              show or clear Gain Records
toltrim --help                this usage, on stdout, exit 0
toltrim --version             the version, exit 0
```

- `--raw`: `toltrim --raw mvn test` gives one Invocation's untrimmed stdout (ANSI
  is still removed). It is the escape hatch when a rule is too aggressive for the
  moment. `--raw` is consumed by toltrim and never forwarded to the command.
- `init`: `toltrim init` writes an idempotent mapping block into `<cwd>/AGENTS.md`
  so an agent working in the repository learns the prefix from the repository.
  Re-running it replaces the block in place rather than duplicating it, and
  `--dry-run` prints the file that would result and writes nothing. It touches no
  global agent configuration.
- A Machine-Readable Request — `--porcelain`, `-z`, `--color=always`,
  `--batch-mode`, `-q`, and the like — is preserved: only ANSI is removed, so the
  format the caller asked for is the format the caller gets.
- Exit codes are forwarded verbatim: `git diff` exiting 1 for "there are
  differences" is not reported as a failure. A command that cannot be started
  exits 127 with one line on stderr, and a command killed by signal `n` reports
  `128 + n`, matching shell convention.
- stderr is forwarded verbatim for every tool. toltrim adds no lines of its own to
  a Proxied Command's stdout.

### Where Gain Records live

One user-level SQLite database holds every Gain Record (ADR-0006):

- `~/.local/share/toltrim/history.db` on Unix, honouring `XDG_DATA_HOME`;
- `%LOCALAPPDATA%\toltrim\history.db` on Windows.

Set `TOLTRIM_DB` to override the path — that is what the tests and the demo do to
keep a measurement out of the developer's own history:

```sh
TOLTRIM_DB=/tmp/demo.db toltrim mvn test
TOLTRIM_DB=/tmp/demo.db toltrim gain
```

Records are kept indefinitely and removed only by `toltrim gain --clear`.

---

## Accounting: `toltrim gain`

`toltrim gain` aggregates the Gain Records and prints them. `--here` scopes the
view to the current directory, `--all` names the default whole-history scope
explicitly, and `--clear` deletes the records in scope and confirms in one line.
An unknown option exits 2 with one line on stderr.

Here is the real view over the demo's fixture replay (see
[Savings](#savings) for what that replay covers):

```
Commands proxied: 22 (trimmed 20, preserved 2)
Passthrough Commands: 0 (excluded from the Savings Rate)
Estimated Tokens in: 30008
Estimated Tokens out: 3203
Estimated Tokens saved: 26805
Savings Rate: 89.3% of the original, over Proxied Commands only
Passthrough Estimated Tokens in: 0 (unchanged, so they save nothing)

Estimated Tokens are an estimate, not billed usage: output bytes / 4, rounded up.
The Savings Rate is the figure to trust; the absolute Estimated Tokens above are approximate.

By command:
COMMAND        RUNS  TOKENS IN  TOKENS OUT  SAVED
maven test        5      25544        1418  24126
maven compile     1       1554         329   1225
git status        2       1124          57   1067
listing           9       1120        1005    115
git diff          1        209         127     82
git show          1        264         182     82
git log           1        142          65     77
git branch        2         51          20     31
```

The summary separates Proxied Commands from Passthrough Commands, and the
Savings Rate counts Proxied Commands only, so an unknown command can neither
flatter nor dilute it. Each Gain Record also carries the rule version
(`RULES_VERSION`, currently 2) that produced it, so savings measured under
different rules are never silently compared.

---

## Estimated Tokens and the Savings Rate

Estimated Tokens are a byte-based approximation (ADR-0007): **output bytes
divided by four, rounded up, with no tokenizer and no model call**. toltrim ships
no tokenizer and never contacts a provider to count anything.

The approximation is wrong for CJK text and for dense machine formats, so the
absolute numbers are only useful for comparing before against after. The
**Savings Rate is the trustworthy figure**: Estimated Tokens removed as a share of
the original, counted over Proxied Commands only, at one decimal place. Never read
Estimated Tokens as billed usage.

---

## Savings

The demo is a coding agent fixing a real failing test through toltrim. The
evidence is the recorded fixtures under `tests/fixtures/`, which replay offline
when a repository or a warm Maven cache is not reachable. Replaying them through
the pure render seam gives the demo's headline figure:

```
Savings Rate: 89.3% (recorded fixtures, 22 Invocations)
```

That is 30008 Estimated Tokens in and 3203 out — 26805 removed over 22 Proxied
Commands, 20 trimmed and 2 preserved (`mvn -o -q -Dstyle.color=always test` and
`git status --porcelain`, both Machine-Readable Requests that must arrive intact
and therefore save nothing).

The figure covers exactly these Invocations, in the order
`tests/savings_claim.rs` lists them:

| # | Invocation | Exit | Handling | Fixture |
| --- | --- | --- | --- | --- |
| 1 | `mvn test` | 0 | trimmed | `mvn-test-pass.txt` |
| 2 | `mvn test` | 1 | trimmed | `mvn-test-fail-surefire.txt` |
| 3 | `mvn test` | 1 | trimmed | `mvn-dependency-resolution-failure.txt` |
| 4 | `mvn compile` | 1 | trimmed | `mvn-compile-error.txt` |
| 5 | `mvn -o -q -Dstyle.color=always test` | 1 | preserved | `mvn-quiet-test-fail.txt` |
| 6 | `mvn -o test` | 0 | trimmed | `mvn-reactor-test-pass.txt` |
| 7 | `git status --untracked-files=all` | 0 | trimmed | `git-status-large-untracked.txt` |
| 8 | `git status --porcelain` | 0 | preserved | `git-status-porcelain.txt` |
| 9 | `git diff -M --cached` | 1 | trimmed | `git-diff-metadata.txt` |
| 10 | `git show -M HEAD` | 0 | trimmed | `git-show-commit.txt` |
| 11 | `git log` | 0 | trimmed | `git-log.txt` |
| 12 | `git branch` | 0 | trimmed | `git-branch.txt` |
| 13 | `git branch -v` | 0 | trimmed | `git-branch-verbose.txt` |
| 14 | `ls` | 0 | trimmed | `ls-project-root.txt` |
| 15 | `ls -1` | 0 | trimmed | `ls-1-project-root.txt` |
| 16 | `ls -l` | 0 | trimmed | `ls-l-project-root.txt` |
| 17 | `ls -la` | 0 | trimmed | `ls-la-project-root.txt` |
| 18 | `ls -la "$PWD/"` | 0 | trimmed | `ls-la-trailing-slash.txt` |
| 19 | `ls -m` | 0 | trimmed | `ls-columns-project-root.txt` |
| 20 | `ls -1` | 0 | trimmed | `ls-long-line.txt` |
| 21 | `tree` | 0 | trimmed | `tree-project-root.txt` |
| 22 | `dir` | 0 | trimmed | `dir-project-root.txt` |

Two of the 22 are format-accurate composed captures rather than recorded process
output, and the test says so at the entry: `tree-project-root.txt` (this machine
has no `tree` binary) and `dir-project-root.txt` (this machine is not Windows).
They are named the same real directory's contents and are the Invocations toltrim
must handle on a machine that does have those tools.

`tests/fixtures/mvn-reactor-test-pass-crlf.txt` is deliberately **not** a 23rd
Invocation: it is the LF reactor capture with its line endings converted to CRLF,
a derived twin rather than a second Invocation.

`tests/savings_claim.rs` is the verification of the claim itself. It recomputes
the figure from the fixture bytes through `toltrim::trim::render`, replays a
trimmed Invocation, a preserved one, and a Passthrough Command through the built
binary and asserts that the Gain Records `toltrim gain` reports agree with the
seam's byte counts, and then parses the published line out of this README and
asserts that it equals the recomputation. If either side drifts — a fixture
changes, a rule changes, or someone edits the figure — the test fails.

---

## Behaviour you can rely on

- **The exit code is the command's.** It is forwarded verbatim; signal `n` is
  reported as `128 + n`; a command that cannot start exits 127 with one line on
  stderr.
- **stderr is never trimmed.** It is forwarded byte for byte for every tool.
- **No output of toltrim's own is injected into a Proxied Command's stdout.**
  Savings are reported by `toltrim gain`, never in the middle of a build's output.
- **A Passthrough Command is byte-identical** to running it alone.
- **Statistics can never break a command.** Gain Record writes are synchronous and
  best-effort: a database that cannot be written changes neither the output nor
  the exit code.
- **stdin is inherited only when toltrim's own stdin is a terminal**, so an
  interactive command fails fast for an agent instead of blocking a turn, and
  still prompts a developer in their own terminal.

## Platforms

macOS, Linux, and Windows. `mvn.cmd`, `git.exe`, and the like are found the way
`PATH` finds them, and `dir` works where it is a shell builtin.

The release workflow builds the Linux and Windows archives in CI. It is the only
place those two platform families are produced and tested: the release build on
this checkout was verified on macOS/aarch64, where Linux and Windows artifacts
cannot be built or run.

## Uninstalling

- macOS and Linux: remove the one file the installer created —
  `rm "${TOLTRIM_INSTALL_DIR:-$HOME/.local/bin}/toltrim"`.
- Windows: remove the one directory —
  `Remove-Item -Recurse -Force "$env:LOCALAPPDATA\toltrim\bin"`.

Neither script touches anything else. Gain Records live in the database above and
are removed with `toltrim gain --clear` or by deleting that file.

## Licence

Apache-2.0. See [`LICENSE`](LICENSE).

toltrim is not affiliated with Apache, Maven, Gradle, or Git.

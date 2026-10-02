# `rtk-ai/rtk` — Factual Research Report

**Repo:** https://github.com/rtk-ai/rtk · **Default branch: `develop`** (NOT `main`).
`https://raw.githubusercontent.com/rtk-ai/rtk/main/README.md` → **HTTP 404**. `master` still exists for `install.sh` only.

All "VERIFIED" quotes are verbatim from fetched raw source at branch `develop`, pinned to tree SHA `ff9bd8e6d06cb671e35e00db23a26ceb40645935`.

---

## 1. CLI surface — VERIFIED

One binary, `rtk`, built by clap derive. It is **both** an explicit proxy (`rtk git status`) **and** a transparent rewrite target: an agent hook rewrites `git status` → `rtk git status` before execution.

Subcommand enum (`src/main.rs`, `enum Commands`, 100+ variants). Quoted samples:

```rust
    Run {
        /// Command string to execute through a shell
        #[arg(short = 'c', long = "command", conflicts_with = "args")]
        command: Option<String>,
        /// Shell used with -c (defaults to sh on Unix and cmd on Windows)
        #[arg(long, value_name = "SHELL", requires = "command", conflicts_with = "args")]
        shell: Option<String>,
        /// Program and literal arguments for direct execution (alternative to -c)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Execute command without filtering but track usage
    Proxy {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
```

RTK-only subcommands that never fall back to raw execution (`src/core/constants.rs`):

```rust
pub const RTK_META_COMMANDS: &[&str] = &[
    "gain", "discover", "learn", "init", "config", "proxy", "recall", "run", "hook",
    "hook-audit", "pipe", "cc-economics", "verify", "trust", "untrust", "session",
    "rewrite", "telemetry", "smart", "deps", "json", "err", "summary",
];
```

**Config file** (`README.md`): `~/.config/rtk/config.toml`; macOS `~/Library/Application Support/rtk/config.toml`:

```toml
[hooks]
exclude_commands = ["curl", "playwright"]  # skip rewrite for these (matches `npx playwright` too)
suppress_hook_warning = false

[retriever]
mode = "sqlite"         # sqlite (default) | tee (legacy files) | disabled
```

**Pattern/glob matching:** there is no glob matcher on command names. Two distinct mechanisms:
- **Hook rewrite matching** is regex over the command string, driven by `src/discover/registry.rs` (251 KB data file). The shell hook comments say: `# To add support for new commands, update src/discover/registry.rs (PATTERNS + RULES).`
- **Filter matching** is a regex field in each TOML filter: `match_command = "^my-tool\\b"  # regex matched against the full command string`.

`rtk run`/`err`/`test`/`summary` deliberately do **not** expand globs/variables: "preserve positional argument boundaries and do not expand globs, variables, or operators by default".

---

## 2. Output interception — VERIFIED

**Mechanism:** RTK spawns the real child, captures stdout **and** stderr via pipes, filters, prints.

```rust
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
```
`src/core/stream.rs`. Both fds are read on separate threads (`stderr_thread`); stderr capture is capped — `eprintln!("[rtk] warning: stderr exceeds 10 MiB — capture truncated");` and `RAW_CAP`.

**stderr handling is conditional** (`src/core/runner.rs`): by default stderr is forwarded verbatim. It is only compressed when the command **succeeded** and stdout was non-empty:

```rust
    if exit_code != 0 || shown_stdout.trim().is_empty() {
        return Cow::Borrowed(stderr);
    }
    // ...
    if stderr.len() < crate::core::tee::MIN_TEE_SIZE {
        return Cow::Borrowed(stderr);
    }
```
Per-filter opt-in exists for tools that banner to stderr (`src/filters/README.md`): `filter_stderr | bool | Capture and merge stderr into stdout before filtering (use for tools like liquibase that emit banners to stderr)`.

**Exit code passthrough — VERIFIED.** Runner returns the child code; `main` propagates it:

```rust
    let code = match run_cli() {
        Ok(code) => code,
        Err(e) => { eprintln!("rtk: {:#}", e); 1 }
    };
    std::process::exit(code);
```
`src/main.rs`. Shell-compatible error codes are documented: "A program that cannot be run answers the way the shell used to: `127` with a `command not found` line, `126` for a path that exists but is a directory or is not executable."

**Marker/notice — VERIFIED** (`src/core/tee.rs`):

```rust
        Stored::Saved(s) => Some(format!("[full output: rtk recall {}]", s.hash)),
        Stored::Saved(s) if s.hidden_lines > 0 => Some(format!(
            "[+{} hidden: rtk recall {}]",
```

**Raw escape hatches — VERIFIED:** `rtk proxy <cmd>` ("Execute command without filtering but track usage"), `rtk run <cmd>` (`--format json` README: "Raw direct execution (no filtering/tracking)"), `rtk pipe --passthrough`, `rtk recall --full`, and `-v/-vv/-vvv` verbosity plus `-u/--ultra-compact`.

**Filter pipeline (8 stages)** — from `src/filters/README.md`:

> `8-stage pipeline strip_ansi → replace → match_output → strip/keep_lines → truncate → tail_lines → max_lines → on_empty`

Filter lookup priority: `.rtk/filters.toml` (project) → `~/.config/rtk/filters.toml` (user) → `BUILTIN_TOML` (embedded) → raw passthrough. First match wins.

---

## 3. Token-savings statistics — VERIFIED

**Commands.** `rtk gain` (summary), `rtk gain --graph`, `--history`, `--daily`, `--weekly`, `--monthly`, `--all`, `--quota -t {pro,5x,20x}`, `--recalls`, `--failures`, `--project`, `--format {text,json,csv}`, `--reset [--yes]`. Plus `rtk discover`, `rtk session`, `rtk cc-economics`, `rtk telemetry`.

Quoted clap definition (`src/main.rs`):

```rust
    Gain {
        /// Filter statistics to current project (current working directory) // added
        #[arg(short, long)]
        project: bool,
        /// Show ASCII graph of daily savings
        #[arg(short, long)]
        graph: bool,
        /// Show recent command history
        #[arg(short = 'H', long)]
        history: bool,
        /// Show monthly quota savings estimate
        #[arg(short, long)]
        quota: bool,
        /// Subscription tier for quota calculation: pro, 5x, 20x
        #[arg(short, long, default_value = "20x", requires = "quota")]
        tier: String,
        /// Show detailed daily breakdown (all days)
        #[arg(short, long)]
        daily: bool,
        ...
        /// Output format: text, json, csv
        #[arg(short, long, default_value = "text")]
        format: String,
        /// Show recall efficiency per filter (elisions vs agent recalls)
        #[arg(long)]
        recalls: bool,
```

**Displayed columns:** `Date | Cmds | Input | Output | Saved | Save%`, plus KPI lines via `print_kpi("Total commands", ...)`, `print_kpi("Input tokens", ...)`, `print_kpi("Output tokens", ...)`, and `print_efficiency_meter(summary.avg_savings_pct)` (`src/analytics/gain.rs`).

**Storage — VERIFIED: SQLite.** Path = `dirs::data_local_dir() / "rtk" / "history.db"`:

```rust
pub const RTK_DATA_DIR: &str = "rtk";
pub const HISTORY_DB: &str = "history.db";
pub const RECALL_DB: &str = "recall.db";
pub const DEFAULT_HISTORY_DAYS: i64 = 90;
```
(`src/core/constants.rs`), resolved as `Ok(user_dirs::data_under(".").join(HISTORY_DB))` with a config override `tracking.database_path` taking priority (`src/core/tracking.rs`). Docs state `~/.local/share/rtk/history.db` (Linux/macOS), **retention 90 days**, scope global.

> ⚠️ **Inconsistency found:** the doc-comment inside `src/core/tracking.rs` says `~/.local/share/rtk/tracking.db` while the code constant and `docs/guide/analytics/gain.md` say `history.db`. The **code** (`HISTORY_DB = "history.db"`) is authoritative.

**Schema — VERIFIED verbatim** (`src/core/tracking.rs`, `run_schema_migrations`, `SCHEMA_VERSION: i64 = 1`):

```sql
CREATE TABLE IF NOT EXISTS commands (
    id INTEGER PRIMARY KEY,
    timestamp TEXT NOT NULL,
    original_cmd TEXT NOT NULL,
    rtk_cmd TEXT NOT NULL,
    input_tokens INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    saved_tokens INTEGER NOT NULL,
    savings_pct REAL NOT NULL
)
```
plus later migrations `ALTER TABLE commands ADD COLUMN exec_time_ms INTEGER DEFAULT 0` and `ADD COLUMN project_path TEXT DEFAULT ''`, and two more tables:

```sql
CREATE TABLE IF NOT EXISTS parse_failures (
    id INTEGER PRIMARY KEY, timestamp TEXT NOT NULL, raw_command TEXT NOT NULL,
    error_message TEXT NOT NULL, fallback_succeeded INTEGER NOT NULL DEFAULT 0)

CREATE TABLE IF NOT EXISTS hook_decisions (
    id INTEGER PRIMARY KEY, timestamp TEXT NOT NULL, session_id TEXT NOT NULL,
    tool_use_id TEXT NOT NULL, project_path TEXT DEFAULT '', raw_cmd TEXT NOT NULL,
    decision TEXT NOT NULL, rewritten_cmd TEXT, rtk_version TEXT NOT NULL)
```

---

## 4. Token estimation — VERIFIED: `bytes / 4`, no tokenizer

`src/core/tracking.rs` (the doc page cites this as `tracking.rs:1284`; actual current lines are 1798–1807 — line number has drifted):

```rust
pub fn estimate_tokens(text: &str) -> usize {
    estimate_tokens_from_len(text.len())
}

/// Token estimate from a raw byte length, for callers that hold a byte count rather
/// than a `&str` (e.g. non-UTF-8 captured output). Same ~4-chars-per-token model as
/// [`estimate_tokens`].
pub fn estimate_tokens_from_len(len: usize) -> usize {
    (len as f64 / 4.0).ceil() as usize
}
```

Documented formula (`docs/guide/analytics/gain.md`):

```
Input Tokens  = estimate_tokens(raw_command_output)
Output Tokens = estimate_tokens(rtk_filtered_output)
Saved Tokens  = Input - Output
Savings %     = (Saved / Input) × 100
```

Note it divides **byte length**, not char count, despite the `~4 chars per token` comment. README is explicit about the limitation: "The token counts RTK reports are estimated as `bytes / 4` — RTK ships no tokenizer, so the **percentages are reliable but the absolute token numbers are approximate**." No USD/cost-per-token constant exists in `tracking.rs`; the only currency figure is the aggregate telemetry field "Estimated USD value, derived from the estimated tokens saved and a fixed internal constant".

---

## 5. Supported commands & per-command filtering

**Two filter engines:**
1. **TOML declarative filters** — 62 files, `src/filters/*.toml`, concatenated alphabetically by `build.rs` into `BUILTIN_TOML` embedded via `include_str!`. DSL fields: `description`, `match_command`, `strip_ansi`, `filter_stderr`, `strip_lines_matching`, `keep_lines_matching`, `replace`, `match_output`, `truncate_lines_at`, `max_lines`, `tail_lines`, `on_empty`.
2. **Hardcoded Rust filters** — `src/cmds/<lang>/*.rs`, e.g. `git/git_cmd.rs` (271 KB), `git/diff_cmd.rs` (330 KB), `jvm/mvn_cmd.rs` (255 KB), `dotnet/dotnet_cmd.rs` (120 KB), `cloud/aws_cmd.rs` (103 KB), `rust/cargo_cmd.rs` (99 KB), `system/search.rs` (90 KB).

Full TOML filter list (62): `ansible-playbook, basedpyright, biome, brew-install, bundle-install, composer-install, df, dotnet-build, du, fail2ban-client, gcc, gcloud, hadolint, helm, iptables, jira, jj, jq, just, liquibase, make, markdownlint, mise, mix-compile, mix-format, nx, ollama, oxlint, ping, pio-run, poetry-install, pre-commit, ps, pulumi-destroy, pulumi-preview, pulumi-refresh, pulumi-stack, pulumi-up, quarto-render, rsync, shellcheck, shopify-theme, skopeo, sops, spring-boot, ssh, stat, swift-build, systemctl-status, task, terraform-plan, tofu-fmt, tofu-init, tofu-plan, tofu-validate, trunk-build, turbo, ty, uv-sync, xcodebuild, yadm, yamllint`.

Complete small examples, verbatim:

```toml
[filters.du]
description = "Compact du output"
match_command = "^du\\b"
strip_lines_matching = ["^\\s*$"]
truncate_lines_at = 120
max_lines = 40
```

```toml
[filters.ps]
description = "Compact ps output — truncate wide lines, limit rows"
match_command = "^ps(\\s|$)"
strip_ansi = true
truncate_lines_at = 120
max_lines = 30
```

**Per-command compression** (README table, verbatim summaries): `ls`/`tree` → tree format with file counts; `cat`/`read` → signatures/structure over bodies; `grep`/`rg` → truncate long lines, group matches by file; `ast-grep` → group by file, cap overflow; `git status` → compact stat grouped by state; `git diff` → reduced context, headers stripped; `git log` → hash/author/subject only; `cargo test`/`npm test` → failures only, passing collapsed to a count; `ruff`/`sqlfluff` → grouped by rule+file; `pytest` → failures only, traceback trimmed; `go test` → NDJSON parsed, failures only; `docker ps` → essential fields only. Four strategies: "Smart Filtering, Grouping, Truncation, Deduplication - Collapses repeated log lines with counts".

**git is NOT a TOML filter** — verified: no `git-*.toml` exists in `src/filters/`; git logic is in Rust (`git_cmd.rs` has `fn run_diff`, `fn run_show`, `fn parse_hunk_header`, `fn hunk_truncation_note`, `fn blob_truncation`, `fn compact_blob_show`, plus a `Budget` type).

---

## 6. Repo metadata — VERIFIED

| Field | Value |
|---|---|
| `full_name` | `rtk-ai/rtk` (owned by GitHub **Organization** `rtk-ai`, id 258253854) |
| Primary language | **Rust** (4,988,808 bytes); also Shell 140,950 / TypeScript 48,012 / Python 21,267 / Ruby 1,178 |
| Stars | **82,207** · forks 5,216 · watchers 82,207 · subscribers 214 |
| Open issues | 1,566 |
| License | **Apache-2.0** (`"spdx_id":"Apache-2.0"`) |
| Created / pushed | `2026-01-22T16:54:16Z` / `2026-10-02T06:35:46Z` (updated `2026-10-02T10:09:10Z`) |
| Default branch | **`develop`** |
| Homepage | https://www.rtk-ai.app · Discord + Homebrew formula present |
| Version | `Cargo.toml` `version = "0.49.0"`, `edition = "2024"`, `rust-version = "1.91"` |
| Binary name | `rtk` (crate/package `rtk`); release assets `rtk-x86_64-apple-darwin.tar.gz`, `rtk-aarch64-unknown-linux-gnu.tar.gz`, `rtk-x86_64-pc-windows-msvc.zip`, `.deb`/`.rpm` via `cargo-deb`/`cargo-generate-rpm` |
| Rust source files | **133** under `src/` (134 `.rs` repo-wide); 446 blobs total in the tree |
| Approx LOC | **~129,000** (`src/` = 4,259,041 bytes ÷ ~33 bytes/line). *Byte-size estimate, not a `tokei` count.* |
| Topics | agentic-coding, ai-coding, anthropic, claude-code, cli, command-line-tool, cost-reduction, developer-tools, llm, open-source, productivity, rust, token-optimization |

**"Zero dependencies" is marketing, not literal — VERIFIED CONTRADICTION.** GitHub `description`: *"CLI proxy that reduces LLM token consumption by 60-90% on common dev commands. Single Rust binary, zero dependencies"*. But `Cargo.toml` has 24 direct dependencies and `Cargo.lock` lists **213 packages**:

```toml
[dependencies]
clap = { version = "4", features = ["derive"] }
anyhow = "1.0"
encoding_rs = "0.8"
codepage = "0.1"
oem_cp = "2"
ignore = "0.4"
walkdir = "2"
regex = "1"
serde = { version = "1", features = ["derive"] }
serde_json = { version = "1", features = ["preserve_order"] }
colored = "3"
dirs = "5"
rusqlite = { version = "0.31", features = ["bundled"] }
toml = "0.8"
toml_edit = "0.22"
chrono = "0.4"
tempfile = "3"
sha2 = "0.10"
ureq = "2"
getrandom = "0.4"
flate2 = "1.0"
quick-xml = "0.37"
which = "8"
```
Plus `[target.'cfg(unix)'.dependencies] libc`, `[target.'cfg(windows)'.dependencies] windows-sys`, and dev-dep `temp-env`. Notably `rusqlite` with `features = ["bundled"]` means the SQLite C library is compiled in. `[lints.rust] unsafe_code = "deny"` and `warnings = "deny"`. *Inference: the claim most plausibly means "no runtime dependency / no tokenizer / no external service", not "no crates".*

---

## 7. Agent integration — VERIFIED

**No MCP server exists.** Grepped the entire recursive tree for any path containing `mcp` → zero matches. Integration is hook/plugin/instructions based.

**Auto-rewrite hook mechanism.** `.github/hooks/rtk-rewrite.json` (verbatim):

```json
{
  "hooks": {
    "PreToolUse": [
      { "type": "command", "command": "rtk hook", "cwd": ".", "timeout": 5 }
    ]
  }
}
```

`rtk init -g` installs this PreToolUse hook (plus `RTK.md`) into the agent's `settings.json`. Since **v0.37.2** it is a **native binary command `rtk hook claude`** — "no Unix shell, bash, or jq required"; older installs used a bash script `rtk-rewrite.sh` (still in `.claude/hooks/`, "rtk-hook-version: 4").

**`rtk rewrite` exit-code protocol** (verbatim comment from `rtk-rewrite.sh`):

```
# Exit code protocol for `rtk rewrite`:
#   0 + stdout  Rewrite found, an allow rule matched → auto-allow
#   1           No RTK equivalent → pass through unchanged
#   2           Deny rule matched → pass through (Claude Code native deny handles it)
#   3 + stdout  Ask rule matched, or no rule matched → rewrite but let Claude Code prompt the user
```

Hook decisions are logged to the `hook_decisions` SQLite table (typed enum `HookOutcome { Allow, Ask, Deny, Defer }`, serialized as `"allow"|"ask"|"deny"|"defer"`).

**Instructions file:** `CLAUDE.md` (7,715 bytes) exists in-repo as RTK's *own* dev guidance; for users, `rtk init` writes `RTK.md`. Awareness is configurable: `[awareness] level = "high"` tells the agent about `rtk gain`/`rtk proxy`; `"full"` makes the agent prefix `rtk` itself. Codex gets `updatedInput` + `AGENTS.md`; Cline/Kilo/Windsurf/Kimi use project rules files.

**18+ agents supported:** Claude Code, GitHub Copilot (VS Code + CLI), Cursor, Gemini CLI, Codex, Windsurf, Cline/Roo Code, OpenCode, OpenClaw, Pi, Oh My Pi, Hermes, Mistral Vibe, Kilo Code, Google Antigravity, Kimi AI, Factory Droid, Trae.

**Shell alias / PATH shim — NOT FOUND.** No alias or PATH shim exists; the README's `git status → rtk git status` transparency comes strictly from the hook rewriting the tool call. Installers only add `~/.local/bin` to PATH for the binary itself.

**Important scope limit (verbatim):** "the hook only runs on Bash tool calls. Claude Code built-in tools like `Read`, `Grep`, and `Glob` do not pass through the Bash hook, so they are not auto-rewritten."

---

## 8. Explicit gaps / unverified

- **`main` branch does not exist** (404); everything above is from `develop`, except `install.sh` which was fetched from `master`.
- **Line-number citation drift:** `docs/guide/analytics/gain.md` cites `src/core/tracking.rs:1284` for `estimate_tokens`; the function is actually at **1798–1807**. Quote is accurate, line number is stale.
- **No crates.io release verified.** README warns: *"Another project named 'rtk' (Rust Type Kit) exists on crates.io. If `rtk gain` fails, you have the wrong package."* I did not fetch crates.io/docs.rs — treat the crates.io `rtk` crate as **unrelated** until checked.
- **No blog/HN post searched.** I did not run a web search for third-party coverage, so I have no independent corroboration of the 60–90% headline claims.
- **`rtk gain --quota` denominators** (pro/5x/20x monthly token budgets) are described only as matching "Anthropic Claude API subscription levels"; the concrete numbers were not located in the files fetched.
- **LOC figure is derived** from TOML blob `size` fields (÷33 bytes/line), not measured with a code counter, and includes test code and doc comments (likely a large share of these files).
- **`src/cmds/system/cc_economics.rs`** returned 404 at the guessed path; the real file is `src/analytics/cc_economics.rs` (not read).

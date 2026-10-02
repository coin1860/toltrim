# toltrim

A command proxy that stands between a coding agent and the developer tools it
runs, so the agent receives the output it needs to act and none of the output it
does not — and so the saving can be counted.

## Language

**Trim**:
Removing output that carries no information an agent needs to choose its next
action, while keeping the output it does need.
_Avoid_: compress, filter, minimize, condense, shrink

**Evidence**:
Output an agent needs in order to act correctly on a command's result, such as a
failure block, the counts that prove work happened, or an exit code.
_Avoid_: signal, important output, error info

**Noise**:
Output that cannot change what an agent should do next, such as progress lines,
per-plugin banners, download chatter, or per-test success lines.
_Avoid_: clutter, spam, chaff, verbose output

**Proxied Command**:
A command toltrim knows how to trim.
_Avoid_: supported command, wrapped command, filtered command

**Passthrough Command**:
A command toltrim runs unchanged, byte for byte, because it has no trimming rule.
_Avoid_: unsupported command, raw command, fallback

**Invocation**:
One run of `toltrim <command> <arguments…>`.
_Avoid_: call, execution, run

**Gain Record**:
The persisted accounting for one Invocation: how large the output was before and
after, its estimated tokens, and how the command ended.
_Avoid_: log row, metric, event, stat

**Estimated Tokens**:
A rough measure of how much text an agent must read, used only to compare before
against after.
_Avoid_: token count, token usage, cost

**Savings Rate**:
Estimated Tokens removed, as a share of the original, counted over Proxied
Commands only.
_Avoid_: compression ratio, reduction, efficiency

**Machine-Readable Request**:
An Invocation where the caller asked for a parseable format, or where the output
is consumed by another program rather than read by an agent.
_Avoid_: raw mode, strict mode, quiet mode

**Success Summary**:
What an agent sees after a Proxied Command succeeded: the counts that prove the
work ran.
_Avoid_: success message, OK output, done line

**Failure Extract**:
What an agent sees after a Proxied Command failed: every line needed to fix the
failure, together with the context that makes those lines meaningful.
_Avoid_: error log, stack trace, error output

**Under-trimming Floor**:
The rule that an Invocation emits its untrimmed output when trimming would leave
almost nothing behind.
_Avoid_: safety net, fallback threshold, guard rail

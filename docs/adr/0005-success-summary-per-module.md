# Success emits a per-module summary, never silence

A successful Proxied Command yields one compact line per module — the counts that prove the work ran (tests run / failures / errors / skipped, elapsed time, warning count) plus the build result. Warnings are reported as a count, not in full.

**Why not silence**: empty output is ambiguous. An agent cannot distinguish "the tests passed" from "the command never ran", and that ambiguity causes real misbehaviour. Disambiguation is the best use of the few tokens this path spends.

**Considered options**: emit the full test summary block (more context, but the per-module line already carries the counts); emit warnings in full (a large project can produce hundreds of lines for warnings that are not the agent's current problem).

**Consequences**: when a successful command contains no test summary, toltrim reports only that the build succeeded — it must never claim that tests passed.

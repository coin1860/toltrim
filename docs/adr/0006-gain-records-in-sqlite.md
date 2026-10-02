# Gain Records live in a single user-level SQLite database

Every Invocation appends one Gain Record to a SQLite database in the user's data directory (`~/.local/share/toltrim/history.db`, `%LOCALAPPDATA%\toltrim\history.db`), and `toltrim gain` aggregates it on read. Records are kept indefinitely and cleared only on explicit request.

**Considered options**: append-only JSONL (no dependency, inspectable with `cat`, and the recommendation at the time — set aside so that per-command and per-project views are queries rather than code); a single JSON file rewritten per Invocation (races between concurrent Invocations).

**Consequences**: writes are synchronous; no latency budget is claimed, and statistics must never turn a successful build into a failed one. Passthrough Commands are recorded but excluded from the Savings Rate, so "the agent ran 200 commands" stays answerable without a wide command surface flattering the average.

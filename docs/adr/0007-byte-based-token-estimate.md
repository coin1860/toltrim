# Estimated Tokens are a byte-based approximation, not a tokenizer

Estimated Tokens are derived from output length with a fixed bytes-per-token divisor. toltrim ships no tokenizer and never calls a model to count tokens. The Savings Rate is presented as the trustworthy figure and the absolute token numbers as approximate.

**Considered options**: a real BPE tokenizer (dependency weight and build risk for a two-day build, and another model's tokenizer would be wrong for the reader anyway); counting tokens through a provider API (a network call inside a command proxy is unacceptable).

**Consequences**: the divisor is wrong for CJK text and for dense machine formats, so Saved Tokens must never be presented as billed usage.

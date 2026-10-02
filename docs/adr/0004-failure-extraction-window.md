# A failure keeps every matching line plus a context window

For a failed Proxied Command, toltrim keeps lines matching failure markers (`[ERROR]`, `[FATAL]`, `Caused by`, `<<< FAILURE!`) together with a fixed window of surrounding lines.

**Why not the obvious rule**: "keep only lines containing `[ERROR]`" is the natural first implementation, and it silently loses the most valuable line in a real Surefire failure. In recorded output the assertion sits on its own unprefixed line:

```
[ERROR] demo.FooTest.boom -- Time elapsed: 0 s <<< FAILURE!
org.junit.ComparisonFailure: user name should be trimmed expected:<alice[]> but was:<alice[ ]>
```

An agent shown only the first line knows a test failed and not why, so it spends extra turns — the opposite of this tool's purpose.

**Consequences**: an Under-trimming Floor applies. When trimming would leave almost none of the original output, the untrimmed output is emitted instead; under-trimming is preferred over silent truncation, and the recorded fixtures assert both directions (the assertion must survive, the plugin banners must not).

**Refined after seeing the output.** A fixed window leaves a hole in the middle of a stack trace — the frames between two markers' windows disappear while a later marker's window pulls the tail back in, so the agent reads a trace that skips from frame 5 to frame 20. A kept line is therefore extended to its block boundaries (blank line, Maven advice line, or transfer-progress line stops it) with a per-side cap. The stop conditions are the load-bearing part, not the extension: without them the first marker's window walks backwards through the download block and drags a 91 KB run back in. Measured on the recorded failure: 91182 → 3561 bytes, exit code 1 preserved.

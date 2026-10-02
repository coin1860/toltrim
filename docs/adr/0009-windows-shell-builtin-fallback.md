# Windows shell builtins fall back to `cmd /c`, after a direct spawn fails

Commands are spawned directly with an argument vector and never through a shell. On Windows only, if the direct spawn fails to find an executable, toltrim retries through `cmd /c` with the original argument vector, so shell builtins such as `dir` — which is not an executable file — still work.

**Considered options**: route every command through a shell (best compatibility, but adds shell quoting and injection risk for arbitrary caller-supplied arguments, which is unacceptable in a proxy for `git` and Maven); declare shell builtins unsupported (cleanest implementation, but `toltrim dir` would be broken on a platform the tool claims to support).

**Consequences**: `PATHEXT` resolution is needed to find `mvn.cmd`, `git.exe`, and `gradle.bat`; this fallback is the only platform-conditional code path in the tool.

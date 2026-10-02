# The tool is named `toltrim`, not `tr`

The short name `tr` collides with the POSIX command `/usr/bin/tr`, which appears inside build scripts, Makefiles, and shell one-liners — exactly the environment this tool has to sit in front of on `PATH`. We chose `toltrim` so the name can be placed on `PATH` without ever shadowing a standard command.

**Considered options**: keep `tr` but never put it on `PATH` (transparency then depends on shell aliases, which agent-spawned non-interactive shells rarely read); keep `tr` on `PATH` anyway (a character-translation call inside someone else's build script becomes a call into this tool, which is a real failure mode on Linux and in CI).

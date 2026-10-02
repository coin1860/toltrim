# MVP trims a fixed command set; everything else passes through

Proxied Commands in the MVP are the Maven lifecycle (`test`, `verify`, `package`, `compile`, `install`, `clean`, `test-compile`, `integration-test`), `git status`/`diff`/`log`/`show`/`branch`, and file listings (`ls`, `dir`, `tree`). Every other command is a Passthrough Command, executed unchanged.

**The explicit no-s**: commands that change repository state (`git add`, `commit`, `checkout`, `pull`, `push`, `merge`, `rebase`) are always passed through and never interpreted; interactive run modes (`mvn spring-boot:run`, `gradle bootRun`, `java -jar`) are not proxied; and `cat`/`head`/`tail`, `docker`, `kubectl`, `curl`, `gradle`/`gradlew`, and process/port inspection tools are out of scope.

**Revised: Gradle is deferred.** It was originally in the Proxied Command list, but a Gradle parser is a second parser with its own output shapes (`> Task :x` banners, `* What went wrong:` blocks) and its own fixture budget, and nothing in the MVP depends on it. Deferring it is a scope decision, not a difficulty one: shipping a command we never exercised would contradict this ADR's own reasoning.

**Why**: this tool makes one claim — the same command, far fewer tokens, no important line lost. Each additional Proxied Command is another untested branch that can fail in front of an audience, while credibility rests entirely on the Maven and git paths.

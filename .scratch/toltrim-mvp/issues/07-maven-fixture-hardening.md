# 07: Maven rules hold on four more recorded shapes

**What to build:** The Maven rules are proven against the shapes real Java projects
produce: a multi-module reactor, a quiet run, a compile error, and a CRLF-terminated
Windows run. The per-module Success Summary is currently the least verified part of the
tool — it has never been run against actual multi-module output.

**Blocked by:** 01 (pure render seam).

**Status:** ready-for-agent

- [ ] A recorded multi-module reactor run yields one Success Summary line per module, and
      each module's test counts are attributed to that module — a module with tests is
      never given another module's totals, and a module without tests omits the count
      segment.
- [ ] A recorded compile-error build keeps the file and line of each compilation error.
- [ ] A recorded quiet run (`-q`) is passed through with ANSI stripped and nothing
      removed, since a quiet run is a Machine-Readable Request.
- [ ] A recorded CRLF-terminated run produces no stray carriage returns and no spurious
      blank lines, and its Success Summary is identical to the LF version of the same run.
- [ ] Each new fixture asserts both directions: the evidence that must survive and the
      Noise that must not.

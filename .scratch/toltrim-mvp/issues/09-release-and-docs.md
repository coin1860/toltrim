# 09: Anyone can install toltrim without a Rust toolchain

**What to build:** Prebuilt artifacts for macOS, Linux, and Windows, an install script for
each platform family, and a README that states what is supported, how to use it, and how
to read the savings. The advertised command set matches what actually shipped.

**Blocked by:** 03, 04, 05, 06, 07, 08 — the artifacts and the README must describe the
delivered command set, and the demo's headline number must be re-derivable from the
recorded fixtures before it is published.

**Status:** ready-for-agent

- [ ] A release produces runnable artifacts for the three platform families, with no
      runtime dependency on Rust or a JDK.
- [ ] An install script puts the binary on `PATH` and prints what it changed; uninstalling
      is removing one file or directory.
- [ ] The README documents installation, the `toltrim <command>` invocation, `--raw`,
      `gain`, `init`, and the exact list of Proxied Commands — and that list matches
      ADR-0008, including the Gradle deferral.
- [ ] The README states that Estimated Tokens are an approximation and that the Savings
      Rate is the trustworthy figure.
- [ ] The Savings Rate published for the demo can be recomputed offline from the recorded
      fixtures and Gain Records.
- [ ] `toltrim --help` and `toltrim --version` work on a fresh install.

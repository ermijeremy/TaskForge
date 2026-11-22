# Issue #9 verification status

**Status: passed.** JSON reporting, CLI judging/readiness and explicit runner
shutdown are complete. No exercise placeholders remain.

Passed checks:

- Normal workspace suite: **129 passed, 7 native checks ignored by default**.
- Formatting and Clippy with warnings denied.
- Six JSON/exit-code regressions and CLI argument/preparation-error checks.
- All seven native checks explicitly run: **7 passed**, including both new CLI
  tests and the execution library's five existing host tests.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --offline --locked -- -D warnings
cargo test --workspace --offline --locked
TASKFORGE_REAL_CONFIG="$PWD/examples/judge.local.json" \
  cargo test --workspace --offline --locked -- --ignored --test-threads=1
```

The native CLI test ran all three accepted solutions, the known wrong solution,
whitespace variation, syntax/exception, CPU/wall, memory and stdout/stderr/mixed
output fixtures. Completed user verdicts returned exit zero, with exactly one
bounded JSON document and newline on stdout, and no stderr. An unavailable
sandbox produced error JSON with exit three and no fabricated verdict.

Every command verified empty sandbox/cgroup state and no quarantine marker.
The test's dedicated temporary directory was empty after each judging command,
confirming cached runtime-image and trusted temporary-file cleanup before exit.
Native readiness JSON and checked runtime-image removal also passed explicitly.

Normal tests cover source size/UTF-8/regular-file requirements, FIFO rejection
without waiting for a writer, active-ID/path traversal rejection, optional and
zero measurements, integer preservation and worst-case diagnostic JSON escaping.
Shutdown tests cover cancelled workers, owned image removal and shared-owner
failure reporting. These are observations of tested paths, not a kernel security
proof or a hard real-time guarantee under arbitrary host failure.

The seven native tests remain opt-in; their default skips are not counted as
host verification. Phase 1's consolidated matrix and additional error-path audit
belong to issue #10. See the [CLI guide](../judge-local-cli.md).

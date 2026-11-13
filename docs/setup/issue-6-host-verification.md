# Issue #6 restricted-sandbox verification

**Status: passed.** The completed limit conversion and the real restricted-runtime
integration test pass on the documented native Linux setup. This publishable
report records behavior and test outcomes; deployment-specific host observations
are retained locally.

## Real execution check

```sh
TASKFORGE_REAL_CONFIG="$PWD/examples/judge.local.json" \
  cargo test -p taskforge-execution --offline --locked --test sandbox_real \
  -- --ignored --nocapture
```

Result: **1 passed, 0 failed, 0 ignored**. The test explicitly ran rather than
counting a default-suite skip as evidence. It uses the runtime versions and
layout specified in the [lifecycle guide](../sandbox-lifecycle.md).

Seven executions reuse one dedicated slot. The test checks:

| Probe | Observed outcome |
| --- | --- |
| Sum-two accepted source, stored position 1 | Expected stdout `5` |
| Sum-list accepted source, stored position 4 | Expected stdout `6` |
| Balanced-brackets accepted source, stored position 4 | Expected stdout `YES` |
| Subprocess creation | `SUBPROCESS_DENIED` |
| External network access | `NETWORK_DENIED` |
| Trusted-file reads and filesystem writes | `FILESYSTEM_DENIED` |
| Fresh-box reuse | `FRESH`; only the new submitted source was present |

Every run exited successfully and produced structured CPU/wall/memory evidence.
These are selected execution checks, not full 30-case verdict evaluation or
benchmark results. The fixed-probe harness captures at most 4097 bytes per stream,
fails above 4096, drains both streams concurrently and uses a ten-second
running-process guard.

Before testing read denial, the harness confirms its nonsecret marker and
expected-output file are readable on the host. Both stay outside the runtime
image and source mounts. The write probe attempts to create files in the source
and runtime trees; every attempt is denied.

The harness confirms the box directory and box cgroup are removed after each
execution. A separate post-run scan also found no remaining sandbox processes,
box resources or quarantine marker for the tested slot. Persistent OS lock files
and the cgroup keeper are expected to remain.

## Normal checks and completed exercise

Passed:

- `cargo test --workspace --offline --locked`: **71 passed**, three host-dependent
  checks ignored by default, including the real test explicitly passed above.
- `cargo clippy --workspace --all-targets --offline --locked -- -D warnings`
- `cargo fmt --all -- --check`
- `cargo build --workspace --all-targets --offline --locked`
- `cargo run -p taskforge-execution --offline --locked --example isolate_limits`:
  the four documented default limit arguments were emitted successfully.

The six core exercise regressions now run normally. Fake-tool lifecycle tests use
the completed conversion and check ID contention/reuse, source permissions,
restricted arguments, setup/input/execution/metadata errors, cleanup, quarantine,
explicit cancellation/drop control flow and rejection of invalid limits before
any box is touched. These mock checks do not establish real cancellation or
failure-path cleanup guarantees.

The separate native staging-only check also passed. It verifies the copied
interpreter/loader, filtered encodings, absence of site packages and exactly three
approved extensions without executing any submitted source.

## Remaining scope

The module supplies per-test execution and evidence, not the full `Runner`
evaluation pipeline. Combined-output enforcement, the overall infrastructure
deadline, async supervision, real cancellation/error cleanup verification,
resource-exhaustion fixtures and verdict precedence remain issues #7–#10.
Low-level waits and Drop can block and are not all covered by the fixed test
harness's guard.

The [setup guide](linux-isolate.md) and [lifecycle guide](../sandbox-lifecycle.md)
define requirements and assumptions. These tests are observations, not a formal
security proof or production-host guarantee. Rerun after relevant runtime, kernel,
policy or service changes. Skipped tests never count as verification evidence.

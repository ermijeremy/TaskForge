# Issue #7 verification status

**Status: passed.** Output accounting, supervised protocol checks and the four
explicit native-host checks pass with the supported CPython 3.14.10 runtime.

## Runtime and shutdown fixes

Readiness still enforces an exact Python patch version. Its pin and trusted smoke
probe now require 3.14.10. Active problem definitions use new `-v2` IDs for this
runtime; the original `-v1` definitions for 3.14.8 remain unchanged. The revision
regression verifies that only IDs and runtime identifiers differ.

The first native output-limit run exposed a manager-shutdown race. Sending
SIGKILL immediately bypassed Isolate's shutdown handler and could race cgroup
removal. The supervisor now requests SIGTERM, waits up to 250 ms within the
remaining cleanup budget, then escalates if necessary. It always invokes Isolate
cleanup afterward. A protocol regression verifies that the manager receives its
termination request before cleanup. Failed cleanup still quarantines the slot.

The supported behavior follows Isolate's pinned
[signal handling and sandbox teardown](https://github.com/ioi/isolate/blob/v2.7/isolate.c)
and [cgroup removal](https://github.com/ioi/isolate/blob/v2.7/cg.c) implementation.

## Checks run

Passed:

- `cargo test --workspace --offline --locked`: **92 passed, 4 ignored**.
- `cargo fmt --all -- --check` and Clippy with `-D warnings`.
- Python fixture syntax inventory: 15 parsed, one intentional syntax error; no
  fixture executed by that syntax-only command.
- All four host-dependent tests explicitly invoked: **4 passed**, including
  restricted runtime staging, readiness, restricted execution/access probes and
  the new supervision integration test.

Native command, run outside the outer workspace sandbox on the provisioned host:

```sh
TASKFORGE_REAL_CONFIG="$PWD/examples/judge.local.json" \
  cargo test -p taskforge-execution --offline --locked \
  -- --ignored --test-threads=1
```

The native supervision test verifies:

| Scenario | Observed outcome |
| --- | --- |
| Normal execution and repeated session tests | Expected output, metadata, unchanged shared deadline and clean slot reuse |
| Stdout-only, stderr-only and combined excess | Output-limit evidence recorded; retained output at most the configured 40000 bytes |
| Infinite loop | CPU timeout metadata |
| Sleeping program | Wall timeout metadata with low CPU time |
| Evaluation budget shorter than sandbox limits | Infrastructure deadline error, followed by cleanup and reuse |
| Explicit cancellation and aborted waiter | Worker completes cleanup and the dedicated slot is reusable |

A final check acquired every configured slot lock and confirmed no remaining
sandbox directories, box cgroups or quarantine markers. Persistent lock files
remain intentionally. The earlier failed native attempt's dedicated slot was
cleaned, verified empty and released from quarantine before rerunning the tests.

Normal protocol checks additionally cover setup failures, execution/metadata
errors, cleanup failure quarantine, cancellation during setup, late cleanup
recovery, output capacity rejection and an async response deadline that leaves
cleanup ownership with the worker. Protocol doubles do not establish real
isolation on their own.

These observations cover the tested paths on the supported host, not arbitrary
kernel failure or a hard real-time guarantee. Full verdict evaluation and broader
failure/load integration remain later issues. Native checks remain opt-in in the
normal suite and must be rerun after relevant runtime or host policy changes.

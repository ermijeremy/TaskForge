# Per-test Isolate lifecycle (issue #6)

**Status: implemented and checked on the supported native host.** The core
limit-conversion exercise is complete. The real test verifies selected fixtures,
access restrictions and fresh slot reuse; this is not a completed judge or a
formal security proof. Supervision and verdict evaluation remain later work.

## Library ownership

`IsolateBackend::new(config)` runs the existing native readiness check, then
stages one immutable runtime image shared through `Arc`. Construct the backend
before concurrent evaluations: the existing readiness check needs idle IDs.
Cloned backends share the staged runtime; each prepared test owns its own slot,
source directory, trusted files and cleanup state.

`backend.prepare(source, limits)` first invokes `IsolateLimits::try_from(limits)`.
An unrepresentable limit becomes `InvalidConfiguration` before a slot is reserved
or source staged. With valid limits, preparation:

1. Validates the source byte bound (1..=65536).
2. Acquires the first free, non-quarantined ID in the configured inclusive range.
3. Calls `isolate --cg --box-id=N --init` for a fresh box.
4. Stages only `box/main.py`, mode 0444, using create-new semantics.
5. Returns a `PreparedSandbox` owning the OS file lock until cleanup.

`prepared.spawn(input)` accepts at most 8 KiB of test input. It stores that input
outside the sandbox and inherits a read-only file as stdin. Source/runtime are
read-only mounts; expected output is not an argument to this API. Metadata lives
in a different trusted host file, never a staged submission file.

`RunningSandbox` owns the child and prepared sandbox. It exposes stdout/stderr
pipe handles, the configured combined-output budget, `try_wait()`, `finish()` and
`cancel()`. The run command uses `--silent` to suppress routine Isolate status
lines on the inherited stderr; fatal helper errors may still appear there.
`finish()` waits, reads bounded structured metadata and explicitly cleans up. Wrapper exit 0/1 may represent valid program outcomes; other wrapper
exits and internal-error metadata become infrastructure errors. Classification
into user verdicts remains issue #8; this module does not implement `Runner` yet.

## Restricted native runtime

The supported initial runtime is the documented Fedora x86_64 CPython 3.14.8
installation at `/usr/bin/python3.14`. Its image contains:

- That interpreter, executable but not writable.
- Exactly four explicitly named ELF runtime files: the dynamic loader,
  libpython3.14, libc and libm. The loader retains execute permission.
- Standard-library `.py` files under `/usr/lib64/python3.14`, with symlinks
  rejected and site-packages, caches, virtual-environment/install tooling,
  test trees and configuration directories excluded.
- Exactly three approved native extensions: math, select and _socket.

Only this private image's `/usr` and `/lib64` subtrees and the test's `/box` are
mounted, read-only/nonrecursive. The code never mounts host `/usr`, `/lib64`,
`/etc`, `/proc`, `/tmp`, `/run`, `/home`, credentials or the Docker socket into a
submission. There is no user-supplied mount list. Paths containing Isolate option
separators/newlines are rejected. Python uses `-I -B -S`: isolated interpreter
configuration, no bytecode writes, and no site startup. The minimal environment
adds only `LANG=C.UTF-8` to Isolate's own default rule. Network sharing and extra
file-descriptor inheritance are never enabled.

The source filter permits pure standard-library modules, not every native Python
extension or third-party package. Additional packages/extensions require explicit
runtime-policy work. Host interpreter/libraries are trusted administrator-managed
files; this is not independent binary provenance verification. An image is a
snapshot; recreate the backend after runtime upgrades. Copying is synchronous and
must run outside an async worker's executor thread.

There is no writable submission filesystem or scratch mount. I/O uses stdin and
output pipes. Opening a host-only marker is meaningful only after the harness
confirms it is readable on the host. The real test creates its own nonsecret
marker and expected-output file, keeps both outside the image/source mounts,
and confirms host readability before the access probe.

## Limits and metadata

The completed core exercise converts configured CPU/wall milliseconds to decimal seconds
and rounds memory bytes upward to KiB within Isolate's signed integer limits.
Process count must be one. The fixed filesystem policy also sets core/file-size
options and a 32-descriptor limit; these do not supervise output pipes.

`SandboxMetadata::parse` caps input at 16 KiB, rejects duplicate keys, malformed
values, conflicting exit-code/signal fields and missing outcome evidence. It
preserves reported status, exit code/signal, optional exact CPU/wall `Duration`s,
optional peak memory in bytes, and explicit killed/OOM flags. Cgroup memory is
preferred to max-rss when present; unavailable values remain None. Metadata is
raw evidence, not a verdict classifier, and unknown keys within the size bound
are tolerated for Isolate's auxiliary measurements.

The argument units and lifecycle follow the
[pinned Isolate manual](https://github.com/ioi/isolate/blob/v2.7/isolate.1.txt),
[argument parsing/lifecycle source](https://github.com/ioi/isolate/blob/v2.7/isolate.c),
[directory rules](https://github.com/ioi/isolate/blob/v2.7/rules.c), and
[cgroup cleanup implementation](https://github.com/ioi/isolate/blob/v2.7/cg.c).

## Cleanup, slots and failure behavior

Slot leases use the same `box-N.lock` files as readiness. Allocation is
nonblocking, skips occupied slots and reports unavailable when none is free.
Persistent lock files are never unlinked. Setup/input/spawn errors explicitly
attempt cleanup; normal completion cleans after metadata collection. If both
execution and cleanup fail, the cleanup infrastructure error records the original
reason code in its bounded trusted diagnostic.

A failed cleanup creates `box-N.quarantine` in the judge-owned lock directory.
Later allocation skips that ID. The marker prevents a cleanup failure from being
reported as a healthy reusable slot. An operator must stop users of the affected
slot, run Isolate cleanup as the same judge user, verify no box/cgroup/process
remains, and only then remove that specific quarantine marker. Do not clear
quarantine markers automatically or unlink active lock files. Readiness is an
explicit resetting check and must not run while test slots are active.

`cancel()` kills/reaps the wrapper and invokes Isolate cleanup to terminate the
cgroup; wrapper termination alone never counts as cleanup. Drop is a blocking,
best-effort backstop and cannot report cleanup errors, so callers must use the
explicit finish/cancel/cleanup methods. Unexpected process exit, SIGINT, panic,
service failure and unkillable process behavior are not yet comprehensively
verified. Fake tools verify these control-flow/error paths; the real test verifies
normal execution and cleanup. Real cancellation and failure cleanup remain part
of the issue #7/#10 supervision verification.

## Supervision boundary and current limitations

Callers must concurrently drain both stream handles before finishing; do not
replace this with unbounded reads or sequential draining of arbitrary source.
The small real-test harness uses bounded captures and a ten-second guard only
for its fixed probes. It is not a general supervisor.

Issue #7 implements combined output enforcement, cancellation-safe orchestration,
the whole-evaluation infrastructure deadline and async integration. This module's
control commands have ten-second/16 KiB guards, but running-process waits, drop,
staging, and pipe reading are not yet covered by one infrastructure deadline.
Do not expose this low-level API as a general judging CLI/API before that work.
Only trusted init/cleanup commands use temporary-file captures; submission
streams remain pipes. No ordinary-process submission fallback exists.

## Checks

```sh
cargo test --workspace --offline --locked
cargo clippy --workspace --all-targets --offline --locked -- -D warnings
cargo fmt --all -- --check
```

The six completed exercise regressions run normally with
`--test sandbox_limits_exercise`. Three host-dependent checks remain explicitly
ignored by default. A host staging-only check can run without executing any
submitted source:

```sh
TASKFORGE_REAL_CONFIG="$PWD/examples/judge.local.json" \
  cargo test -p taskforge-execution --offline --locked --lib \
  sandbox::runtime::tests::real_restricted_runtime_image_contents \
  -- --ignored --exact --nocapture
```

The real execution command is in the [learning guide](learning/issue-6.md);
its successful run and cleanup observations are in the
[host log](setup/issue-6-host-verification.md). Issue #3's broad trusted smoke
probe remains separate evidence from this restricted runtime test.

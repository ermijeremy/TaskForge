# Issue #6 — Core exercise: map limits to Isolate safely

**Status: completed by the project owner.** The conversion validates all supplied
limits before the public launch path reserves a slot or stages a submission. The
six regression tests run in the normal suite, and a real supported-host test
verifies selected fixtures, access restrictions and slot reuse. The original
contract and learning notes are retained below.

## Implementation contract

The completed `TryFrom<&ExecutionLimits> for IsolateLimits` is in
[`sandbox/limits.rs`](../../crates/taskforge-execution/src/sandbox/limits.rs).
It validates representability and constructs the private fields. All production
and protocol-test preparation uses this conversion; no unchecked public bypass
or duplicate conversion is provided.

`ExecutionLimits` already guarantees positive settings. Isolate 2.7 imposes
narrower representation limits that must be checked too:

| Input | Required representation / rule |
| --- | --- |
| `processes().get()` | Must equal 1; otherwise `UnsupportedProcessCount` |
| `cpu_time().as_millis()` | 1..=i32::MAX milliseconds; otherwise `TimeOutOfRange { field: "cpu_time_ms" }` |
| `wall_time().as_millis()` | 1..=i32::MAX milliseconds; otherwise `TimeOutOfRange { field: "wall_time_ms" }` |
| Memory bytes | Round **up** to KiB using the earlier `memory_kib_ceil()` helper; require 1..=i32::MAX KiB, otherwise `MemoryOutOfRange` |

Check process count, CPU, wall, then memory. Never clamp an oversized value,
truncate integer bits, substitute a default or silently omit a limit.

The `cpu_seconds` and `wall_seconds` strings must contain exactly three decimal
places, calculated with integer arithmetic. Examples:

| Milliseconds | Seconds string |
| --- | --- |
| 1 | `0.001` |
| 25 | `0.025` |
| 1000 | `1.000` |
| 2501 | `2.501` |
| 2147483647 | `2147483.647` |

This is exact decimal argument formatting, not a guarantee of timer scheduling
precision inside Isolate. CPU and wall time are different clocks; do not merge
them or change one based on the other.

Store validated memory KiB as `u32`, only after checking the signed 32-bit bound.
The 128 MiB default is 131072 KiB. One byte becomes one KiB, and 1025 bytes become
two. `u64::MAX` bytes must be rejected as unrepresentable by Isolate, without
arithmetic overflow. The earlier helper already solves overflow-safe rounding.

`arguments()` is implemented for you. It produces CPU, wall, cgroup-memory and
one-process options. **Do not map `output_bytes` to `--fsize`.** Stdout/stderr
are pipes; file-size limits do not enforce their combined stream budget. The
RunningSandbox exposes the configured output budget for issue #7's supervisor.

## Rust hints

- Borrowing `&ExecutionLimits` lets you inspect limits without consuming them.
- `Duration::as_millis()` returns `u128`; a cast must not silently discard bits.
- Validate ranges before narrowing, or use checked integer conversion.
- Quotient/remainder can split milliseconds into whole seconds and the remainder.
  Rust formatting supports zero-padding the remainder to three digits.
- Return the specific typed error at the first failed validation, then move the
  completed strings/integer into `Self` after every check succeeds.
- Keep all arithmetic integer-based. Floating point is unnecessary here.

Read the [lifecycle guide](../sandbox-lifecycle.md), your completed issue #2 memory
helper, then [`sandbox_limits_exercise.rs`](../../crates/taskforge-execution/tests/sandbox_limits_exercise.rs).
Protocol tests use fake tools but call the same public preparation and completed
conversion as real callers.

## Run the exercise

```sh
cargo test -p taskforge-execution --offline --locked \
  --test sandbox_limits_exercise
```

All six pass as normal regression tests. To select one test:

```sh
cargo test -p taskforge-execution --offline --locked \
  --test sandbox_limits_exercise fractional_seconds_preserve_milliseconds_exactly \
  -- --exact
```

The mapping-only example does not execute any submission:

```sh
cargo run -p taskforge-execution --offline --locked --example isolate_limits
```

It prints the four default flags, including `--time=1.000`, `--wall-time=3.000`
and `--cg-mem=131072`. Host-dependent tests stay explicitly ignored by default;
formatting, Clippy and the full normal workspace suite also pass.

## Real verification

Run the real test on the provisioned native host
as the ordinary judge user, outside an outer filesystem/privilege sandbox:

```sh
TASKFORGE_REAL_CONFIG="$PWD/examples/judge.local.json" \
  cargo test -p taskforge-execution --offline --locked --test sandbox_real \
  -- --ignored --nocapture
```

This checks actual execution of three fixtures, subprocess/network denial,
read-only filesystem behavior, trusted-file inaccessibility and repeated reuse
of one slot with fresh contents. The passing run is recorded in the
[host verification record](../setup/issue-6-host-verification.md). A skip or
fake-tool test is not real isolation evidence. Broader cancellation/output and
exhaustion verification remains in the later supervision/integration issues.

## Explain it back

Explain why a valid u64 configuration number might be unusable by Isolate, why
milliseconds and KiB need different conversions, why output pipes need their own
supervisor, and why launching with a missing/rounded-down/clamped limit would
break the execution contract. Also explain how holding the slot lease until
cleanup prevents another test from resetting an active sandbox.

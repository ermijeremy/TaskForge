# Issue #8 — Core exercise: classify execution evidence

**Status: complete and reviewed.** Branch `issue/8-ordered-evaluation` implements
ordered evaluation, raw-byte token comparison, aggregation and the concrete
`IsolateRunner`. The owner implemented the classification exercise, and review
corrected its remaining boolean conditions, decision priority and missing
outcomes. The completed
[`classify_execution`](../../crates/taskforge-execution/src/runner/classification.rs)
is used by every evaluation; nine classification tests and the two ordered
protocol tests now run normally. Full native runner verification passed.

The implementation contract and examples are retained below.

## Inputs and return value

The function takes a supervisor `output_limit_exceeded` flag and optional trusted
`SandboxMetadata`. Return `Ok(ExecutionDecision::...)` for a user-code outcome,
and `Err(InfrastructureError::new(reason, "bounded trusted explanation"))` for
an infrastructure failure. Keep diagnostics static; do not add raw output or
private data. You can import `SandboxStatus` and `TimeoutKind` from `crate::sandbox`.

`ExecutionDecision::CompareOutput` means the process completed successfully and
the evaluator may compare stdout. It does **not** mean the solution is Accepted:
wrong output may still fail, and all later cases must pass too. The other enum
variants map to compatible verdict/reason pairs for you.

## Required decisions

First reject invalid trusted state; infrastructure failures take precedence over
user verdicts even when output excess is also reported:

| Evidence | Required infrastructure reason |
| --- | --- |
| Metadata status `InternalError` | `SandboxSetupFailed` |
| Both exit code and exit signal present | `MetadataInvalid` |
| No status, no exit code and no exit signal | `MetadataInvalid` |
| `timeout_kind` present while status is not `Timeout` | `MetadataInvalid` |

Those structural checks apply when metadata exists. After rejecting such state,
apply the following ordered rules. The first matching rule wins:

| Evidence | Return |
| --- | --- |
| Supervisor output-limit flag is true | `OutputLimit`, even if metadata is absent |
| Metadata absent without that flag | `Err` with `MetadataMissing` |
| `oom_killed` is true | `MemoryLimit` |
| Status `Timeout`, kind `Cpu` | `CpuTimeLimit` |
| Status `Timeout`, kind `Wall` | `WallTimeLimit` |
| Status `Timeout`, kind absent | `UnspecifiedTimeLimit` |
| Status `RuntimeError` or `Signal`, any exit signal, nonzero exit code, or otherwise unexplained `killed` | `RuntimeError` |
| Clean exit code zero without the preceding failure indicators | `CompareOutput` |

`SandboxMetadata::parse` already rejects malformed files. The function still
checks structural invariants because callers can construct the public metadata
struct directly. Unknown status strings are rejected by the parser, not guessed.

Do not infer a memory limit from signal 9, large RSS, or the word `MemoryError`.
Only `oom_killed` is the confirmed cgroup evidence. Do not infer a timeout merely
because a measurement looks large. Only reported timeout status selects a time
verdict. The metadata parser preserves the clock named in Isolate's trusted
message; an unknown/missing clock uses the generic `reported_time_limit_exceeded`
reason without inventing a cause or a measurement.

Both Python syntax errors and exceptions become `RuntimeError` with
`program_failed`. The classifier receives no stderr and must not guess a Python
error type from untrusted text. A program can print the word `SyntaxError` itself.

## Examples

- Output flag + OOM flag + timeout status: `OutputLimit`.
- OOM flag + timeout status, with no output flag: `MemoryLimit`.
- Signal 9 without OOM evidence: `RuntimeError`.
- Missing metadata after a supervisor output kill: `OutputLimit`.
- Missing metadata after ordinary completion: infrastructure `MetadataMissing`.
- Successful exit: `CompareOutput`; the evaluator decides Wrong Answer or continues.

Useful Rust patterns are `Option::is_some`, `Option::is_some_and`, early returns,
`match`, and `matches!`. Preserve the input metadata. Read the nine exercise tests
before changing the function; no sandbox is needed to run them.

## Commands

Run the nine classification regression tests:

```sh
cargo test -p taskforge-execution --offline --locked \
  --test classification_exercise
```

Then run the two ordered-evaluation protocol tests. These use trusted tool
doubles and execute no submitted Python:

```sh
cargo test -p taskforge-execution --offline --locked --lib \
  ordered_runner_
```

They check stored order, no execution after the first mismatch, counts/indexes,
aggregation including the failing test, absence of private data in diagnostics,
infrastructure propagation, cleanup/reuse and the synchronous `Runner` interface.
All eleven former owner-dependent checks now run in the normal suite. Native
tests remain opt-in.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --offline --locked -- -D warnings
cargo test --workspace --offline --locked
```

On the provisioned native host, outside an outer sandbox, run the new full runner
test explicitly:

```sh
TASKFORGE_REAL_CONFIG="$PWD/examples/judge.local.json" \
  cargo test -p taskforge-execution --offline --locked \
  --test ordered_evaluation_real -- --ignored --nocapture
```

It covers all three accepted solutions, whitespace output, the known wrong
solution, syntax/exception, CPU/wall, memory and three output fixtures, followed
by another accepted evaluation. Memory exhaustion may legitimately produce
Python `MemoryError`/Runtime Error instead of a confirmed OOM kill. The pure
exercise verifies the distinction; the native test allows those two outcomes.
**Full ordered native evaluation passed**, including a confirmed cgroup OOM kill
for the memory-pressure fixture in the recorded run.

See the [runner guide](../ordered-evaluation.md) and
[verification status](../setup/issue-8-verification.md).

## Explain it back

Why does a successful process still need output comparison? Why is a timeout
status different from a large CPU measurement? Why must missing metadata after
an output kill be handled differently? Explain why a killed program is not
necessarily an OOM kill, why all infrastructure errors stay outside user verdicts,
and why the first failing case contributes measurements but not a passed count.

# Issue #5 — Core exercise: validate the final evaluation result

**Status: completed.** The project owner implemented this core construction
boundary and its regression tests now run in the normal suite. The original
contract and reasoning are retained below.

## Your implementation

The completed `TryFrom<EvaluationResultFields> for EvaluationResult` is in
[`evaluation.rs`](../../crates/taskforge-execution/src/evaluation.rs). It validates
the candidate before moving its unchanged fields into the private wrapper.

The candidate has a verdict, reason code, counts, optional failing index,
resource measurements and a bounded diagnostic. The private wrapper represents
a final result whose fields agree. Invalid combinations must return a typed
`ResultValidationError`. Do not repair or silently change the supplied values.

Validate in this order; it determines which error wins when multiple fields fail:

1. If `total_tests` is zero, return `EmptyTestSuite`.
2. If `passed_tests` exceeds `total_tests`, return `PassedExceedsTotal`.
3. If the reason code is incompatible with the verdict, return `ReasonVerdictMismatch`.
4. For Accepted, require all tests passed and `first_failing_test == None`;
   otherwise return `InvalidAcceptedProgress`.
5. For every failing verdict, require fewer passed than total tests and
   `first_failing_test == Some(passed_tests)`; otherwise return `InvalidFailureProgress`.
6. Only after validation, construct the private wrapper, preserving every field.

Allowed pairs:

| Verdict | Reasons |
| --- | --- |
| `Accepted` | `AllTestsPassed` |
| `WrongAnswer` | `OutputMismatch` |
| `TimeLimitExceeded` | `CpuTimeLimitExceeded` or `WallTimeLimitExceeded` |
| `MemoryLimitExceeded` | `ConfirmedMemoryLimitKill` |
| `OutputLimitExceeded` | `CombinedOutputLimitExceeded` |
| `RuntimeError` | `PythonSyntaxError` or `ProgramFailed` |

Zero-based failing positions follow from stopping at the first failure: if test
positions 0 and 1 pass and position 2 fails, `passed_tests` is 2 and the failing
index is `Some(2)`. If all tests pass, there is no failing index.

Do not change unavailable measurements to zero, remove a supplied diagnostic,
invent a verdict, or infer a memory kill. Your function validates a proposed
result; it does not execute Python, compare output, parse sandbox metadata, or
implement the later verdict classifier.

## Rust hints

- `TryFrom` models a conversion that can fail; `From` would promise infallibility.
- `match` on enums/pairs is useful for expressing compatible variants. Keep the
  compatibility check separate from count/index rules so errors are predictable.
- `Option<u32>` distinguishes an absent failing index from the actual position 0.
- You own the candidate, so it can move into the wrapper after your checks.
- Relational checks avoid needing arithmetic near `u32::MAX`.
- The private wrapper prevents callers from bypassing this constructor. Do not
  expose its field or add an unchecked public constructor to satisfy the tests.

Read the [contract guide](../evaluation-contract.md), then the types above the
exercise and [`evaluation_exercise.rs`](../../crates/taskforge-execution/tests/evaluation_exercise.rs).
There is no hidden implementation of this validation elsewhere. Both the public
regression tests and normal serialization fixtures use the validated constructor.
Accepted and failing progress checks occupy mutually exclusive branches: every
non-Accepted verdict uses the failure rules, including resource/runtime failures.

## Run the regression tests

From the repository root:

```sh
cargo test -p taskforge-execution --offline --locked --test evaluation_exercise
```

All nine tests run normally. The original eight exercise tests cover valid results,
all verdict/reason pairings, error precedence, counts, failing positions, maximum
values and preservation of diagnostics/measurements. An additional regression
rejects all-tests-passed progress for every failing verdict. The unrelated
real-host readiness test remains ignored by default.

For the complete checks and the schema demonstration:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --offline --locked -- -D warnings
cargo test --workspace --offline --locked
cargo run -p taskforge-execution --offline --locked --example result_contract
```

The last command prints one JSON object containing four synthetic result/error
examples. It never executes submitted code.

## Explain it back

Explain why a constructor matters even when all fields have valid Rust types;
why a Runtime Error is `Ok(EvaluationResult)` while sandbox setup failure is
`Err(InfrastructureError)`; why an index is optional; and why a measured zero
and an unavailable measurement differ. Also explain why this validator must not
silently change inconsistent fields into something that happens to pass.

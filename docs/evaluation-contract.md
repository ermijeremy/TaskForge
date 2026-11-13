# Evaluation library contract (issue #5)

**Status: implemented, including the completed core result-validation exercise.**
`EvaluationResult::try_from` validates the public construction boundary and its
regression tests run in the normal suite. No concrete submission runner exists
yet; issues #6–#8 implement the judging pipeline.

## Calling boundary

```rust
pub trait Runner {
    fn evaluate(
        &self,
        source: &str,
        problem: &Problem,
        limits: &ExecutionLimits,
    ) -> Result<EvaluationResult, InfrastructureError>;
}
```

Source, the validated problem and per-test limits are borrowed. The implementation
will own host configuration and the whole-evaluation infrastructure deadline.
The contract is synchronous and callable through `&dyn Runner`. A future async
worker must put blocking evaluation on an appropriate blocking thread. There is
no HTTP, SQL, CLI invocation, Tokio dependency, retry policy or process fallback.
Issues #6–#8 supply concrete execution, supervision and ordered evaluation.

`Ok(result)` means a valid completed user-code evaluation, including a failing
submission. `Err(error)` means trusted infrastructure could not produce such an
evaluation. Missing/corrupt metadata, setup failure, cancellation, cleanup failure
and the whole-evaluation deadline belong to the error side. A program exceeding
its own CPU/wall limit belongs to a user-code Time Limit Exceeded verdict.

Future workers can match `error.reason_code()` directly; retry policy remains
separate. A result-construction validation error indicates invalid trusted judge
state and must not be turned into a Runtime Error verdict for a submission.

## Result fields and units

| Field | Meaning |
| --- | --- |
| `verdict` | One of the six snake_case user verdicts |
| `reason_code` | Stable snake_case explanation compatible with that verdict |
| `passed_tests` | Number of tests passed before evaluation stopped |
| `total_tests` | Total stored tests for the selected problem, greater than zero |
| `first_failing_test` | Zero-based stored position; `null` for Accepted |
| `resources.cpu_time_ms` | Sum of attempted test CPU times, in integer milliseconds |
| `resources.wall_time_ms` | Sum of attempted test wall durations, in integer milliseconds |
| `resources.peak_memory_bytes` | Maximum reported per-test peak, in bytes |
| `diagnostic` | Trusted explanation capped at 4096 UTF-8 bytes |

Attempted tests include the first failing test. Resource values exclude setup and
cleanup. Measurements unavailable for an attempted test must keep the respective
aggregate `None`/JSON `null`; a partial sum/maximum cannot be presented as complete.
A genuine measured zero is `Some(0)`, serialized as `0`. The later runner owns
aggregation, unit conversion and overflow handling; this issue defines the values,
not that aggregation implementation.

`EvaluationResultFields` is the candidate assembled by trusted code. Its public
fields make assembly explicit, but constructing/serializing it does not validate
it. The runner's return type is the private-field `EvaluationResult` wrapper.
Its only public construction path is the validated `TryFrom` conversion. A borrowed
`fields()` getter permits inspection without mutating the validated result.
Neither the wrapper nor the diagnostic type offers unchecked deserialization.

## Verdict and reason compatibility

| Verdict | Allowed reason codes |
| --- | --- |
| `accepted` | `all_tests_passed` |
| `wrong_answer` | `output_mismatch` |
| `time_limit_exceeded` | `cpu_time_limit_exceeded`, `wall_time_limit_exceeded` |
| `memory_limit_exceeded` | `confirmed_memory_limit_kill` |
| `output_limit_exceeded` | `combined_output_limit_exceeded` |
| `runtime_error` | `python_syntax_error`, `program_failed` |

Python has no Compilation Error variant. Memory-limit classification requires
trusted sandbox evidence; an unexplained signal or Python `MemoryError` alone is
insufficient. This table validates a proposed verdict/reason pair. Metadata
classification, output comparison, and verdict precedence remain issue #8.

Sequential evaluation stops at the first failing test. Accepted requires
`passed_tests == total_tests` and no failing index. Every other verdict requires
`passed_tests < total_tests` and `first_failing_test == Some(passed_tests)`.
For example, one pass followed by a failure gives `passed_tests = 1` and failing
position `1`, not `2`. See the exercise guide for validation order and error types.

## Infrastructure errors and diagnostic limits

`InfrastructureError` is a separate serializable Rust error with `reason_code`
and `diagnostic`, and no user verdict. Codes cover invalid configuration,
unavailable sandbox, failed setup, missing/invalid metadata, process I/O failure,
overall deadline expiry, cancellation, cleanup failure and internal failure.
Infrastructure errors do not fabricate user verdicts, progress or resource values.

Both result and infrastructure diagnostics use `BoundedDiagnostic`. Its
constructor copies at most 4096 bytes, backing up to a UTF-8 character boundary.
Truncation adds no suffix; the stored text is a valid prefix of the input. The
byte bound applies before JSON escaping; the encoded JSON string may be larger.
This is a storage bound, not a secrecy filter or output supervisor. Callers must
supply trusted explanations and keep private inputs, expected outputs, credentials
and unfiltered user stdout/stderr out of this field.

## Serialization examples

Synthetic schema examples, not actual judging results or measurements:

- [Accepted](../examples/results/accepted.json)
- [Wrong Answer](../examples/results/wrong-answer.json)
- [Unavailable measurements](../examples/results/unavailable-measurements.json)
- [Infrastructure failure](../examples/results/infrastructure-failure.json)

The snapshots show the separate result/error payload shapes. This issue does not
define a CLI/HTTP envelope or external exit-code policy; those arrive later.
Normal unit tests construct their schema fixtures through the same validated
`TryFrom` conversion used by library callers and compare serialized payloads
with these snapshots.

The public-API example emits all four synthetic payloads in
one JSON object, without executing any submission:

```sh
cargo run -p taskforge-execution --offline --locked --example result_contract
```

The only Runner implementation in tests is a fake unavailable backend that
returns a typed infrastructure error without launching processes.

## Verification state

The normal suite covers schema, bounded diagnostics, null/zero measurements,
the typed library boundary, result validation and existing regression checks.
Nine completed exercise regressions cover all allowed verdict/reason pairs,
incompatible pairs, invalid counts, Accepted/failure progress, maximum counts,
and preservation of supplied fields. Every failing verdict is explicitly checked
against a misleading all-tests-passed state.

Run the completed exercise tests with:

```sh
cargo test -p taskforge-execution --offline --locked --test evaluation_exercise
```

Only the existing real-host readiness test remains ignored by default. The
[exercise guide](learning/issue-5.md) retains the contract and reasoning.

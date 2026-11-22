# Issue #9 — Core exercise: JSON reports and exit codes

**Status: complete.** The JSON-reporting function was completed at the owner's
request. Its six regression tests and CLI outcome tests run normally, and native
CLI verification passed. The implementation contract and examples remain below.

The completed function is
[`report_outcome`](../../crates/judge-local/src/reporting.rs). It serializes the
library result/error without modifying fields and chooses exit code zero or
three. No placeholder or alternate judgment serializer remains.

## Contract

The input is the library's `EvaluationOutcome`:

```rust
Result<EvaluationResult, InfrastructureError>
```

Your output is a different result:

```rust
Result<JudgingReport, ReportError>
```

An infrastructure failure in the input must still produce **Ok(report)**. It is
an error that the CLI can describe successfully. The outer `Err(ReportError)` is
reserved for a serialization/report-bound failure, not for a user's failing
solution or an already typed infrastructure error.

| Library input | Bytes in the report | Report exit code |
| --- | --- | --- |
| `Ok(result)` with any verdict | Serialize the complete `EvaluationResult` | `EVALUATED_EXIT_CODE` = 0 |
| `Err(error)` | Serialize the complete `InfrastructureError` | `INFRASTRUCTURE_EXIT_CODE` = 3 |

Wrong Answer, Runtime Error and resource-limit verdicts are completed evaluations,
so they all use exit code zero. The caller reads `verdict` to distinguish them.
Exit three means the judge could not produce a valid evaluation.

Use compact `serde_json::to_vec` serialization rather than building JSON strings
manually. Preserve counts, reasons, diagnostics and measurements exactly. In
particular, unknown measurements stay JSON null and measured zero stays zero.
Do not invent a verdict or progress fields for an infrastructure error, add an
envelope, truncate a serialized document, or convert integers to floating point.

The `JudgingReport` fields are private to this module. You can construct it in
this function with a struct literal containing `json` and `exit_code`. Its
getters let the executable write the bytes and return the code.

After serialization, reject more than `MAX_JSON_BYTES` (32 KiB) with
`ReportError::TooLarge`. The bound allows worst-case JSON escaping of the existing
4 KiB diagnostic. Forward a serde failure as `ReportError::Serialization`; the
provided `From` conversion lets `?` do that for you.

The function performs no printing or file I/O. Return one compact JSON document
without a trailing newline. The executable appends exactly one newline and writes
no headings or submitted output to stdout. The supplied source never enters this
report type; output privacy remains enforced by the execution library.

## Examples

- Accepted with all eight cases passed: result JSON, exit 0.
- Wrong Answer after one pass: result JSON preserving failing index 1, exit 0.
- Confirmed CPU timeout: Time Limit Exceeded result JSON, exit 0.
- Sandbox unavailable: error JSON containing its reason and diagnostic, exit 3.
- A missing measurement: null, not zero.

Read the six tests before replacing the placeholder. A `match` on the input is
useful. Both branches need a common report type even though result and error are
different Rust types; serializing each to `Vec<u8>` gives common byte storage.

## Commands

Run the six reporting regressions:

```sh
cargo test -p judge-local --offline --locked \
  --test reporting_exercise
```

Then run CLI argument/input checks, including the owner-dependent error report:

```sh
cargo test -p judge-local --offline --locked \
  --test judging_cli
```

All seven former owner-dependent checks now run normally. Keep native checks opt-in.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --offline --locked -- -D warnings
cargo test --workspace --offline --locked
```

Explicitly run the real CLI checks on the supported native
host, outside an outer sandbox:

```sh
TASKFORGE_REAL_CONFIG="$PWD/examples/judge.local.json" \
  cargo test -p judge-local --offline --locked --test judging_real \
  -- --ignored --test-threads=1
```

The readiness JSON test is independent of the exercise and already passed. The
full judging test also passed: it checks actual CLI verdicts, zero exit codes
for valid failures, a typed startup failure with exit three, stdout JSON framing,
private-data exclusion and cleanup/reuse. A guard check is not a native judging
pass. See the [verification status](../setup/issue-9-verification.md).

## Explain it back

Why does Wrong Answer use the same exit code as Accepted? Why does a library
`Err(InfrastructureError)` become `Ok(JudgingReport)`? Explain why a serialization
failure is a different error layer, why JSON escaping can make a diagnostic
larger, and why printing from the report function would break the CLI's framing.

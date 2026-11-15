# Ordered evaluation and verdicts

Issue #8 adds a concrete `IsolateRunner` around the issue #7 supervisor. Its core
classification exercise is complete, all normal checks pass, and native full-runner
verification passed. The CLI remains configuration/readiness only until issue #9.

## Main parts

| File | Responsibility |
| --- | --- |
| `runner/mod.rs` | Startup, sync/async entry points, shared evaluation session, ordered cases, early stop and validated final result |
| `runner/classification.rs` | Completed classifier selecting the execution decision from supervisor/metadata evidence |
| `runner/comparison.rs` | Exact raw-byte token comparison with six ASCII whitespace separators |
| `runner/resources.rs` | CPU/wall duration sums, peak memory, unknown propagation and checked conversion |
| `sandbox/metadata.rs` | Bounded trusted metadata parsing and optional timeout clock, without retaining arbitrary message text |
| `evaluation.rs` | Result/error schema and compatible verdict/reason validation |

## Calling the library

An ordinary blocking thread can use the existing object-safe contract:

```rust,ignore
use taskforge_execution::{ExecutionConfig, IsolateRunner, Runner, load_problems};
let config = ExecutionConfig::load("examples/judge.local.json")?;
let problems = load_problems("fixtures/problems")?;
let runner = IsolateRunner::new(config)?;
let problem = &problems[0];
let result = runner.evaluate(source, problem, problem.limits())?;
```

A Tokio caller uses `IsolateRunner::initialize(config).await` and
`runner.evaluate_async(source, problem, limits).await`. Synchronous entry points
reject a Tokio context with `InvalidConfiguration`; they do not nest runtimes or
block its executor. The sync adapter uses a temporary time-enabled runtime and
shuts it down in the background so a deadline response does not wait for late
recovery. Started cleanup workers retain process/lease ownership as in issue #7.

Initialize once and reuse the runner. Every evaluation creates exactly one
session whose absolute budget spans all its tests, including setup and cleanup.
The explicitly passed limits govern every test; `problem.limits()` supplies the
problem's policy. The current backend supports the active Python 3.14.10 runtime;
archived Python 3.14.8 definitions remain loadable but cannot be evaluated on it.
Unsupported limits/source sizes are rejected before reserving a box.

## Evaluation flow

For each case in its stored array order:

1. Check the shared work deadline and request one fresh sandbox run with the
   selected input only. Expected output stays in trusted memory.
2. Await execution and cleanup. Propagate setup, process I/O, metadata,
   cancellation, deadline and cleanup errors as infrastructure failures.
3. Record CPU/wall/memory evidence from the attempted case, including a failure.
4. Invoke the one core classifier. Only `CompareOutput` permits comparison.
5. Compare stdout tokens off the async executor, with the same absolute response
   deadline covering blocking-pool queue time. Comparison owns only bounded output
   and the case's bounded expected bytes, with no active sandbox.
6. Stop immediately on the first failure. Otherwise continue and report Accepted
   only when every case passed. Validate the result through `EvaluationResult::try_from`.

First-failure indexing is zero-based: a pass at position 0 and failure at position
1 gives `passed_tests=1` and `first_failing_test=1`. No later case starts. Diagnostic
text contains a static explanation plus typed, bounded evidence flags; it never
copies raw stderr, actual/expected output, hidden inputs, source or host paths.

Comparison splits raw bytes on space, tab, LF, CR, vertical tab and form feed.
Empty/whitespace-only outputs are equal. Tokens preserve order and must match
byte-for-byte: `05` differs from `5`, `1.0` differs from `1`, and Unicode nonbreaking
space does not act as a separator. Stderr contributes to the shared output budget
but never participates in answer comparison. Invalid UTF-8 is compared as bytes.

## Evidence and aggregation

Infrastructure errors and invalid trusted metadata always remain errors. User
execution precedence is output excess, confirmed OOM kill, reported timeout,
other reported failures, then output comparison. The
[core exercise](learning/issue-8.md) gives the exact contract and examples.

CPU/wall timeout clocks are taken from the two exact trusted messages emitted by
[pinned Isolate](https://github.com/ioi/isolate/blob/v2.7/isolate.c). Unknown messages
preserve timeout status but not a fabricated clock: the new
`reported_time_limit_exceeded` reason is compatible with Time Limit Exceeded.
No arbitrary Isolate message is retained in public diagnostics. Python syntax
errors/exceptions use Runtime Error / `program_failed`; untrusted stderr is never
parsed to guess the error type. Specific `python_syntax_error` remains an allowed
schema reason but this implementation does not infer it.

CPU/wall durations sum before flooring to integer milliseconds, so 0.7 ms +
0.7 ms reports 1 ms, not zero. Peak memory is the largest per-test reported value.
If a field is missing for any attempted test, its entire aggregate is null and
stays unknown. Supervisor output kills may have no metadata, making all three
aggregate fields unknown. Measured zero remains zero. Arithmetic overflow becomes
`MetadataInvalid`, never a wrapped number, partial sum or user-code failure.

See the [verification record](setup/issue-8-verification.md) for actual checks.

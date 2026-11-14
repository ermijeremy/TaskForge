# Issue #7 — Core exercise: one stdout/stderr budget

**Status: complete; native supervision verification pending.** The accounting
function was corrected at the owner’s request. Branch `issue/7-bounded-supervision`
supplies supervision, shared deadlines, cancellation and cleanup ownership. The
completed function is
`OutputBudget::record` in
[`supervision/budget.rs`](../../crates/taskforge-execution/src/supervision/budget.rs).
This is the only output-accounting implementation: every chunk from both pipes
passes through it. The seven public tests, private overflow test and three
supervised protocol tests now pass as normal regressions. The original
implementation contract is retained below.

## Implementation contract

The method updates the fields and returns `Ok(())`. Output excess is a recorded
condition: `exceeded` tells the supervisor to terminate the whole sandbox. The
`ExercisePending` variant was the original scaffold placeholder; the completed
accounting method returns `Ok(())` for all chunks.

| Field | Contract |
| --- | --- |
| `capture.observed_bytes` | Total bytes read from both streams, including discarded bytes. Saturate at `u64::MAX`; never wrap. |
| `exceeded` | Set when combined observed output is **strictly greater** than `limit`, or counting overflows. Never reset it. Equality is allowed. |
| `capture.stdout`, `.stderr` | Preserve raw bytes in their respective stream. Retain only the incoming prefix that fits the remaining combined retention allowance. |
| Combined retained length | At most `min(limit, MAX_CAPTURE_BYTES)`, where the hard ceiling is 1 MiB. |

An empty chunk is a no-op and returns `Ok(())`. After excess, later chunks still
increase the observed counter (with saturation) but retain no additional bytes.
Do not decode UTF-8, append an entire chunk before truncating it, reserve memory
based on the whole chunk, or allocate based on a potentially enormous `u64` limit.
Validate or bound values before narrowing them to `usize`.

Examples with a limit of five bytes:

| Reads in order | Retained stdout | Retained stderr | Observed | Exceeded |
| --- | --- | --- | --- | --- |
| stdout `abc`, stderr `de` | `abc` | `de` | 5 | false |
| then stderr `f` | `abc` | `de` | 6 | true |
| stderr `12`, stdout `abcdef` in a fresh budget | `abc` | `12` | 8 | true |

The retention ceiling and configured output limit are distinct. A standalone
`OutputBudget` with a huge limit retains at most 1 MiB while continuing to count.
The supervisor itself rejects limits above 1 MiB before launch, so output from
an otherwise successful test is fully available for the later output comparer.

Useful Rust tools include `checked_add`, `saturating_add`, `saturating_sub`, `min`,
a `match` on `OutputStream`, and `extend_from_slice` on a bounded slice. The
private overflow test seeds the counter near its maximum without producing
exabytes of output. The tests document the boundary cases.

## Commands

Run the seven public regression tests and private overflow test:

```sh
cargo test -p taskforge-execution --offline --locked \
  --test output_budget_exercise
cargo test -p taskforge-execution --offline --locked --lib \
  count_overflow_records_excess_instead_of_wrapping
```

Run the three supervision protocol tests. They use trusted
shell doubles and execute no submitted Python:

```sh
cargo test -p taskforge-execution --offline --locked --lib \
  supervised_
cargo test -p taskforge-execution --offline --locked --lib \
  explicit_and_dropped_waiter_cancellation
```

These eleven tests run normally. Keep native-host tests ignored by default.
Check the workspace:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --offline --locked -- -D warnings
cargo test --workspace --offline --locked
```

On the provisioned native host, outside an outer sandbox, run:

```sh
TASKFORGE_REAL_CONFIG="$PWD/examples/judge.local.json" \
  cargo test -p taskforge-execution --offline --locked --test supervision_real \
  -- --ignored --nocapture
```

The native test uses one dedicated configured slot. It checks stdout-only,
stderr-only and combined excess, real CPU/wall timeout evidence, the separate
evaluation deadline, explicit cancellation, aborted waiters and subsequent slot
reuse. **Native supervision has not yet been verified on this branch.**
A skip is not evidence of isolation. See the [supervision guide](../supervision.md)
and [verification status](../setup/issue-7-verification.md).

## Explain it back

Why must both pipes use one ledger? Why must equality remain allowed? Why is the
number of observed bytes different from the number retained, and from bytes the
process may have written before being killed? Why does a timeout on a Tokio
blocking task still require explicit cancellation and sandbox cleanup? Explain
why a supervisor output kill is evidence for a later verdict, while an evaluation
deadline is an infrastructure error.

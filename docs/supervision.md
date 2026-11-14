# Bounded execution supervision

Issue #7 adds the Tokio-facing supervisor around the issue #6 Isolate lifecycle.
The core `OutputBudget::record` exercise is complete. Accounting and supervised
protocol tests run in the normal suite; native supervision verification has
also passed on the supported runtime. The CLI still provides configuration/readiness commands; verdict
evaluation is issue #8.

## Main parts

| File / type | Responsibility |
| --- | --- |
| `supervision::Supervisor` | Initialize the immutable backend on Tokio's blocking pool; create evaluation sessions. |
| `EvaluationSession` | Own one absolute evaluation deadline and cancellation flag across ordered tests; allow one active test. |
| `EvaluationTask` | Await the worker, signal cancellation on drop/abort, and bound the response by the absolute deadline. |
| `supervision::execute` | Own setup, fair pipe reads, wrapper observation, metadata and explicit cleanup on a blocking worker. |
| `OutputBudget::record` | Completed exercise: combined byte accounting and bounded retention for every read. |
| `execution_control.rs` | Shared cancellation and work/cleanup deadline checkpoints. |
| `sandbox/mod.rs` | Poll control processes, stage source/input, request manager shutdown, reap it, clean the sandbox and quarantine failed cleanup. |

Create `Supervisor::initialize(config).await` once at startup. Readiness and the
cached runtime image are service initialization, before evaluations are admitted.
Use a Tokio runtime with time enabled. `from_backend` is available for an already
initialized backend; its construction must happen off the async executor too.

Create one `evaluation()` session per submission, then reuse it across ordered
tests. A new session per test would reset the overall deadline incorrectly.
Source and input are bounded before owned copies are made. The blocking worker
checks its shared deadline even if it spent time queued in Tokio's pool. Busy
sessions reject a second active test; sandbox slot acquisition remains nonblocking
and reports exhaustion explicitly.

## Output capture

Both standard output pipes are set nonblocking using safe rustix file-descriptor
operations. One worker reads at most 8 KiB from each stream per iteration,
checking cancellation and the shared budget between reads. It polls every 2 ms
when running. Neither a blocked stderr writer nor continuously ready stdout can
prevent the other stream from being serviced. There are no unbounded channels,
reader queues or per-read allocations. Retained bytes belong to the ledger;
stdout/stderr ordering is preserved within each stream, not across streams.

The configured combined budget is at most 1 MiB in this supervision path. Larger
values are rejected explicitly before launch. The ledger independently caps
retention at 1 MiB, even for an enormous standalone limit. Its observed counter
counts only bytes actually read; excess detection can overshoot by one 8 KiB
chunk. Unread kernel-pipe bytes are not invented as a measurement.

After excess, the supervisor requests SIGTERM so Isolate can terminate its
sandbox processes, then reaps the manager and invokes Isolate cleanup. It waits
up to 250 ms within the remaining cleanup budget before force-killing an
unresponsive manager. Wrapper termination alone never counts as cleanup. This
follows the pinned [Isolate shutdown handler](https://github.com/ioi/isolate/blob/v2.7/isolate.c).
The supervisor returns bounded raw output and explicit
`output_limit_exceeded` evidence. Metadata is absent after that forced kill;
measurements are not fabricated. A known fatal wrapper exit remains an
infrastructure error. Issue #8 will turn raw evidence into verdicts and compare
stdout against trusted expected output. Captured data is trusted runner data,
not a public API response.

## Deadlines and cleanup

CPU and wall limits still belong to each Isolate run. An Isolate timeout remains
raw sandbox metadata (`Timeout`) for later TLE classification. The independent
`evaluation_timeout_ms` starts at session creation, includes all tests, setup,
reads and cleanup, and produces `EvaluationDeadlineExceeded`, an infrastructure
error. Up to `min(timeout / 4, 2 seconds)` is reserved *inside* that budget for
cleanup; evaluation work stops at the earlier work deadline.

Cancellation interrupts work checkpoints but never disables cleanup. Dropping a
prepared sandbox, dropping a running sandbox, setup failure, metadata failure,
stream errors, output excess, explicit cancellation and dropping/aborting a
waiting future all retain a resource owner that attempts whole-sandbox cleanup.
Killing the wrapper alone never counts as cleanup. Locks remain owned until the
worker has finished its cleanup attempt; `session.is_idle()` becomes true only
then. Cancellation is permanent for a session: start a fresh session afterward.
Await `task.cancel(); task.wait().await` when you need the cleanup outcome. A
dropped waiter only requests cleanup; it cannot synchronously guarantee completion.

Tokio documents that a started [`spawn_blocking` task cannot be aborted](https://docs.rs/tokio/1.54.1/tokio/task/fn.spawn_blocking.html).
The implementation therefore uses an explicit atomic cancellation signal and
keeps process/lease ownership in the worker. At the absolute response deadline,
`wait()` signals cancellation and returns an infrastructure error. An unfinished
worker continues recovery with the slot still owned; this is not reported as a
completed cleanup or a reusable slot.

If the reserved cleanup budget is exhausted, the worker still attempts Isolate
cleanup with a separate bounded recovery guard (10 seconds for that control
command), then quarantines the ID even if late recovery succeeds. It returns no
successful result. This recovery can outlive the response deadline. Operators
must resolve failed cleanup and inspect a quarantined slot before removing its
marker; automatic reuse is forbidden. A pathological stuck kernel filesystem
operation cannot be interrupted by a Rust deadline. This is a bounded response
and cooperative process supervisor, not a hard real-time kernel guarantee.

All staging, process polling, metadata reads, teardown and runtime-image removal
run outside Tokio's async executor. Metadata remains capped at 16 KiB; returned
infrastructure diagnostics remain capped at 4 KiB. Trusted control-command
capture retains at most 16 KiB and has a 10-second local guard constrained by the
shared work/cleanup deadline during an evaluation.

See the [exercise](learning/issue-7.md) for commands and the
[verification record](setup/issue-7-verification.md) for what has actually run.

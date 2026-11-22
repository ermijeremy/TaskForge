# Phase 1 verification matrix

**Status: passed on 2026-11-22.** Phase 1 provides local Python judging, bounded
Isolate execution and deterministic JSON results without an HTTP service or a
database. Issues #9 and #10 complete the command interface and consolidated audit.

## Reproduce the run

Provision the [documented native host](linux-isolate.md), use its exact supported
runtime and a judge-owned configuration, and stop other users of the reserved
sandbox range. Run as the ordinary judge user, outside an outer privilege or
filesystem sandbox:

```sh
TASKFORGE_REAL_CONFIG="$PWD/examples/judge.local.json" \
  bash tools/verify-phase-1.sh
```

The driver fails on any unsuccessful command. It records runtime/host
observations, checks formatting/Clippy/build, runs normal tests, parses the Python
fixture inventory, checks readiness, explicitly runs all native tests serially,
and audits the final slot/process state. It never counts default-suite skips as
isolation evidence or clears a quarantine marker automatically.

The actual run passed:

- Rust 1.97.1, CPython 3.14.10 and Isolate 2.7, with the documented runtime layout.
- Formatting, Clippy with warnings denied and the all-target build.
- **130 normal tests passed**, with nine native checks skipped by default.
- **All nine native checks explicitly passed**: 139 executed test passes overall.
- Python inventory: 16 named sources, with 15 parsed and one intentional syntax
  error; the syntax-only checker executed none of them.
- The final audit acquired every configured allocation lock and found no box
  directories, box cgroups, quarantine markers or reserved-UID processes.

The local run's kernel, configuration, checked IDs and detailed metadata are kept
in `artifacts/phase-1.local.log`, which is deliberately ignored by Git. This
publishable record reports project requirements and outcomes. To retain a fresh
local log without exposing it in a commit:

```sh
mkdir -p artifacts
TASKFORGE_REAL_CONFIG="$PWD/examples/judge.local.json" \
  bash tools/verify-phase-1.sh > artifacts/phase-1.local.log 2>&1
```

## Scenario evidence

| Phase 1 scenario | Actual evidence | Test / check |
| --- | --- | --- |
| Correct solution for each problem | Accepted across all 30 stored cases | `ordered_evaluation_real`; `judging_real` |
| Known wrong solution | One pass, failure at index 1; later inputs not read | Native runner/CLI; ordered protocol trace |
| Whitespace-only output differences | Accepted; exact token order retained | Comparison unit tests; whitespace fixture in runner/CLI |
| Syntax error and exception | Runtime Error / program_failed | Both named fixtures through native runner and CLI |
| Infinite loop | CPU Time Limit Exceeded | Native runner, CLI and supervision clock assertion |
| Sleeping program | Wall Time Limit Exceeded with little CPU use | Native runner, CLI and supervision clock assertion |
| Memory pressure | Confirmed cgroup OOM produced Memory Limit Exceeded | Native memory fixture; pure classifier rejects guesses from signals/RSS |
| Stdout, stderr and mixed floods | Output Limit Exceeded with bounded capture; valid result exits zero | Native supervision, runner and CLI; output ledger boundary/overflow tests |
| Child process creation | `SUBPROCESS_DENIED` from the actual catalog fixture | Native catalog access audit |
| External network access | `NETWORK_DENIED` on explicit routing/permission denial | Native catalog access audit; timeout/refusal does not pass |
| Host-only file access | `HOST_FILE_DENIED` for a host-readable marker | Native catalog access audit and restricted filesystem probes |
| Read-only/runtime mount policy | Writes denied; host/private files unavailable; fresh box contents | `sandbox_real`; restricted image staging/filter checks |
| Cancellation / aborted waiter | Worker terminates sandbox and releases slot | Native `supervision_real`; cancellation-during-setup protocol checks |
| Overall infrastructure deadline | Infrastructure error rather than a user TLE, followed by reuse | Native `supervision_real` |
| Setup failure after initialization | Missing private staged interpreter rejected; prepared box cleaned | New native setup/error audit and normal launch-validation regression |
| Missing/corrupt metadata | Typed infrastructure error; no remaining box/cgroup | New native setup/error audit |
| Busy slot / unavailable sandbox | Explicit infrastructure error; no reset or host fallback | Native busy-slot audit; CLI incompatible-helper case; readiness/protocol tests |
| Reuse after success/error | Same dedicated ID succeeds after every tested failure | Native setup/error audit and existing runner/supervision checks |
| CLI result/error distinction | One compact JSON document, exit zero for user verdicts, three for infrastructure | Reporting regressions and native CLI checks |
| Diagnostics/measurements | Bounded trusted text; unknown stays null; no private test content | Schema, aggregation, reporting and ordered protocol tests |
| Cached runtime cleanup | Owned image and trusted temporary directory removed before CLI exit | Native CLI temporary-directory assertion; explicit shutdown tests |

Together the native CLI's thirteen fixture sources and the catalog access audit's
three probes exercise all sixteen named sources. For the host-file probe, the
harness substitutes its one fixed path constant with a temporary marker path.
It confirms the marker's contents on the host and makes its file/parent readable
by ordinary permissions; neither is mounted into the submission. No global host
marker or host Python installation is modified.

## Failure injection and the resulting fix

The new native audit temporarily renames the copied interpreter inside its own
private cached image. Before the fix, Isolate reported user Runtime Error 127.
The launch path now checks required staged runtime files and their access modes
before starting the process. A missing runtime is a trusted setup failure, and
the prepared sandbox still cleans up. Explicit user `SystemExit(127)` remains
Runtime Error, verified by both pure classification and a real native program.

The audit also removes/corrupts only its private metadata after a bounded benign
program exits, then checks the error and reuses the slot. Holding the allocation
lock tests exhaustion without touching another user's active sandbox. These
faults are confined to test-owned files; no production fault-injection switch or
unsafe execution fallback was added.

## Exit criteria and limits

All six Phase 1 checklist entries in [phase_1.md](../../phase_1.md) are supported
by the table above, the setup guide and the actual consolidated run.

Verification covers the tested paths on the documented native setup. It is not a
formal sandbox/kernel security proof. Arbitrary SIGKILL/crash recovery, stuck
kernel I/O, resource starvation by unrelated host workloads and system service
failure are not promised to recover automatically. Cleanup failure is an
infrastructure failure; quarantine requires operator inspection. The process
scan uses the pinned installation's reserved UID base 60000, checked against its
configuration. Successful cgroup removal and lock checks supplement that scan;
the tool is not a general-purpose host security auditor.

The evaluation work/cleanup budget, late cleanup recovery and separate CLI
shutdown grace are documented in the [supervision](../supervision.md) and
[CLI](../judge-local-cli.md) guides. Host executables/configuration remain trusted;
runtime presence checks are not cryptographic provenance checks. Native checks
must be rerun after relevant runtime, policy or host changes.

HTTP APIs, SQL queues, workers, retries and distributed recovery remain later
phases. No additional submission language or generic job framework was added.

# Issue #8 verification status

**Status: passed.** The classifier, ordered runner and native fixture evaluation
pass on the documented supported runtime. The completed exercise tests and
ordered protocol tests are normal regressions.

## Checks run

Passed:

- `cargo test --workspace --offline --locked`: **112 passed, 5 ignored**.
- Formatting and Clippy with warnings denied.
- Nine classification tests: all pass, covering infrastructure rejection,
  metadata consistency, output/OOM/timeout priority, runtime failure and success.
- Ordered protocol tests: stored order, first-failure stopping, counts/indexes,
  measurements including the failing case, private-data exclusion, error
  propagation, cleanup/reuse and the synchronous `Runner` interface.
- All five native-host checks explicitly invoked: **5 passed**. These include
  restricted image staging, readiness, restricted execution/access probes,
  supervision, and the new ordered-evaluation fixture test.

Native command, run outside the outer workspace sandbox on the supported host:

```sh
TASKFORGE_REAL_CONFIG="$PWD/examples/judge.local.json" \
  cargo test -p taskforge-execution --offline --locked \
  -- --ignored --test-threads=1 --nocapture
```

A default-suite skip is not verification evidence. The five host-dependent checks
remain opt-in so unsupported environments do not silently claim isolation.

## Native ordered-evaluation outcomes

| Scenario | Observed outcome |
| --- | --- |
| Three correct solutions | Accepted across all 30 stored cases, with available resource measurements |
| Known wrong sum solution | Wrong Answer after one pass, failing index 1 |
| Whitespace variation | Accepted through exact ASCII-whitespace byte-token comparison |
| Python syntax error and exception | Runtime Error / program_failed |
| Infinite loop | Time Limit Exceeded / CPU reason |
| Sleeping program | Time Limit Exceeded / wall reason |
| Memory-pressure fixture | Memory Limit Exceeded / confirmed_memory_limit_kill |
| Stdout-only, stderr-only and mixed floods | Output Limit Exceeded / combined-output reason |
| Output-kill measurements | Unknown aggregate values remain null; measurements are not fabricated |
| Another accepted evaluation after failures | Accepted with clean slot reuse |

The memory test allows Runtime Error when Python raises MemoryError without
confirmed OOM evidence; the recorded run produced a confirmed cgroup memory kill.
Pure classification tests verify that a signal or large resource number alone
never implies Memory Limit Exceeded. Unknown timeout clocks use the generic
reported-timeout reason rather than inventing CPU/wall evidence.

Each native evaluation checked that its sandbox directory, cgroup and quarantine
marker were absent afterward. An additional final slot-lock check confirmed no
remaining box resources or quarantine markers across the configured range.

## Scope

Normal checks cover raw-byte comparison, six ASCII separators, empty/binary
output, precise aggregation before millisecond rounding, optional/zero
measurements, overflow, typed timeout parsing, compatible reason validation,
archived-runtime rejection and safe sync/async entry points. Diagnostics contain
bounded trusted explanations and typed evidence, not hidden inputs/answers or raw
program stderr.

These checks establish the tested behavior on the supported host. They do not
claim a general kernel security proof or hard real-time behavior under arbitrary
host failure. The judging CLI and broader failure/load integration remain later
issues. The [learning guide](../learning/issue-8.md) retains the exercise contract
and commands.

# Phase 4 — Measurements and Portfolio Demonstration

**Status:** Planned; no implementation yet.

**Depends on:** All exit checks in [Phase 3](phase_3.md).

**Outcome:** The functional MVP has reproducible correctness demonstrations, measured performance, and documentation that explains both engineering decisions and practical limits.

## 1. Implementation Tasks

### 1.1 Metrics

Add a lightweight metrics endpoint to the API and a host-local metrics listener for each worker, with configurable addresses so two workers can run on one host. A Prometheus server/dashboard is optional; the benchmark script must also be able to collect machine-readable snapshots directly.

| Measurement | Definition |
| --- | --- |
| API acceptance latency | Client request-to-202 duration; failed/rejected requests reported separately |
| First queue wait | Database first-start timestamp minus submission acceptance timestamp |
| Evaluation duration | Runner wall duration for one attempt, including setup and cleanup |
| Test execution time | CPU/wall measurements returned by the sandbox for executed tests |
| End-to-end latency | Terminal timestamp minus acceptance timestamp; includes queueing, retries, and retry delays |
| Throughput | Accepted submissions per second and terminal submissions per second, reported separately |
| Backlog | Queued jobs, including a separate count not yet eligible because of retry delay |
| Owned running work | Running jobs with unexpired leases |
| Local active slots | Worker permits held for execution or cleanup, including cancellation in progress |
| Slot utilization | Time-weighted active slots divided by configured slots; not a CPU-utilization measurement |
| Retries and failures | Claims beyond the initial attempt, expired attempts, infrastructure errors, and exhausted jobs |
| Rejections | Capacity, validation, size, and database-availability errors by category |

- Use histograms for latency, counters for events, and gauges for sampled state.
- Metric labels may include verdict, event category, and a small stable worker identity. Do not label by submission ID, attempt token, source, or test contents.
- Avoid scanning database tables on every request. Sample shared queue state at a bounded interval and record the sampling interval in reports.
- Document that process counters reset on restart. Use persisted timestamps/attempts for crash demonstrations and benchmark reconciliation.
- An expired attempt's recovery-observed lifetime is not known execution time. Keep it separate from runner-measured duration.
- Record host CPU, memory, and process resource observations alongside slot utilization to identify saturation.

### 1.2 Load generator

Implement a script that uses the public API and persists its run configuration and raw observations.

Required modes:

1. **Burst:** Submit a fixed batch without waiting for evaluations, then poll until terminal states or the run deadline.
2. **Steady arrivals:** Submit at a configured rate for a fixed duration, independently of completion speed, with bounded client in-flight requests. Record achieved request rate and client-side throttling.
3. **Capacity test:** Stop or constrain workers, use a small admission limit, and send more work than the unfinished capacity.

Required behavior:

- Configure API URL, random seed, workload mix, request concurrency/rate, batch size/duration, polling interval, and a total deadline.
- Use only deterministic fixture sources and fixed problem definitions from earlier phases.
- Identify every accepted submission and poll it with bounded concurrency; record status-poll traffic separately from submission traffic.
- Bound HTTP request duration. Do not retry forever or hide errors by discarding them.
- Treat a request with an unknown acceptance outcome as ambiguous. Record it rather than automatically resubmitting and counting it as a clean success.
- Save request timings, IDs, terminal verdicts, rejection/error counts, worker/slot settings, and metrics snapshots as JSON/CSV artifacts.
- Count only terminal results within the measurement window as measured completions. Record later drain completions and unfinished/failed jobs explicitly.
- Summarize p50/p95/p99 from raw observations with sample counts. Mark small-sample p99 values as exploratory; do not average percentiles from separate runs or average worker percentiles.

### 1.3 Workloads and comparison matrix

Create deterministic source mixes for:

- Short correct submissions, to reveal API/database/sandbox startup overhead.
- CPU-bound correct submissions, using bounded extra computation before producing the correct answer; record the source/work count.
- Mixed outcomes: correct, wrong answer, runtime error, timeout, memory exhaustion, and output exhaustion. Report each category's completion/verdict counts.

Run the short and CPU-bound workloads across this matrix on the same host:

| Configuration | Workers | Slots per worker | Total healthy capacity |
| --- | --- | --- | --- |
| Baseline | 1 | 1 | 1 |
| Local concurrency | 1 | 2 | 2 |
| Multiple processes | 2 | 1 | 2 |
| Higher capacity | 2 | 2 | 4 |

Pilot the workload first. Choose and record warmup size, measured sample size, and arrival rate before comparisons; use the same settings across comparable runs. Target at least 1,000 completed samples per configuration when practical for tail measurements, and repeat each comparison three times. Report practical sample/time limitations explicitly.

- Drain the backlog before each run and keep runtime/problem versions and resource limits constant.
- Run overload and injected-failure experiments separately from normal throughput comparisons.
- Measure both acceptance responsiveness and completion capacity. More accepted requests alone do not establish higher evaluation throughput.
- Record whether the load generator shares the worker host and whether it could be a bottleneck.
- All workers initially share one Linux host. Describe this as multiple worker processes on one machine, not evidence of multi-machine scalability.
- Do not require linear speedup as an exit condition. Explain actual scaling and saturation using measurements.

### 1.4 Failure and overload demonstrations

Provide executable scripts or documented commands for these three main demonstrations:

1. **Worker crash:** Start two workers, submit work, kill the current owner, and show the lease, retry, new owner, and final verdict from persisted attempt history.
2. **Stale publication:** Pause an attempt before publication, expire it, let a replacement finish, then resume the old attempt. Show that the old write was rejected and the final result stayed unchanged.
3. **Capacity and scaling:** Compare the matrix above, then overload a small configured admission limit. Show bounded owned work, explicit rejections, backlog behavior, and recovery after workers resume.

Also include a retry-exhaustion run: induce a controlled infrastructure failure until three claims have been consumed and verify failed/Internal Judge Error. Distinguish this from a valid Runtime Error verdict.

Scripts must have deadlines and clean up only processes/data created by that run. Keep test hooks private to test/demo builds or configuration; do not expose failure injection through public routes. No unsafe "run without sandbox" demo mode.

### 1.5 Performance report and targeted improvement

Write a report containing:

- Hardware, operating system/kernel, tool/runtime versions, sandbox settings, database settings, and deployment arrangement.
- Exact commands, workload sources/mix, seeds, warmup, run durations, samples, and worker/slot counts.
- Acceptance and completion throughput, latency percentiles, backlog behavior, failures/rejections, and host resource observations.
- A chart comparing completion throughput across the matrix and a chart separating queue wait from evaluation time under load.
- Raw artifact locations, not just screenshots.
- The measured bottleneck and evidence supporting it: for example sandbox startup, CPU contention, database polling, or admission serialization.

If a clear bottleneck is found, make at most one focused improvement in this phase, such as adjusting polling or an index/query. Repeat the affected comparison with the same workload, and rerun relevant correctness checks. If no change is justified, document the evidence rather than adding infrastructure to create a larger architecture.

Performance numbers must come from actual runs. Until measurements exist, documentation must use targets or placeholders clearly marked as unmeasured.

### 1.6 Portfolio packaging

Create or finalize:

- `README.md`: problem statement, supported host, setup, configuration, API examples, test commands, and demo commands.
- `docs/architecture.md`: component/data-flow diagram, schema, job lifecycle, transaction boundaries, and worker ownership protocol.
- `docs/decisions.md`: concise rationale and alternatives for PostgreSQL queue, Isolate, token/lease fencing, retry budget, and admission serialization.
- `docs/benchmarks.md`: measured report with charts and links to raw artifacts.
- `docs/limitations.md`: supported threat model/configuration, shared host resources, single database, local-only access assumptions, duplicate execution, and deferred request deduplication.
- A repeatable setup/seed/demo workflow that does not require a frontend.
- Automated build/lint/unit and PostgreSQL integration checks, plus a documented real Linux sandbox test run.

Keep the portfolio claims specific to evidence. Describe implemented recovery and tested concurrency; do not claim production readiness, formal sandbox security, or a throughput figure without a supporting result.

## 2. Verification

- Reproduce setup, migrations, seeds, API acceptance, and sandbox execution from a clean supported environment.
- Reconcile every load run: sent requests, accepted IDs, explicit rejections, ambiguous outcomes, terminal results, and unfinished jobs must be accounted for.
- Confirm the chosen matrix configurations and test sources are recorded in each artifact.
- Confirm metric units and definitions agree with database timestamps and runner measurements.
- Run the crash, stale-write, capacity, and retry-exhaustion demonstrations with deadlines.
- Verify scripts cannot terminate unrelated processes or erase unrelated data.
- Check documentation links and run the published commands rather than relying on intended behavior.

## 3. Exit Criteria

- [ ] API/worker metrics and reproducible load scripts exist.
- [ ] Baseline and capacity comparisons have actual recorded results and raw artifacts.
- [ ] The report separates API latency, queue delay, evaluation time, and end-to-end latency.
- [ ] Crash recovery, stale-result rejection, overload, and retry exhaustion are demonstrated.
- [ ] Bottlenecks and any justified improvement are explained with evidence.
- [ ] Setup and portfolio documentation reproduce the result without a frontend.
- [ ] Performance and reliability claims match what was measured.

## 4. Optional Work After Completion

Choose an extension only after the four phases are complete and there is a concrete reason: C++ compilation, authentication for a shared deployment, a second machine, a minimal status UI, or broker comparison. A reusable job framework is an exploration, not part of this project's completion criteria.

**Project overview:** [project.md](project.md).

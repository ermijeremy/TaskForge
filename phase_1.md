# Phase 1 — Sandboxed Execution and Verdicts

**Status:** In progress — issue #2 workspace/configuration scaffold and its learning
exercise are implemented. Issue #4 adds the three local problems, strict fixture
loading, 30 ordered cases and 16 Python sources; see the [fixture guide](fixtures/README.md).
Isolate readiness is developed separately in issue #3. Submission execution and
the remaining issues are planned.

**Depends on:** [Project scope](project.md).

**Outcome:** A Rust CLI evaluates a local Python source file against fixed fixtures, returns a structured result, and cleans up every sandbox. No HTTP API, database, or distributed queue is introduced here.

## 1. Why This Comes First

Sandbox setup is the largest feasibility risk. Verify Linux support, permissions, resource accounting, and cleanup before building infrastructure around the runner.

Use Isolate as the initial candidate. Record the selected version, Python version, kernel, cgroup arrangement, and required privileges. Use the [official manual](https://www.ucw.cz/isolate/isolate.1.html) for prerequisites and controls supported by the installed version.

## 2. Implementation Tasks

### 2.1 Repository and configuration

- Create a Rust workspace with an execution library and a small `judge-local` CLI. API and worker binaries arrive later; do not create a generic plugin framework.
- Add formatting/lint configuration, a pinned Rust toolchain, and a documented build command.
- Define typed limits with units: CPU/wall seconds, memory bytes, output bytes, and process count.
- Validate configuration before executing anything. Reject nonpositive limits, nonexistent runtime paths, and unusable sandbox settings.
- Add a readiness command verifying sandbox availability and required permissions. An unavailable sandbox must fail explicitly.

### 2.2 Local problems and fixtures

Create three version-controlled problem definitions:

| Problem | Input/output contract | Required cases |
| --- | --- | --- |
| Sum two integers | Two signed integers; print their sum | Zero, negatives, and large values |
| Sum a list | `n` followed by `n` signed integers; print the sum | Empty list, one item, and mixed signs |
| Balanced brackets | One line using `()[]{}`; print `YES` or `NO` | Empty line, nesting, mismatch, and unmatched brackets |

- Store at least five ordered test cases per problem, with descriptions, public examples, and limits.
- Keep private inputs and expected outputs separate from source code provided to the sandbox.
- Use a fixed Python runtime; definitions become immutable seeded records in Phase 2.
- Include sample submissions for correct output, incorrect output, syntax error, runtime exception, infinite loop, sleeping, memory exhaustion, and excessive output.
- Add controlled probes for subprocess creation, network access, and reading a host-only marker file.

### 2.3 Runner interface

Implement an interface equivalent to `evaluate(source, problem, limits) -> EvaluationResult`, with no dependency on HTTP or PostgreSQL.

The result includes:

- A verdict or a typed infrastructure error.
- Passed-test count and total-test count.
- The first failing test's index, if any, for trusted diagnostics.
- Measured CPU time, wall duration, and peak memory where reported. Use explicit units and unavailable/null rather than invented zero values.
- A bounded diagnostic and machine-readable reason code.

Keep infrastructure errors distinct from user-code verdicts. In Phase 3 they become retry candidates; they are not silently converted into Runtime Error.

### 2.4 Sandbox lifecycle

For each test:

1. Reserve a sandbox identifier from a configured host-local range.
2. Initialize a fresh sandbox and stage only the source and permitted runtime files.
3. Supply the test input through stdin; retain expected output in the trusted process.
4. Execute Python with bytecode writes disabled and a minimal environment.
5. Drain stdout and stderr concurrently through bounded buffers; enforce the combined byte limit while reading.
6. Collect metadata and classify the outcome.
7. Terminate remaining sandbox processes and release resources before returning.

Initial restrictions:

- No network access, host credentials, database credentials, Docker socket, or arbitrary host mounts.
- Source/runtime mounts are read-only. The Python MVP provides no writable filesystem to submitted code; I/O uses stdin/stdout/stderr. If the runtime needs writable scratch, introduce and verify a bounded mount explicitly before enabling it.
- Enforce one submitted process initially; subprocess creation is outside supported problem behavior.
- Enforce CPU time, elapsed wall time, memory, and combined stdout/stderr limits.
- Handle cancellation, runner errors, and setup failures with cleanup. Killing a wrapper alone is insufficient if descendants survive.
- Keep sandbox metadata and trusted result files outside the submitted program's accessible filesystem.

Start with development defaults of 1 second CPU, 3 seconds wall time, 128 MiB memory, and 1 MiB combined output per test. These are tunable settings to verify on the host, not benchmark results.

Also bound the entire evaluation, including setup and cleanup, with an infrastructure deadline. Expiry of that guard is an infrastructure error; a program exceeding its per-test time limit is Time Limit Exceeded.

### 2.5 Verdict rules

- Evaluate tests sequentially and stop at the first failure.
- Split actual and expected stdout on ASCII whitespace and compare byte tokens exactly.
- Inspect metadata and supervisor actions before comparing output.
- If the supervisor stopped execution for excess output, return Output Limit Exceeded.
- Otherwise classify a reported memory-limit kill as Memory Limit Exceeded, a reported CPU/wall timeout as Time Limit Exceeded, then other program failures as Runtime Error.
- A Python syntax error is Runtime Error; there is no Compilation Error verdict.
- Compare successful output to produce Wrong Answer or Accepted.
- Missing/corrupt metadata, failed sandbox setup, and trusted judge failures are infrastructure errors.
- Preserve bounded reason information when multiple limit indicators appear; document the classification order above.

### 2.6 CLI and documentation

- Provide a CLI taking a problem identifier and source-file path, emitting a JSON result.
- Report infrastructure failure with a distinct nonzero CLI exit code. A valid failing verdict is not an infrastructure failure.
- Document native Linux setup, required privileges, pinned versions, and fixture commands.
- Keep the library callable by the later worker without invoking the CLI or parsing human-readable text.

## 3. Verification

| Scenario | Expected behavior |
| --- | --- |
| Correct solution for each problem | Accepted with all tests passed |
| Wrong solution | Wrong Answer; later tests do not run |
| Whitespace-only differences | Accepted under token comparison |
| Syntax error or exception | Runtime Error |
| Infinite loop | Time Limit Exceeded through CPU limit |
| Sleeping program | Time Limit Exceeded through wall limit |
| Controlled memory exhaustion | Memory Limit Exceeded when confirmed by sandbox evidence |
| Flood stdout, stderr, or both | Output Limit Exceeded; bounded judge memory |
| Spawn a subprocess | Denied under the process policy |
| Read a host-only marker or open a network connection | Access denied under configured policy |
| Cancel evaluation or induce setup failure | No surviving test process or unreleased sandbox |
| Reuse a slot for another evaluation | No files or state leak from the first |
| Sandbox unavailable | Explicit error; no ordinary-process fallback |

Unit-test comparison and metadata classification. Run real sandbox integration tests on supported Linux. Label environment-dependent tests clearly and record actual runs; a skipped sandbox test is not evidence that isolation works.

These checks verify declared configuration and common failure modes; they do not establish a formal security guarantee against kernel or sandbox vulnerabilities.

## 4. Exit Criteria

- [x] All three problems and required fixture categories exist (issue #4; data/inventory/reference checks).
- [ ] CLI produces structured results without a database or API.
- [ ] Real integration runs verify resource and access restrictions.
- [ ] Output capture, diagnostics, and execution duration are bounded.
- [ ] Cleanup works on normal completion, limit breaches, cancellation, and errors.
- [ ] Host prerequisites and assumptions are documented and reproducible.

## 5. Explicitly Deferred

HTTP routes, SQL schema, queue polling, worker pools, leases, retries, metrics servers, compilation, and additional languages.

**Next:** [Phase 2 — Durable API and Single Worker](phase_2.md).

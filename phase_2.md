# Phase 2 — Durable API and Single Worker

**Status:** Planned; no implementation yet.

**Depends on:** All exit checks in [Phase 1](phase_1.md).

**Outcome:** A client submits Python through HTTP, receives a durable submission ID, and polls for a result produced by one independent worker. This phase completes the normal execution path; automatic recovery from a killed worker arrives in Phase 3.

## 1. Implementation Tasks

### 1.1 Persistence and migrations

Add SQLx migrations and a bounded PostgreSQL connection pool. Use foreign keys, checked state values, nonnegative counts, and explicit units.

| Table | Required data and constraints |
| --- | --- |
| `problems` | Stable identifier, title, description, public examples, Python runtime identifier, CPU/wall/memory/output/process limits |
| `test_cases` | Identifier, problem foreign key, zero-based order, input, expected output; unique `(problem_id, position)` |
| `submissions` | UUID, problem foreign key, language, source, database-generated acceptance timestamp, nullable structured final result |
| `jobs` | UUID, unique submission foreign key, state, attempt count, available-at time, first-start time, terminal time, bounded infrastructure error code |

- Job states are `queued`, `running`, `completed`, and `failed`.
- Default a new job to `queued`, zero attempts, and immediately available.
- Store timestamps as PostgreSQL timestamps with time zone and use database time for lifecycle changes.
- Final result fields are verdict, passed/total test counts, aggregate CPU milliseconds, aggregate test wall milliseconds, peak memory bytes, and evaluation wall milliseconds. Unavailable measurements are null. CPU/wall test measurements sum executed tests; peak memory is their maximum; evaluation wall time includes setup and cleanup.
- Keep public result fields separate from trusted, bounded diagnostics. Never return raw program stdout/stderr or private test data.
- Index the queued-job lookup by availability and acceptance order, and the unfinished-state count used by admission. Verify the chosen SQL and indexes against PostgreSQL rather than assuming an index will be used.
- Seed the three Phase 1 problems idempotently. Rerunning seeds must not alter existing problems; changed fixtures receive a new identifier.

The `jobs` row is the source of lifecycle status. Do not add another independently updated status field to `submissions`.

### 1.2 HTTP API

| Route | Required behavior |
| --- | --- |
| `GET /health/live` | Reports that the API process is running |
| `GET /health/ready` | Verifies database access and expected schema version; 503 when unavailable |
| `GET /problems` | Lists the three problems' public identifiers and summaries |
| `GET /problems/{id}` | Returns description, public examples, supported runtime, and limits; never hidden tests |
| `POST /submissions` | Accepts `{problem_id, language: "python", source}` and returns 202 after commit |
| `GET /submissions/{id}` | Returns identifier, problem identifier, state, timestamps, and final result when terminal |

Submission acceptance returns the ID and a status URL, with a `Location` header. The API does not wait for evaluation. A nonterminal status has a null final result; repeated reads of a terminal result are stable.

Validation and error contract:

- Use a consistent JSON error object with a stable code and readable message.
- Reject malformed JSON/identifiers with 400, unsupported language or empty/whitespace-only source with 422, and unknown problem/submission identifiers with 404.
- Enforce a 96 KiB request-body limit and a 64 KiB UTF-8 source-byte limit; oversized requests/source return 413.
- Clients cannot override execution limits, select executable paths, or supply hidden test cases.
- Capacity exhaustion and database unavailability return 503. For capacity exhaustion include `Retry-After` with a short configured delay.
- Do not return connection strings, SQL errors, sandbox paths, source, or private tests in error messages.

### 1.3 Atomic acceptance and bounded backlog

Set an initial configurable maximum of 1,000 unfinished submissions, where unfinished means `queued` or `running`. This is an admission limit, not a promise of simultaneous execution capacity.

The acceptance transaction must:

1. Acquire one shared, transaction-scoped PostgreSQL advisory lock reserved for admission across all API instances.
2. Under READ COMMITTED isolation, count unfinished jobs in a subsequent statement after acquiring the lock, so a waiting request sees previous admissions' commits.
3. Reject if the limit has been reached, creating no submission or job.
4. Insert both the submission and its job.
5. Commit before returning 202.

All admission paths must use the same lock. Terminal transitions reduce the unfinished count; retries in Phase 3 preserve it. Use bounded database operation/lock waits so admission cannot hang indefinitely.

This deliberately serializes the small admission transaction for an exact limit. Document that tradeoff; optimize it only if later measurements justify a different design. See [PostgreSQL advisory locking](https://www.postgresql.org/docs/current/explicit-locking.html#ADVISORY-LOCKS).

If the response is lost after commit, the client may not know acceptance succeeded. Request deduplication is deferred; do not describe this as exactly-once HTTP submission.

### 1.4 Independent single worker

- Add a worker binary using the Phase 1 library and the same database migrations.
- Start with one execution slot. Do not run evaluation in the API process.
- Poll available queued jobs at a bounded, configurable interval; start with approximately 500 ms and modest jitter when idle.
- Claim one eligible row through `FOR UPDATE SKIP LOCKED`, ordered by availability, creation time, then ID. This is best-effort ordering, not strict FIFO.
- In the claim transaction, mark it running, increment attempt count, and set first-start time. Commit before invoking the runner.
- Execute all tests through the runner; never hold a claim transaction or pooled connection throughout execution.
- On a user-code verdict, atomically store the result and mark the job `completed`.
- On infrastructure failure, atomically publish Internal Judge Error and mark the job `failed`. Retrying infrastructure failures is Phase 3 work.
- On graceful shutdown, stop claiming, allow bounded draining, cancel/clean up the runner if needed, and record a failure if the database is reachable.

For this phase, an ungraceful worker crash can leave a `running` job without automatic reassignment. Document that limitation. Do not add a manual "reset every running row" mechanism that could later race with live workers.

### 1.5 Configuration, deployment, and logs

- Add Docker Compose services for persistent PostgreSQL storage and the API, with explicit readiness checks.
- Run the worker natively on the verified Phase 1 Linux host. Document how it reaches PostgreSQL and where sandbox privileges are required.
- Provide example configuration without real credentials. Configure database URL, bind address, poll interval, admission limit, source/body limits, and sandbox paths.
- Add JSON logs with request ID, submission ID, job ID, lifecycle event, duration, and stable error category.
- Exclude submitted source, private test data, program output, and credentials from routine logs.
- Add build/lint/unit and PostgreSQL integration checks. Sandbox checks run only on a host with verified prerequisites and must report whether they ran.

## 2. Verification

| Scenario | Expected behavior |
| --- | --- |
| Submit a correct solution over HTTP | 202; eventually completed/Accepted |
| Submit a known wrong or crashing solution | Completed with the appropriate user-code verdict |
| Keep workers stopped and submit work | API remains usable; job is durably queued |
| Restart the API before starting the worker | Queued submission and status survive |
| Run a slow evaluation while making status requests | Requests do not wait for that evaluation to finish |
| Force acceptance transaction failure after its first insert | Neither submission nor job commits |
| Run concurrent admissions at a small configured capacity | Unfinished count never exceeds the limit; excess requests get 503 |
| Database unavailable before acceptance | No 202 acknowledgment of uncommitted work |
| Read unknown IDs or send invalid/oversized input | Documented status/error code; no invalid job created |
| Query public routes and inspect routine logs | No hidden tests, source, raw program output, or secrets exposed |
| Repeat reads of a terminal submission | Same final verdict and terminal lifecycle state |
| Kill the worker during evaluation | Document the resulting stuck job as the Phase 3 recovery test case |

Integration tests must query the real database to verify atomicity and persisted state, not only inspect HTTP status codes. Demonstrate concurrent acceptance while the runner is busy without claiming an unmeasured latency target.

## 3. Exit Criteria

- [ ] Migrations and seed command reproduce the schema and three problems.
- [ ] All routes and the error contract are documented with example requests.
- [ ] Acceptance commits a submission and job together before returning 202.
- [ ] The worker evaluates committed jobs independently of the API.
- [ ] Results and job terminal state publish together.
- [ ] Request size, unfinished capacity, and output/diagnostics are bounded.
- [ ] Persistence, concurrency, validation, and rollback tests pass.
- [ ] Setup reproduces the flow on the supported Linux host.
- [ ] Documentation states that ungraceful worker recovery is not implemented yet.

## 4. Explicitly Deferred

Multiple active workers, lease ownership, automatic reassignment, infrastructure retries, attempt history, dedicated metrics endpoints, and performance optimization.

**Next:** [Phase 3 — Multiple Workers and Failure Recovery](phase_3.md).

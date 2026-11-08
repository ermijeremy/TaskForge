# Phase 3 — Multiple Workers and Failure Recovery

**Status:** Planned; no implementation yet.

**Depends on:** All exit checks in [Phase 2](phase_2.md).

**Outcome:** At least two independent worker processes share the durable queue, recover abandoned jobs, and reject stale results. Completing this phase produces the functional MVP.

## 1. Guarantees to Implement

- A healthy worker can eventually reclaim abandoned work while PostgreSQL is reachable.
- Execution may be duplicated after failure, but only the current, unexpired attempt may publish a result.
- Each job has at most three claims, including the initial claim.
- User-code verdicts finish the job; infrastructure failures may retry.
- Each worker bounds local execution and keeps a slot occupied until its sandbox processes have stopped.
- API admission and final-result behavior from Phase 2 remain intact.

These guarantees do not make PostgreSQL highly available, prevent all temporary overlap after expiry, or provide exactly-once execution.

## 2. Implementation Tasks

### 2.1 Lease and attempt schema

Add migrations with:

| Location | New data |
| --- | --- |
| `jobs` | Current worker ID, unique current-attempt token, lease expiry, hard attempt deadline, most recent queued-at timestamp |
| `job_attempts` | Attempt token, job foreign key, attempt number, worker ID, start/end timestamps, outcome, bounded error code, available resource measurements |

- Make `(job_id, attempt_number)` unique and tokens globally unique.
- Keep the authoritative attempt count on `jobs`; increment it only when a claim commits.
- Record attempt outcomes such as `completed`, `infrastructure_error`, `lease_expired`, and `shutdown`; never label an observed expiry as a known user-code verdict.
- Store a terminal result once on `submissions`; attempt history contains bounded metadata, not unlimited source/output copies.
- Add an index supporting expired-running-job scans and retain the available-queue index.
- Enforce consistent ownership fields for running rows and clear current ownership when leaving running state.

Upgrade a Phase 2 environment with API/workers stopped. Assign any preexisting running job an already-expired lease so normal recovery can handle it. Preserve its existing attempt count; historical details from before attempt tracking may be unavailable. Zero-downtime migration is outside scope.

### 2.2 Ownership protocol

Claim in one short transaction:

1. Acquire a local execution permit before querying for a job.
2. Select one eligible queued job whose attempt count is below the maximum using `FOR UPDATE SKIP LOCKED`.
3. Generate a new attempt token and increment attempt count.
4. Set running state, worker ID, lease expiry, and hard attempt deadline using database time.
5. Insert the attempt history row and commit.

Release the permit immediately if no claim is available. On execution, retain it through sandbox cleanup.

Every heartbeat, completion, and retry transition must check job ID, `running` state, current attempt token, and unexpired lease/deadline. A token alone is insufficient: an expired worker must not publish even before reassignment occurs. Recheck expiry with the live database clock after acquiring the row lock; a transaction-start timestamp captured before a lock wait can be stale.

For final publication, perform the guarded state transition and result write in the same transaction. If the guarded transition affects no row, roll back/discard the result. Terminal jobs are never overwritten by delayed completions.

### 2.3 Leases, heartbeats, and deadlines

Initial development defaults:

| Setting | Default | Purpose |
| --- | --- | --- |
| Lease duration | 15 seconds | Ownership expires without renewal |
| Heartbeat interval | 3 seconds | Renew with room for ordinary scheduling delay |
| Expiry scan interval | 1 second | Workers inspect abandoned work periodically |
| Maximum attempt duration | 60 seconds | Bound total attempt lifetime despite heartbeats |
| Maximum attempts | 3 total | Bound infrastructure retries and crash loops |
| Retry delays | 1 second after attempt 1; 2 seconds after attempt 2 | Avoid an immediate retry loop |

- Validate heartbeat interval is comfortably shorter than lease duration.
- Renew to the earlier of database-now plus lease duration and the fixed hard deadline. Heartbeats cannot extend the hard deadline.
- Run heartbeats independently from program execution so a busy runner does not block renewal.
- Use database time for ownership decisions and a local monotonic deadline to stop local work; do not rely on synchronized worker wall clocks.
- If renewal confirms lost ownership, immediately cancel execution and discard the result.
- If database access becomes uncertain, stop claiming new work and cancel the current attempt conservatively rather than assuming ownership persists. An owned retry transition can be attempted if database access is restored before expiry; otherwise leave recovery to expiry.
- The runner must honor cancellation and its fixed deadline, including when the database is unreachable.

Per-test resource limits still apply. The 60-second attempt guard covers the whole evaluation and trusted overhead; it is an infrastructure failure, not a replacement for per-test Time Limit Exceeded.

### 2.4 Recovery and retries

Run a maintenance loop within each worker; do not introduce a separate scheduler service.

For expired running jobs, take short row locks with skip-locked selection and recheck expiry after locking. In the same transaction:

1. Close the previous attempt as expired, if its history row exists.
2. If attempts remain, set queued state, clear ownership, record queued-at time, and set availability after the retry delay.
3. Otherwise set failed state, clear ownership, and publish Internal Judge Error.

For a known infrastructure failure while the attempt still owns an unexpired lease, use the same guarded retry-or-fail policy and record the actual infrastructure error category.

- Sandbox launch failure and trusted runner failure are retry candidates.
- Invalid problem/runtime configuration also consumes the bounded budget; record the cause so it is diagnosable rather than retrying forever.
- Wrong Answer, Time Limit Exceeded, Memory Limit Exceeded, Output Limit Exceeded, and Runtime Error are completed evaluations and never retried.
- Recovery cannot claim that a process on another host has already stopped. Ownership fencing protects final writes; cancellation, sandbox limits, and hard deadlines bound execution overlap.
- Maintenance loops must cooperate through row locking and conditional transitions so two workers cannot schedule two retries for the same expired attempt.

### 2.5 Local execution capacity and sandbox identities

- Add configurable execution slots, starting with one per worker and then two for capacity tests.
- Use a bounded permit/semaphore; no unbounded spawning or claiming a batch of jobs before slots are free.
- Give each process a unique worker ID and configure disjoint sandbox-ID ranges for workers on the same host.
- Prevent simultaneous ownership of the same host-local sandbox ID, including configuration errors, with host-local locking. Do not share a mutable work directory across attempts.
- Keep a permit reserved until execution and cleanup have ended, including cancellation after lease loss.
- Verify what happens to the sandbox supervisor and child when the worker receives SIGKILL. Use a supported parent-death/supervision mechanism or surviving bounded sandbox supervisor so submitted programs cannot run indefinitely after worker death.
- On restart, reconcile leftovers only in the worker's configured sandbox range after verifying no live process owns that range. Never sweep another live worker's resources.
- On graceful shutdown, stop claiming, drain within a deadline, then cancel and perform an owned retry transition if possible. Otherwise the lease expires normally.

### 2.6 Reliability logs and status

- Add attempt token, attempt number, worker ID, lease-loss event, retry reason, and expiry-recovery event to structured logs.
- Keep final API responses compatible with Phase 2. A queued retry or running attempt still has no final verdict.
- Expose worker readiness through startup checks and structured status logs; no worker management UI or separate registry service is needed.
- Preserve the admission bound: a running-to-queued retry does not create another submission or job and does not increase unfinished count.

## 3. Verification

Use real PostgreSQL integration tests for ownership races. Deterministic test hooks may pause execution before publication or shorten lease settings. Keep such hooks out of the public API.

| Scenario | Expected behavior |
| --- | --- |
| Two workers race for one job | One initial claim and one current token |
| Many jobs with two workers/two slots each | At most four currently owned local attempts across those workers in a healthy run |
| Kill a worker during execution | Another worker recovers after expiry and retry delay; no permanently stuck job |
| Kill after evaluation but before publication | Another attempt can evaluate; only the current owner publishes |
| Pause worker A, expire its lease, let worker B finish, then resume A | A's result is rejected; B's terminal result is unchanged |
| Publish after expiry but before reassignment | Rejected because the lease is expired |
| Heartbeat races with expiry recovery | Row locking/conditions allow only a valid renewal or recovery |
| Two maintenance loops recover the same job | One retry transition and one subsequent claim |
| Repeated infrastructure failure or crash | No fourth claim; failed/Internal Judge Error after budget exhaustion |
| User-code failure | Completed after one attempt, without an infrastructure retry |
| Database outage during execution | No new claims; local work stops; recovery proceeds after connectivity returns |
| Graceful stop and hard kill | No unbounded child execution or cleanup of another worker's sandbox |
| Run beyond the hard attempt deadline | Heartbeats cannot keep the attempt alive indefinitely |
| Concurrent admission while jobs retry | Unfinished count remains within configured capacity |

For crash recovery, measure expiry-to-reclaim delay and show that it includes maintenance polling, retry delay, and slot availability. Do not promise immediate recovery or a fixed bound while all workers are unavailable.

## 4. Exit Criteria

- [ ] Two independent workers complete the same shared workload correctly.
- [ ] Lease renewal, expiry, hard deadline, and three-attempt budget are enforced.
- [ ] Expired attempts cannot renew, publish, or overwrite terminal results.
- [ ] Crash, stale-result, recovery-race, and retry-exhaustion tests pass.
- [ ] Worker slots and sandbox identities remain bounded and independent.
- [ ] Shutdown, database outage, and worker-kill behavior are verified on the supported host.
- [ ] Atomic acceptance, final publication, and admission tests still pass.
- [ ] Architecture/lifecycle documentation describes actual guarantees and limitations.

## 5. Explicitly Deferred

Broker migration, strict FIFO, distributed global execution quotas, database failover, autoscaling, public deployment, and generic scheduling abstractions.

**Next:** [Phase 4 — Measurements and Portfolio Demonstration](phase_4.md).

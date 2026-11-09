# Issue #3 host verification

**Status: verified on 2026-11-09, approximately 17:46–17:48 EAT (UTC+03:00).**
The real integration test and the operator CLI both passed as ordinary UID/GID
1000, outside the agent filesystem sandbox. All ten IDs were cleaned after each
run. This completes issue #3's readiness/feasibility verification; it does not
complete Phase 1's later submission execution or adversarial verification work.

## Observed environment

| Item | Observation |
| --- | --- |
| OS | Fedora Linux 44 Workstation, native x86_64 |
| Kernel | `7.2.14-200.fc44.x86_64` |
| Python | `Python 3.14.8`; `/usr/bin/python3` resolves to `/usr/bin/python3.14` |
| Isolate | `The process isolator 2.7` |
| Provisioning source | `tools/provision-isolate-fedora.sh`, pinned upstream commit `8f185bb37f3f23e29b33b0c7727c91c13429abe3` |
| Binary permissions | `/usr/local/bin/isolate`: `root:root`, mode `4755` |
| Configuration permissions | `/usr/local/etc/isolate`: `root:root`, mode `0644` |
| Sandbox directory permissions | `/var/local/lib/isolate`: `root:root`, mode `0755` |
| Judge lock directory | `/run/lock/taskforge`: `ermias:ermias`, mode `0700` |
| Lock-directory persistence | `/etc/tmpfiles.d/taskforge.conf`: `d /run/lock/taskforge 0700 1000 1000 -` |
| Keeper | `isolate.service`: enabled and active since `2026-11-09 17:43:11 EAT` |
| Keeper executable | `/usr/local/sbin/isolate-cg-keeper` |
| Delegated cgroup root | `/sys/fs/cgroup/isolate.slice/isolate.service`, from `/run/isolate/cgroup` |
| Delegated available controllers | `cpuset cpu io memory pids` |
| Delegated subtree control after checks | `cpuset memory` |
| Configured sandbox identities | `first_uid = 60000`, `first_gid = 60000`, `num_boxes = 1000` |
| Checked TaskForge range | 0..9 inclusive, UID/GID 60000..60009 |
| Host swap | `/dev/zram0` active; no host-wide swap/performance tuning applied |

The user completed the reviewed installer with local sudo authentication. The
initial agent attempt, `sudo -n bash tools/provision-isolate-fedora.sh`, had exited
1 with `sudo: a password is required` before executing the script.

The installed binary's version string identifies the release. This verification
does not independently attest the installed binary's source hash.

## Configuration and runtime policy

`examples/judge.local.json` matches the committed `judge.example.json`: CPU 1000 ms,
wall 3000 ms, memory 134217728 bytes, output 1048576 bytes, one process, evaluation
timeout 60000 ms, box IDs 0..9, locks `/run/lock/taskforge`, Python `/usr/bin/python3`,
Isolate `/usr/local/bin/isolate`. Static `check-config` passed and resolved Python
to `/usr/bin/python3.14`.

The built-in trusted smoke probe uses fixed feasibility limits of 1 second CPU,
3 seconds wall time, 128 MiB cgroup memory, one process, 32 open files and zero
file/core sizes. It disables default mounts, binds `/box`, `/usr`, `/lib` and
optional `/lib64` read-only/nonrecursive, and executes Python with `-I -B` and a
minimal environment. `/etc`, `/proc`, `/tmp`, and the Docker socket are absent.
No network sharing or extra descriptor inheritance is enabled.

The probe's successful exit verifies its assertions: a non-root sandbox identity,
denied host-file reads and filesystem writes, denied fork, and network-unreachable
or permission-denied for the numeric documentation address. A mere network timeout
cannot pass. All these assertions run inside Isolate.

## Initial integration finding and correction

The first real test failed before sandbox initialization because preflight expected
an invented `Isolate 2.7` banner. The installed release prints:

```text
The process isolator 2.7
(c) 2012--2026 Martin Mares and Bernard Blackham
```

Preflight now checks the actual first line and retains the full banner in its
report. The fake-tool fixture also emits the real multiline format; rejection of
a different release remains covered. The following real runs use that correction.

## Real integration test

From the repository root, as the ordinary judge user outside the agent sandbox:

```sh
TASKFORGE_REAL_CONFIG="$PWD/examples/judge.local.json" \
  cargo test -p taskforge-execution --offline --locked --test readiness \
  real_isolate_readiness_smoke -- --ignored --exact --nocapture
```

Exit status: **0**. Complete report:

```json
{
  "isolate_version": "The process isolator 2.7\n(c) 2012--2026 Martin Mares and Bernard Blackham",
  "python_version": "Python 3.14.8",
  "cgroup_root": "/sys/fs/cgroup/isolate.slice/isolate.service",
  "checked_box_ids": [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
  "smoke_metadata": "time:0.018\ntime-wall:0.021\nmax-rss:11544\ncsw-voluntary:6\ncsw-forced:8\ncg-mem:6308\nexitcode:0\n"
}
```

```text
test real_isolate_readiness_smoke ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 11 filtered out; finished in 0.27s
```

Isolate's metadata reports CPU and wall times in seconds and memory in KiB.
These measurements describe a tiny feasibility probe, not benchmark performance.
Each tool operation is limited to 10 seconds; the overall preflight budget is
60 seconds here, with a separately bounded cleanup grace on failure.

## Operator CLI and independent cleanup observation

A second real invocation also passed, demonstrating sequential reuse of the range:

```sh
target/debug/judge-local check-ready --config examples/judge.local.json
```

Exit status: **0**. Complete output:

```text
Sandbox readiness passed (trusted smoke probe; cleanup completed).
The process isolator 2.7
(c) 2012--2026 Martin Mares and Bernard Blackham
Python 3.14.8
Cgroup root: /sys/fs/cgroup/isolate.slice/isolate.service
Checked sandbox IDs: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
Smoke metadata:
time:0.017
time-wall:0.017
max-rss:11480
csw-voluntary:3
csw-forced:9
cg-mem:5784
exitcode:0
```

Immediately afterward, an independent Python check enumerated all ten expected
box directories and cgroups and scanned every readable `/proc/<pid>/status`
for real/effective/saved/filesystem UIDs in 60000..60009. It found:

```json
{
  "checked_box_ids": [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
  "remaining_box_directories": [],
  "remaining_box_cgroups": [],
  "remaining_sandbox_processes": [],
  "unreadable_process_statuses": [],
  "delegated_controllers": "cpuset cpu io memory pids",
  "delegated_subtree_control": "cpuset memory"
}
```

The check asserted all four remaining/unreadable lists were empty and exited 0.
Persistent TaskForge lock files are expected and must not be unlinked; OS file
locks are released when preflight returns. The keeper's `daemon` cgroup and
process are also expected to persist.

## Ordinary regression checks

Passed after the correction:

- `cargo build --workspace --offline --locked`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --offline --locked -- -D warnings`
- `cargo test --workspace --offline --locked`: 26 passed, one real-host test
  ignored by default; that test was explicitly run successfully above.
- `bash -n tools/provision-isolate-fedora.sh`

The normal tests cover missing sandbox/no host fallback, version/delegation and
permission errors, range boundaries, ID contention/reuse, initialization/probe
failure cleanup, deadline expiry, cleanup failure reporting, malformed metadata,
symlinked lock rejection and bounded retained diagnostics. These are fake-tool
protocol tests, not evidence that real failure/cancellation paths are isolated.

## Limits of this evidence

This establishes a working native Linux init/run/cleanup path and basic access
restrictions for the fixed trusted probe. Broad read-only runtime mounts are
appropriate only for this feasibility check; restricted submission staging is
issue #6. CPU/wall/memory options were accepted and measurements were reported;
exhaustion fixtures, verdict classification, combined-output supervision, and
cancellation/error cleanup on real workloads remain issues #7–#10.

The workstation has active zram swap and shared workloads. Readiness is not a
performance baseline, a permanent host guarantee, or a formal security proof.
It must be rerun after relevant kernel/runtime/service/configuration changes.
The setuid helper requires execution outside an outer environment that disables
privilege elevation or cgroup/filesystem operations.

# Native Linux / Isolate setup (issue #3)

Issue #3 introduces synchronous library preflight and `judge-local check-ready`.
It runs only a built-in trusted probe; submission execution belongs to issue #6.
Readiness is required before using the later runner.

## Pinned environment

| Component | Selected version / arrangement |
| --- | --- |
| Rust | 1.97.1, from `rust-toolchain.toml` |
| Python | CPython 3.14.10, `/usr/bin/python3.14` on this host |
| Isolate | 2.7, commit `8f185bb37f3f23e29b33b0c7727c91c13429abe3` |
| Target OS | Native Fedora Linux 44 x86_64, systemd, cgroup v2 |
| Kernel prerequisite | Linux >= 5.19 for cgroup peak memory reporting |
| Isolate binary | root-owned `/usr/local/bin/isolate`, mode 4755 |
| Configuration | root-owned `/usr/local/etc/isolate`, mode 0644 |
| Sandbox filesystem | `/var/local/lib/isolate`, ancestors writable only by root |
| Cgroups | `isolate.service` runs `isolate-cg-keeper`; delegated subtree via `/run/isolate/cgroup` |
| Sandbox identities | UID/GID 60000..60999, `num_boxes = 1000` |
| TaskForge allocation | Dedicated box IDs 0..9, inclusive, one judge user |
| TaskForge locks | `/run/lock/taskforge`, owner is the judge user, mode 0700 |

The selected release and installation mechanisms come from the
[pinned upstream source](https://github.com/ioi/isolate/tree/8f185bb37f3f23e29b33b0c7727c91c13429abe3),
[installation rules](https://github.com/ioi/isolate/blob/v2.7/Makefile),
and [Isolate manual](https://www.ucw.cz/isolate/isolate.1.html).
The OS, kernel, Python and swap observations for this workstation are recorded
in [the successful host verification log](issue-3-host-verification.md). An unprovisioned
host and mocked tests do not count as a successful real sandbox run.

## Provision Fedora 44

Review [the provisioning script](../../tools/provision-isolate-fedora.sh), then
run from the repository root as the ordinary user who will run TaskForge:

```sh
sudo bash tools/provision-isolate-fedora.sh
```

This requires administrator authentication. The script installs build packages,
downloads the pinned source over HTTPS, builds Isolate, installs its setuid binary,
configuration and systemd units, enables/starts its keeper service, and creates a
persistent tmpfiles rule for the user-owned lock directory. It refuses to replace
an existing installation/configuration or use a UID/GID range occupied by a local
account. An administrator must also reserve that range against future account
or subordinate-ID allocation. No subordinate `isolate` account is needed with
these explicit numeric settings. Build files remain in a printed `/tmp` directory.

Run the judge as the ordinary user. Root is needed by the Isolate helper to create
namespaces, mount filesystems, change identity and manage cgroups. Running the
judge itself as root broadens access and is unnecessary. The filesystem containing
Isolate must permit setuid execution; a `nosuid` mount, `NoNewPrivileges`, an outer
container policy or a restrictive agent sandbox can prevent it. Use a dedicated
native Linux host for real verification. Containers are not a supported setup.

The script does not change SELinux policy, system swap, CPU governors or ASLR.
If SELinux denies an operation, inspect `journalctl -u isolate.service` and the
administrator's audit log before choosing a policy adjustment. Do not disable
SELinux to mask an unexplained failure. Upstream recommends disabling swap for
measurement reproducibility; Isolate 2.7 also attempts `memory.swap.max=0` for a
memory-limited sandbox. This workstation's active zram and shared workloads remain
limitations on reproducibility. See [the cgroup implementation](https://github.com/ioi/isolate/blob/v2.7/cg.c).

## Runtime updates

The supported Python pin is now 3.14.10. Readiness still rejects every other
patch version. The smoke probe, active problem runtime and submission catalog
use the same pin. New `-v2` problem revisions preserve the original `-v1`
definitions for 3.14.8. Earlier verification logs describe their original runs;
current-runtime verification is recorded in the issue #7 report.

The restricted image still uses the documented Python 3.14 filesystem layout.
Its interpreter, standard library, approved extensions and shared libraries must
come from a consistent host installation. Recreate the backend and rerun native
checks after a runtime update; changing a version string alone is insufficient
verification.

## Configure and check

```sh
cp examples/judge.example.json examples/judge.local.json
cargo build --workspace --locked
cargo run -p judge-local --locked -- check-config --config examples/judge.local.json
cargo run -p judge-local --locked -- check-ready --config examples/judge.local.json
```

The example selects `/usr/bin/python3`; static validation resolves its symlink.
Preflight rejects a different Python patch version or Isolate version. Runtime
paths are trusted administrator configuration, not user input; printing a matching
version is not a cryptographic binary authenticity check. A Python installed
outside `/usr` will need an explicitly reviewed smoke runtime mount policy first.

`check-config` stays static and launches nothing. `check-ready`:

1. Requires Linux/cgroup v2 and the Phase 1 one-process policy.
2. Executes version checks and locates the Isolate delegated cgroup root, requiring
   the cpu, memory and pids controllers.
3. Acquires nonblocking OS file locks for the entire configured ID range before
   modifying any box. A busy range fails without initializing or cleaning boxes.
4. Initializes and cleans each configured ID. In the first one it executes the
   built-in Python probe and verifies successful metadata, CPU/wall measurements
   and cgroup memory reporting.
5. Returns success only after cleanup. Failed initialization or a failed/timed-out
   probe still attempts cleanup; cleanup failure preserves the original failure.

Reserve the configured range across the entire host. Every TaskForge process for
this range must use the same canonical lock directory and the same Unix user.
Do not unlink lock files while running. Other software that invokes Isolate
directly does not honor these locks. Preflight intentionally resets configured
boxes via `--init`; never point it at another application's range. The upper bound
999 matches the provisioned `num_boxes`, and every ID is checked so a smaller
installed `num_boxes` fails explicitly.

The trusted probe has fixed readiness limits of 1 second CPU, 3 seconds wall time,
128 MiB cgroup memory, one process, 32 open files and zero-sized files/cores. These
are feasibility settings, independent of eventual submission limits. It disables
default directory mounts, binds only `/box`, `/usr`, `/lib`, and optional `/lib64`
read-only with nonrecursive mounts, and uses Python `-I -B`. The environment is
Isolate's minimal environment plus `LANG=C.UTF-8`. There is no `/tmp`, `/proc`,
`/etc` or Docker socket mount. Network sharing and inherited extra descriptors
are never enabled. The probe checks non-root identity, host-file denial, write
denial, fork denial, and a network-unreachable/permission-denied result for a
numeric documentation address (a connection timeout alone does not pass).

These broad read-only system runtime mounts are sufficient for a trusted smoke
probe. Restricting the staged Python runtime for submissions is issue #6. CPU,
wall and memory options are exercised here, but adversarial exhaustion tests,
output supervision and cancellation-safe submission cleanup remain issues #7
and #10. Preflight does not implement SIGINT/crash recovery. If interrupted,
stop all users of the range, then explicitly clean affected IDs as the same user:

```sh
/usr/local/bin/isolate --cg --box-id=0 --cleanup
```

Each trusted tool command has a 10-second maximum and bounded 64 KiB retained
output/metadata. Preflight uses the smaller of the configured evaluation deadline
and 60 seconds, with a separate 10-second cleanup grace on failure. Captures use
host-only temporary files with size polling, not production submission output
pipes. This does not establish a hard temporary-disk bound for malicious tools;
the configured host executables must be trusted. Later asynchronous workers must
call this synchronous API on a blocking thread.

Success exits 0; configuration/readiness failure prints an actionable error on
stderr and exits 1. Argument errors exit 2. Missing Isolate never triggers ordinary
host-process submission execution. Human-readable output is for operators; future
judging JSON and exit semantics belong to issue #9. The library already returns
structured `ReadinessReport` / typed `ReadinessError` without parsing display text.

## Verification

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
TASKFORGE_REAL_CONFIG="$PWD/examples/judge.local.json" \
  cargo test -p taskforge-execution --locked --test readiness \
  real_isolate_readiness_smoke -- --ignored --exact --nocapture
```

The ordinary readiness tests use fake tools to verify command protocol, bounds,
locking, version rejection, error handling and cleanup. Only the explicitly
invoked ignored test runs real Isolate. It requires the configuration environment
variable and fails on unavailable prerequisites; it never silently skips at run
time. An ignored default test is not evidence of working isolation. Record the
real command, output, versions and limitations in the host log.

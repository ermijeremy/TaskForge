# TaskForge

A backend project exploring reliable, resource-bounded online code judging.
See [project.md](project.md) for the design and the [Phase 1 plan](phase_1.md)
for the current implementation roadmap.

## Current scope

This branch implements [issue #3](https://github.com/ermijeremy/TaskForge/issues/3):
Linux/Isolate readiness, a trusted sandbox smoke probe, and native host setup.
Issue #2 configuration and its Rust exercise are complete. Submitted programs
are not executed yet; their staging, execution and supervision follow later.

See the [Linux setup guide](docs/setup/linux-isolate.md),
[host verification record](docs/setup/issue-3-host-verification.md), and
[readiness walkthrough](docs/learning/issue-3.md). The host record distinguishes
the successful real Linux smoke run from fake-tool protocol tests.

## Workspace

```text
crates/
  taskforge-execution/   configuration, limits, and Linux/Isolate readiness
  judge-local/          command-line entry point
examples/
  judge.example.json    configuration template with explicit units
docs/learning/
  issue-2.md            completed configuration exercise
  issue-3.md            readiness walkthrough
docs/setup/
  linux-isolate.md      pinned installation and readiness commands
```

The library uses Serde/serde_json, thiserror and tempfile; the CLI uses Clap.
Preflight uses synchronous standard-library processes. Tokio arrives with the
submission supervision work.

## Build and check

Rust 1.97.1 is pinned in `rust-toolchain.toml`, with rustfmt and Clippy.
The compiler matches the version available when this workspace was created.
Install Rust through rustup, then run from the repository root:

```sh
rustup toolchain install 1.97.1 --profile minimal --component rustfmt,clippy
cargo build --workspace --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo run -p judge-local --locked -- --help
```

`Cargo.lock` records the dependency versions used by both workspace packages.
The normal suite includes the completed exercise's rounding and maximum-value
regression tests and fake-tool readiness checks. One real Isolate test is
explicitly ignored by default; see the setup guide to run it. A skipped test
is not evidence of working isolation.

## Configuration

Copy `examples/judge.example.json` to `examples/judge.local.json` and adjust
`runtime.python_path` and `runtime.isolate_path` to actual files on your Linux
host. The local copy is ignored by Git. Absolute paths are supported; relative
paths are resolved from the configuration file's directory, not the caller's
working directory. Paths are not searched through `PATH`.

```sh
cargo run -p judge-local --locked -- check-config --config examples/judge.local.json
```

The command validates the JSON, rejects unknown fields, and rejects zero limits,
negative/fractional numbers, and missing or non-file runtime paths. On Unix it
also checks that files have executable permission bits. A configuration error
prints to stderr and exits with status 1; command usage errors exit with 2.

Omitting `limits` or individual limit fields uses these defaults:

| Setting | Default | Scope |
| --- | --- | --- |
| `cpu_time_ms` | 1,000 | Per test |
| `wall_time_ms` | 3,000 | Per test |
| `memory_bytes` | 134,217,728 (128 MiB) | Per test |
| `output_bytes` | 1,048,576 (1 MiB) | Combined stdout/stderr per test |
| `processes` | 1 | Submitted processes per test |
| `evaluation_timeout_ms` | 60,000 | Whole evaluation including setup/cleanup |

The runtime section is required. Validation does not launch those files, verify
their identity/version, check cgroup delegation or effective privileges, or
prove sandbox readiness. Run the explicit check after provisioning:

```sh
cargo run -p judge-local --locked -- check-ready --config examples/judge.local.json
```

The `sandbox` section selects an inclusive host-reserved ID range (default 0..9,
within 0..=999) and a shared lock directory (default `/run/lock/taskforge`).
The setup guide explains the permissions, pinning, probe policy and cleanup.

A missing Isolate file is an explicit configuration
failure; no ordinary-process execution fallback exists.

Limits are configuration values only at this stage. Enforcement is later work.
An eventual per-test timeout is a user-code verdict; an eventual whole-evaluation
deadline expiry is an infrastructure failure.

## Development workflow

Use one branch per implementation issue, starting from `main` once its
prerequisites have been merged. This branch is `issue/3-isolate-readiness`.
Leave the documented exercise for the project owner, review the changes and
test results together, and do not start another issue until requested.

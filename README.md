# TaskForge

A backend project exploring reliable, resource-bounded online code judging.
See [project.md](project.md) for the design and the [Phase 1 plan](phase_1.md)
for the current implementation roadmap.

## Current scope

This branch addresses [issue #2](https://github.com/ermijeremy/TaskForge/issues/2):
the Rust workspace, typed resource limits, and static configuration validation.
It does not execute submitted programs. Isolate installation/readiness belongs
to issue #3; problems, execution, verdicts, and the judging CLI follow in their
own issues.

One method is intentionally left for you to implement. Start with the
[issue #2 learning guide](docs/learning/issue-2.md).

## Workspace

```text
crates/
  taskforge-execution/   reusable configuration and validated limit types
  judge-local/          command-line entry point
examples/
  judge.example.json    configuration template with explicit units
docs/learning/
  issue-2.md            code walkthrough and your Rust exercise
```

The library uses Serde/serde_json for the configuration boundary and thiserror
for errors. The CLI uses Clap. Tokio and sandbox process execution are not
needed for static configuration checks and will be introduced when used.

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
The normal suite reports two ignored learning tests. They are explicitly
unfinished work, not successful verification of the exercise.

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
prove sandbox readiness. A missing Isolate file is an explicit configuration
failure; no ordinary-process execution fallback exists.

Limits are configuration values only at this stage. Enforcement is later work.
An eventual per-test timeout is a user-code verdict; an eventual whole-evaluation
deadline expiry is an infrastructure failure.

## Development workflow

Use one branch per implementation issue, starting from `main` once its
prerequisites have been merged. This branch is `issue/2-rust-workspace-config`.
Leave the documented exercise for the project owner, review the changes and
test results together, and do not start another issue until requested.

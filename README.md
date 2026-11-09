# TaskForge

A backend project exploring reliable, resource-bounded online code judging.
See [project.md](project.md) for the design and the [Phase 1 plan](phase_1.md)
for the current implementation roadmap.

## Current scope

This branch implements [issue #4](https://github.com/ermijeremy/TaskForge/issues/4):
three fixed problem definitions, strict trusted-fixture loading, and controlled
Python sample submissions. The definitions contain 30 ordered private cases;
16 named source fixtures cover the planned outcomes and access probes.

See the [fixture guide](fixtures/README.md) for I/O contracts, the catalog,
validation rules and commands, and the [Rust walkthrough](docs/learning/issue-4.md)
for implementation details and an optional exercise. Issue #2 configuration is
complete; Isolate readiness is developed separately in issue #3. Submission
execution, verdict classification and the judging CLI follow in later issues.

## Workspace

```text
crates/
  taskforge-execution/   configuration, validated limits, and trusted problem loading
  judge-local/          command-line entry point
fixtures/
  problems/             three versioned JSON problem definitions
  submissions/          named Python sources and catalog
  private/              host-only marker fixture
examples/
  judge.example.json    configuration template with explicit units
docs/learning/
  issue-2.md            completed configuration exercise
  issue-4.md            problem/fixture walkthrough and optional exercise
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
The normal suite includes the completed exercise's rounding and maximum-value
regression tests; no learning tests are skipped.

## Problem fixtures

```sh
cargo test -p taskforge-execution --offline --locked --test problems
cargo test -p taskforge-execution --offline --locked --test submission_catalog
python3 tools/check-python-fixtures.py
```

The problem tests check every expected answer with independent Rust references.
The Python check parses the sources and expects exactly one intentional syntax
error; it executes none of them. Actual sandbox verdict checks belong to later
issues. Keep private problem data outside sandbox mounts and public API output.

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

Host and problem limits are validated values at this stage. Enforcement is later work.
An eventual per-test timeout is a user-code verdict; an eventual whole-evaluation
deadline expiry is an infrastructure failure.

## Development workflow

Use one branch per implementation issue, starting from `main` once its
prerequisites have been merged. This branch is `issue/4-problems-fixtures`.
Leave the documented exercise for the project owner, review the changes and
test results together, and do not start another issue until requested.

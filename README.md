# TaskForge

A backend project exploring reliable, resource-bounded online code judging.
See [project.md](project.md) for the design and the [Phase 1 plan](phase_1.md)
for the current implementation roadmap.

## Current scope

This branch starts [issue #7](https://github.com/ermijeremy/TaskForge/issues/7):
Tokio-facing execution supervision, fair stdout/stderr draining, an evaluation-wide
deadline, cancellation and cleanup ownership. The core combined-output accounting exercise is complete and its tests run
normally. Native verification of this new path is pending.

Start with the [issue #7 exercise](docs/learning/issue-7.md),
[supervision guide](docs/supervision.md), and
[verification status](docs/setup/issue-7-verification.md).

Issues #2–#6 provide validated configuration, Linux/Isolate readiness, three
problems with 30 cases and 16 source fixtures, validated result/error types, and
fresh restricted sandbox lifecycles. The issue #6 core exercise is complete;
its [recorded native run](docs/setup/issue-6-host-verification.md) covers selected
fixtures, access restrictions and slot reuse. Ordered verdict evaluation and the
judging CLI arrive in later issues.

## Workspace

```text
crates/
  taskforge-execution/   configuration, fixtures, contracts, sandbox lifecycle, supervision
  judge-local/          command-line entry point
fixtures/
  problems/             three versioned JSON problem definitions
  submissions/          named Python sources and catalog
  private/              host-only marker fixture
examples/
  judge.example.json    configuration template with explicit units
  results/              synthetic result/error JSON examples
docs/learning/
  issue-2.md            completed configuration exercise
  issue-3.md            readiness walkthrough
  issue-4.md            problem/fixture walkthrough and optional exercise
  issue-5.md            completed core result-validation exercise
  issue-6.md            completed core Isolate limit-conversion exercise
  issue-7.md            completed core combined-output accounting exercise
docs/setup/
  linux-isolate.md      pinned installation and readiness commands
```

The library uses Serde/serde_json, thiserror, tempfile, Tokio and rustix; the CLI
uses Clap. Supervision moves blocking host work off the async executor.

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
The normal suite covers configuration, fixtures, fake-tool readiness and the
result contract, including the completed issue #5 exercise, and fake-tool sandbox
lifecycle checks. Completed issue #6/#7 accounting and supervised protocol tests
run normally. Four host-dependent checks remain ignored by default; run them
explicitly on the supported host. Skips are not evidence of working isolation.

## Issue #7 exercise

The completed `OutputBudget::record` is in
`crates/taskforge-execution/src/supervision/budget.rs`. Run its seven regression tests:

```sh
cargo test -p taskforge-execution --offline --locked \
  --test output_budget_exercise
```

The [learning guide](docs/learning/issue-7.md) specifies overflow, equality, raw
byte handling, bounded retention and the additional protocol/native commands.
The overflow and supervised protocol tests also run normally. Native checks stay opt-in.

## Completed issue #6 exercise

```sh
cargo test -p taskforge-execution --offline --locked --test sandbox_limits_exercise
cargo run -p taskforge-execution --offline --locked --example isolate_limits
```

The six regression tests validate units, range boundaries, memory rounding and
process policy. The mapping demo prints the four default Isolate limit flags and
never executes submitted source. The learning guide includes the explicit
real-host integration command.

## Completed issue #5 exercise

```sh
cargo test -p taskforge-execution --offline --locked --test evaluation_exercise
```

The nine tests cover valid results, invalid counts/reason pairs, verdict-specific
progress rules, preserved fields and maximum integer values. The schema example
prints four synthetic result/error payloads:

```sh
cargo run -p taskforge-execution --offline --locked --example result_contract
```

The static JSON examples under `examples/results/` and ordinary serialization
tests document and validate the schema. They do not represent actual submission
execution.

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
prove sandbox readiness. Run the explicit check after provisioning:

```sh
cargo run -p judge-local --locked -- check-ready --config examples/judge.local.json
```

The `sandbox` section selects an inclusive host-reserved ID range (default 0..9,
within 0..=999) and a shared lock directory (default `/run/lock/taskforge`).
The setup guide explains the permissions, pinning, probe policy and cleanup.

A missing Isolate file is an explicit configuration
failure; no ordinary-process execution fallback exists.

Readiness uses fixed smoke-probe limits. The completed issue #6 conversion maps
per-test CPU/wall/memory/process settings into Isolate options. Issue #7 supplies the supervisor around
those controls with completed combined-output accounting. A per-test
timeout supplies user-code timeout evidence; whole-evaluation deadline expiry is
an infrastructure failure. Supervised output budgets above 1 MiB are rejected.

## Development workflow

Use one branch per implementation issue, starting from `main` once its
prerequisites have been merged. This branch is `issue/7-bounded-supervision`.
Leave the documented exercise for the project owner, review the changes and
test results together, and do not start another issue until requested.

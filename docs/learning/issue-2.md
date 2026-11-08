# Issue #2 — Workspace and configuration exercise

## What is already implemented

- Cargo manages the execution library and CLI together, sharing dependencies,
  toolchain, lockfile, lint settings, and build output.
- `LimitSettings` describes untrusted configuration numbers with explicit units.
- `TryFrom<LimitSettings>` checks them and builds `ExecutionLimits`, whose private
  fields use `Duration` and `NonZero` integers to preserve valid values.
- `ExecutionSettings::validate` checks the separate infrastructure deadline and
  runtime paths. `ExecutionConfig::load` reads JSON and determines the path base.
- The CLI calls the library and maps success/failure into process exit statuses.

Read these files in order:

1. [Workspace manifest](../../Cargo.toml): shared packages, dependencies and lints.
2. [Limits](../../crates/taskforge-execution/src/limits.rs): defaults, conversions,
   private fields and getters.
3. [Configuration](../../crates/taskforge-execution/src/config.rs): deserialization,
   filesystem checks, `Result`, `?`, and error types.
4. [CLI](../../crates/judge-local/src/main.rs): argument parsing and library usage.
5. [Configuration tests](../../crates/taskforge-execution/tests/configuration.rs):
   positive values, invalid values, defaults, paths and units.

## Your implementation

Implement **`ExecutionLimits::memory_kib_ceil`** in `limits.rs`. Its `todo!` is
intentional and the normal configuration command never calls it.

The method must:

- Read the validated memory limit from `self` without changing it.
- Convert bytes into KiB, where one KiB is 1,024 bytes.
- Round upward when there is a partial KiB.
- Return a `u64`, using integer arithmetic.
- Work for `u64::MAX` without overflowing in either debug or release builds.

Examples: 1 byte becomes 1 KiB, 1,024 bytes becomes 1 KiB, and 1,025 bytes becomes
2 KiB. The default 128 MiB becomes 131,072 KiB. Zero cannot reach this method
because the validated memory field is `NonZeroU64`.

Hints: inspect the field's type and how other getters use it. Think about both
the whole units and a possible remainder. A calculation that adds a constant
to the byte count before dividing needs special care near the integer maximum.
Avoid floating-point conversions, which can lose precision for large integers.

## Test your work

Run the opt-in exercise tests:

```sh
cargo test -p taskforge-execution --locked --test learning_exercise -- --ignored
```

Before you implement the method, both tests fail at the deliberate `todo!`.
Their source is [learning_exercise.rs](../../crates/taskforge-execution/tests/learning_exercise.rs).
Do not change expected values to make the tests pass.

Once both tests pass, remove their `#[ignore = "..."]` attributes so the exercise
becomes part of the normal suite. Then run the formatting, Clippy, and workspace
test commands in the README.

## Explain it back

After implementing, explain why a normal integer division is insufficient,
why your method cannot overflow, how `NonZeroU64` protects the input, and why
`&self` is appropriate here. Also explain why valid file paths alone do not
establish that Isolate can safely execute a submission.

Keep issue #2 open until your exercise and review are complete. Do not begin
issue #3 as part of this exercise.

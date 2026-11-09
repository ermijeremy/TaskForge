# Issue #4 — Problems and fixture boundaries

## Implemented

Three versioned problem definitions supply 30 ordered cases, public I/O examples,
an explicit Python runtime and complete limits. Sixteen named Python sources
cover correct/wrong answers, whitespace, errors, resource exhaustion and access
probes. The Rust loader validates definitions without executing submissions.

Read in this order:

1. [Fixture guide](../../fixtures/README.md): contracts and trusted/private boundaries.
2. [One definition](../../fixtures/problems/sum-two-integers-v1.json): explicit metadata and ordered cases.
3. [Problem module](../../crates/taskforge-execution/src/problem.rs): strict Serde types and validation.
4. [Problem tests](../../crates/taskforge-execution/tests/problems.rs): reference answers, invalid data and size/order constraints.
5. [Source catalog](../../fixtures/submissions/catalog.json): fixture names and conditional expectations.

## Rust decisions to explain

`ProblemDocument` is the private raw deserialization type. A public constructor
validates it before creating `Problem`. The validated object's fields are private,
so callers receive borrowed metadata/cases rather than mutable access.

`ProblemLimits` requires every JSON field. Reusing the defaulting host-config
`LimitSettings` directly would accidentally accept an incomplete immutable problem.
We convert the required values into `LimitSettings` and reuse `ExecutionLimits`
validation, keeping the actual positive-number rules in one place.

`Result` keeps malformed fixture data distinct from future user-code verdicts.
`?` propagates reading, JSON and validation errors. Runtime identification uses an
enum with a fixed serialized name, rather than a host executable path.

The `HashSet` detects duplicate case IDs; `enumerate()` supplies the expected
zero-based position. We reject a mismatched position rather than silently sorting
or repairing a definition. Case identity/order must survive future database seeds.

Explain why `Problem` has no `Serialize` implementation while `PublicProblem`
does, and why public example output is allowed while private expected output
must remain in the trusted judge.

## Optional owner exercise

Add a `Problem::total_input_bytes(&self) -> usize` helper that sums the UTF-8 byte
lengths of private test inputs without cloning them. Use an iterator over borrowed
cases and explain why the current case-count/byte bounds make the total safe.
Write a small regression for empty inputs and a non-ASCII byte-length example.
This helper is not required by the loader or a pending implementation task.

Run the fixture tests and the workspace checks from the fixture guide. The Python
syntax check parses every source but never executes it. Real sandbox fixture
execution is intentionally left for the runner and integration issues.

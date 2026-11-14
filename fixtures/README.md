# Phase 1 problems and Python fixtures

Issue #4 supplies trusted problem data and controlled submission sources. Loading
these files does not execute Python or establish a verdict. The runner and real
sandbox outcome checks arrive in issues #6–#10.

## Fixed problems

| Immutable ID | Contract | Ordered private cases |
| --- | --- | --- |
| `sum-two-integers-v2` | Two integers, each in [-10^18, 10^18]; print their sum | 8: zero, positive, negative, cancellation, large values, whitespace |
| `sum-list-v2` | `n` followed by exactly `n` integers; 0 <= n <= 100, each value in [-10^18, 10^18]; print the sum | 9: empty, singleton, mixed signs, negatives, large values, 100 values, whitespace |
| `balanced-brackets-v2` | One line of at most 1000 characters from `()[]{}`; print `YES` or `NO` | 13: empty line/file, all pairs, nesting, crossed/mismatched/unmatched brackets, depth 500 |

Integer input tokens use ASCII whitespace separators. Decimal output is exact;
no floating-point tolerance exists. The list result may exceed a signed 64-bit
integer. The bracket line's optional trailing LF is not part of the string. An
empty line or empty file is balanced. Spaces and extra bracket-input lines are
outside that problem's input contract. Each definition contains descriptions,
public examples, a runtime ID, all per-test limits, and the private cases.

All three definitions identify `python-3.14.10` and explicitly store these limits:
CPU 1000 ms, wall 3000 ms, memory 134217728 bytes, combined output 1048576 bytes,
and one process. They do not store host executable paths or the infrastructure
evaluation timeout. Those belong to host execution configuration.

## Loading and validation

The library exports `Problem::load(path)`, `Problem::from_json(bytes)` and
`load_problems(directory)`. The latter loads the three fixed IDs in the table's
order, checking that each filename matches its declared ID. Example library use:

```rust
let problems = taskforge_execution::load_problems("fixtures/problems")?;
for problem in &problems {
    println!("{}: {} tests", problem.public().id, problem.test_cases().len());
}
```

The loader rejects malformed/unknown fields, missing schema/runtime/limits,
unsupported runtime IDs/schema versions, zero limits, process counts other than
one, blank descriptions/contracts, invalid IDs, duplicate test IDs, and positions
that do not match array order. It requires 1..=8 public examples and 5..=128 cases.
Definitions are limited to 256 KiB; each individual input/output to 8 KiB; each
metadata text field to 16 KiB. Empty case input/output is structurally allowed.
The fixture tests separately verify the fixed problems' I/O semantics and every
expected answer using independent Rust reference calculations.

Immutable fixture limits intentionally have no defaults. A missing memory limit
must fail, even though omitted host configuration limits have development defaults.
`ProblemLimits` is therefore a separate required-field boundary type; it reuses
`ExecutionLimits` validation after deserialization.

`Problem::public()` contains the serializable public fields and examples.
`Problem::test_cases()` returns trusted private inputs and expected outputs.
`Problem` itself deliberately does not implement `Serialize`. Private test data
must not be included in future API responses or sandbox staging. Load tests in
the trusted judge, pipe only a selected input to the sandbox, and retain expected
output for comparison in the trusted process. Never copy a problem JSON file,
this fixture directory, or the submission catalog into a sandbox.

## Runtime revision

The active catalog uses `-v2` definitions pinned to CPython 3.14.10. The original
`-v1` files remain unchanged and identify CPython 3.14.8. Only the problem ID and
runtime differ between those revisions; contracts, limits and test cases are
preserved. `load_problems` selects the three active `-v2` definitions explicitly.
The legacy runtime identifier remains readable for archived definitions; it does
not bypass the execution backend's exact current-runtime readiness check.

## Stable order and future seeding

`schema_version = 1` identifies the document format. The revision suffixes in problem IDs identify
immutable problem revisions. Test arrays have explicit zero-based `position`
values and stable case IDs. Loading never sorts, rewrites, or renumbers them.
These documents can seed Phase 2 problem/test records without changing their data.

Changing an existing problem's contract, runtime, limits or tests requires a new
problem ID and an explicit catalog update. The loader checks current structure;
Git review and the future idempotent seed command enforce immutability across
revisions. There is no database seeder or in-place-edit API in this issue.

## Named Python sources

[submissions/catalog.json](submissions/catalog.json) maps each source to a problem,
a category and its expected behavior. Access probes have their own diagnostic
markers; they are not intended to print an ordinary sum/bracket answer.

| Source | Purpose / later expected outcome |
| --- | --- |
| `sum_two_accepted.py` | Correct sum of two integers |
| `sum_list_accepted.py` | Correct list sum, including n = 0 |
| `balanced_brackets_accepted.py` | Correct bracket matching, including empty input |
| `sum_two_wrong.py` | Prints a - b; passes position 0 and first fails at position 1 |
| `sum_two_whitespace.py` | Correct token with leading/trailing spaces, tabs and newlines |
| `syntax_error.py` | Intentional syntax error; later Runtime Error |
| `exception.py` | Intentional exception; later Runtime Error |
| `infinite_loop.py` | CPU timeout; requires sandbox limits |
| `sleeping.py` | Fixed 60-second sleep, exceeding the 3-second wall limit |
| `memory_pressure.py` | At most 256 MiB of retained touched data against a 128 MiB limit |
| `stdout_flood.py` | Requests at most 4 MiB on stdout |
| `stderr_flood.py` | Requests at most 4 MiB on stderr |
| `mixed_output_flood.py` | 768 KiB on each stream, 1536 KiB combined; detects combined-budget enforcement |
| `subprocess_probe.py` | Attempts exactly one child and reaps it if unexpectedly allowed |
| `network_probe.py` | One numeric documentation-address connection with a 250 ms socket timeout |
| `host_file_probe.py` | Attempts to read a known host-only marker |

Memory pressure is a bounded allocation attempt, not a guarantee of an OOM kill.
Report Memory Limit Exceeded only when trusted sandbox metadata confirms a memory
kill. Python `MemoryError` alone is Runtime Error. A network timeout or connection
refusal is inconclusive: the probe requires explicit routing/permission denial.

Do not run the resource/access fixtures as ordinary host programs. They are inputs
for the later sandbox runner; this issue does not add an unsafe execution fallback.
The Python syntax checker below parses source without executing it or writing
bytecode. It deliberately expects one syntax error.

## Host-only marker prerequisite

`private/host-only-marker.txt` is a nonsecret test marker. A future supported-host
integration harness must arrange a host-readable copy at
`/var/local/lib/taskforge/host-only-marker.txt`, confirm it exists/is readable
outside the sandbox immediately before the probe, and keep it outside all staged
files and mounts. If that prerequisite is missing, the host-file test must fail
as inconclusive; a nonexistent marker cannot establish isolation. The probe only
reads at most 64 bytes and never prints file contents.

No host marker is installed by loading fixtures or running this issue's tests.
Real sandbox setup and readiness are covered by issue #3; see the
[Linux setup guide](../docs/setup/linux-isolate.md).

## Checks

From the repository root:

```sh
cargo test -p taskforge-execution --offline --locked --test problems
cargo test -p taskforge-execution --offline --locked --test submission_catalog
python3 tools/check-python-fixtures.py
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --offline --locked -- -D warnings
cargo test --workspace --offline --locked
```

These checks validate data, reference answers, fixture inventory, and syntax.
They do not claim actual Accepted/timeout/memory/output/access verdicts. Real
execution and those assertions belong to the sandbox runner/integration issues.

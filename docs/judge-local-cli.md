# Local judging CLI

Issue #9 adds `judge-local judge`, structured JSON results/errors, distinct exit
codes, and machine-readable readiness. Normal tests and native CLI verification
pass, including result framing, resource verdicts and explicit shutdown.

## Build and prepare

Use the [native setup guide](setup/linux-isolate.md) to provision the pinned
Isolate 2.7 / CPython 3.14.10 environment and its privilege/cgroup requirements.
Run as the ordinary judge user. The setuid Isolate helper performs privileged
sandbox operations; the CLI itself does not require root.

```sh
cargo build --workspace --offline --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --offline --locked -- -D warnings
cargo test --workspace --offline --locked
cargo run -p judge-local --offline --locked -- --help
cargo run -p judge-local --offline --locked -- judge --help
```

Use `examples/judge.local.json`, the ignored local configuration, after filling
in the paths/settings from `examples/judge.example.json`. Then run:

```sh
target/debug/judge-local check-config --config examples/judge.local.json
target/debug/judge-local check-ready --config examples/judge.local.json
target/debug/judge-local check-ready --config examples/judge.local.json --json
```

Static configuration validation launches nothing. Readiness runs a fixed trusted
probe and cleans all configured IDs. Do not run readiness while other work is
using that range. Its JSON success payload is the library `ReadinessReport`;
JSON failures use the bounded infrastructure-error shape. Operator readiness
reports include runtime versions, paths, checked IDs and trusted smoke metadata.
The human mode gives detailed setup errors; JSON errors recommend that mode for
troubleshooting.

## Judge a source

Run from the repository root:

```sh
target/debug/judge-local judge \
  --problem sum-two-integers-v2 \
  --source fixtures/submissions/sum_two_accepted.py \
  --config examples/judge.local.json
```

Required flags are `--problem`, `--source` and `--config`. `--problems-dir` defaults
to `fixtures/problems`. Source, config and problems-directory arguments resolve
from the caller's working directory; executable paths inside the configuration
resolve from that configuration file's directory. Pass absolute paths or an
explicit `--problems-dir` when running from another directory.

Only the three active `-v2` problem IDs are accepted; unknown IDs and traversal
strings are argument errors. Source must be a readable regular UTF-8 file of
1..=65536 bytes. The loader reads at most 65537 bytes, rejects excess, and rejects
FIFO/device/directory inputs before reading. It launches no source directly.

The judge uses the selected problem's immutable per-test limits. Configuration
still selects runtime, reserved sandbox IDs, locks and the overall evaluation
budget; its development per-test defaults do not override a problem's policy.
Expected output stays in trusted judge memory. Every test gets a fresh sandbox,
cases run in stored order and evaluation stops at its first failure.

## JSON and exit codes

| Command / outcome | Exit | Stdout |
| --- | --- | --- |
| Judge: any completed evaluation, including Wrong Answer or resource failure | 0 | One `EvaluationResult` JSON document plus newline |
| Judge: input/config/definition/startup/execution infrastructure failure | 3 | One `InfrastructureError` JSON document plus newline |
| Readiness success, human or JSON | 0 | Selected operator report format |
| Readiness failure with `--json` | 1 | One bounded error JSON document plus newline |
| Human readiness/configuration failure | 1 | Empty; explanation on stderr |
| Report serialization or stdout-write failure | 1 | No successful report is promised; explanation on stderr |
| Missing/invalid CLI arguments | 2 | Empty; Clap usage error on stderr |
| Help/version | 0 | Requested help/version text |

A completed result contains `verdict`, `reason_code`, passed/total counts,
zero-based first failing index, optional resource measurements and a bounded
trusted diagnostic. Error JSON contains `reason_code` and `diagnostic`, with no
fabricated verdict/progress. See the existing [schema examples](../examples/results)
and [evaluation contract](evaluation-contract.md). There is no additional JSON
envelope. Successful writing produces no stderr text, logs, raw submission output
or private test data. Judging reports are capped at 32 KiB before the final newline;
readiness has its own existing bounded tool/metadata captures.

Shell status zero means the evaluation completed. Inspect its JSON verdict to
learn whether the solution was Accepted. A caller can distinguish infrastructure
failure using status three without parsing human-readable error messages.

## Fixture examples

Use the command above with these problem/source pairs:

| Problem | Source under `fixtures/submissions/` | Expected result |
| --- | --- | --- |
| `sum-two-integers-v2` | `sum_two_accepted.py` | Accepted |
| `sum-list-v2` | `sum_list_accepted.py` | Accepted |
| `balanced-brackets-v2` | `balanced_brackets_accepted.py` | Accepted |
| `sum-two-integers-v2` | `sum_two_wrong.py` | Wrong Answer after one pass |
| `sum-two-integers-v2` | `sum_two_whitespace.py` | Accepted |
| `sum-two-integers-v2` | `syntax_error.py`, `exception.py` | Runtime Error |
| `sum-two-integers-v2` | `infinite_loop.py` | CPU Time Limit Exceeded |
| `sum-two-integers-v2` | `sleeping.py` | Wall Time Limit Exceeded |
| `sum-two-integers-v2` | `memory_pressure.py` | Memory Limit Exceeded only with confirmed OOM evidence; otherwise Runtime Error |
| `sum-two-integers-v2` | `stdout_flood.py`, `stderr_flood.py`, `mixed_output_flood.py` | Output Limit Exceeded |

For example, capture the known wrong solution's result and shell code separately:

```sh
target/debug/judge-local judge --problem sum-two-integers-v2 \
  --source fixtures/submissions/sum_two_wrong.py \
  --config examples/judge.local.json > /tmp/taskforge-result.json
taskforge_exit=$?
python3 -m json.tool /tmp/taskforge-result.json
printf 'CLI exit: %s\n' "$taskforge_exit"
```

Its expected exit is zero with `wrong_answer` in the JSON. Python syntax errors
are Runtime Error; no separate compilation verdict exists. Stdout comparison
uses exact byte tokens separated by six ASCII whitespace bytes, with no numeric
tolerance. Both output streams share one byte budget, but only stdout is compared.

Subprocess/network/host-file fixtures are access probes with diagnostic markers,
not ordinary accepted sum solutions. Verify them with the restricted native
harness, which creates/checks readable host-only markers before testing denial:

```sh
TASKFORGE_REAL_CONFIG="$PWD/examples/judge.local.json" \
  cargo test -p taskforge-execution --offline --locked --test sandbox_real \
  -- --ignored --nocapture
```

## Timing, shutdown and scope

Defaults are 1 second CPU, 3 seconds wall, 128 MiB memory, 1 MiB combined output
and one submitted process per test. The whole-evaluation default is 60 seconds,
shared across tests, setup and cleanup, with a cleanup reserve. Per-test timeouts
are user verdicts; the overall deadline is an infrastructure error. Unknown
measurements remain null. Retained diagnostics are capped at 4 KiB before JSON
escaping and execution metadata at 16 KiB.

Loading files, startup readiness and staging the cached runtime happen before an
evaluation is admitted. The CLI explicitly shuts down its runner before reporting:
it waits for any late cleanup workers and removes the cached runtime image on a
blocking worker. This shutdown has a separate 15-second grace, which can extend
command lifetime after an infrastructure deadline. Other runner/backend/session
clones must be dropped before shutdown. Cleanup failure supersedes a successful
verdict and is reported as infrastructure failure. The prior infrastructure
reason is retained in the diagnostic when shutdown also fails.

Drop remains best effort; explicit shutdown is necessary for a short-lived
process. Uninterruptible filesystem/kernel failure is outside a hard real-time
guarantee. See the [supervision guide](supervision.md). There is no ordinary-host
execution fallback. HTTP, databases, queues and distributed workers remain later
phases; broader reliability integration is issue #10.

The [learning guide](learning/issue-9.md) and
[verification status](setup/issue-9-verification.md) distinguish what has run from
the recorded native verification scope.

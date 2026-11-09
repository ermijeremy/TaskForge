# Issue #3 — Readiness walkthrough

Read [the setup guide](../setup/linux-isolate.md), then these files:

1. `config.rs`: raw `SandboxSettings`, range validation and path resolution.
2. `readiness.rs`: typed errors, `Result` propagation and command construction.
3. `smoke.py`: a fixed trusted probe, executed only through Isolate.
4. `tests/readiness.rs`: fake-tool protocol tests versus the explicit real test.

The key distinction is between static validation and observed readiness. A file
can exist, be executable, and print a version while cgroup delegation or mount
permissions are still unusable. Real init/run/cleanup supplies evidence those
operations work on this host. The later runner must still implement its own
per-test lifecycle and policy; a past readiness check is not permanent proof.

Explain why locks must remain held across init, run and cleanup, why removing a
lock file can break coordination, and why successful cleanup is part of readiness.
Follow the errors from a failed init, timed-out probe and failed cleanup. Explain
why `?` inside the inner closure still permits cleanup in the outer function.
Then explain why the ignored real test and mocked tests prove different things.

Optional Rust exercise: extend the fake-tool tests with duplicate `exitcode:0`
metadata lines and explain why the public check must reject them. Run only that
regression while iterating, then the normal formatting, lint and workspace checks.
Do not replace the real integration run with a mocked success.

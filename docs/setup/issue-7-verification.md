# Issue #7 verification status

**Status: output accounting and supervised protocol tests verified; native
supervision verification pending.**

`OutputBudget::record` now counts both streams against one configured budget,
retains only the permitted prefix within the 1 MiB ceiling, saturates the observed
counter on overflow and records excess without returning an accounting error.

Verified on this branch:

- `cargo test --workspace --offline --locked`: **90 passed, 4 ignored**.
- Formatting and Clippy with `-D warnings`: passed.
- All seven public accounting tests and the private overflow test pass normally.
- All three supervised protocol tests pass normally: capture and error cleanup,
  stdout/stderr/mixed excess, explicit cancellation, aborted waiters and reuse.

Normal checks also cover absolute deadline sharing, cancellation during setup,
cleanup after setup deadline expiry, late cleanup recovery/quarantine and output
capacity rejection. A worker test checks that the response deadline signals
cancellation while the worker retains ownership. Trusted protocol tests are not
native isolation evidence.

The four host-dependent checks remain ignored by default. The new native
supervision test covers output floods, CPU/wall timeouts, evaluation deadline
expiry, explicit cancellation, aborted waiters and same-slot reuse. It must pass
on the supported host before issue #7 can be considered fully verified.

The earlier [issue #6 host verification](issue-6-host-verification.md) still
covers its recorded lifecycle/access probes; it does not prove the new output,
deadline or async cancellation path. Commands are in the
[issue #7 learning guide](../learning/issue-7.md). Keep verification records focused
on project requirements and observed behavior.

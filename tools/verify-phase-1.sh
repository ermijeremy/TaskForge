#!/usr/bin/env bash
# Run as the ordinary judge user on the provisioned native host, with idle IDs.
set -euo pipefail
: "${TASKFORGE_REAL_CONFIG:?Set TASKFORGE_REAL_CONFIG to the supported native-host JSON configuration}"
taskforge_config=$(realpath -- "${TASKFORGE_REAL_CONFIG}")
taskforge_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
export TASKFORGE_REAL_CONFIG=${taskforge_config}
cd -- "${taskforge_root}"
printf 'Phase 1 verification: host/runtime observations\n'
uname -sr
rustc --version
/usr/bin/python3.14 --version
/usr/local/bin/isolate --version
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --offline --locked -- -D warnings
cargo build --workspace --all-targets --offline --locked
cargo test --workspace --offline --locked
python3 tools/check-python-fixtures.py
target/debug/judge-local check-config --config "${TASKFORGE_REAL_CONFIG}"
target/debug/judge-local check-ready --json --config "${TASKFORGE_REAL_CONFIG}"
# These run only native tests: no passing test is inferred from an ignored skip.
cargo test --workspace --offline --locked -- --ignored --test-threads=1 --nocapture
python3 tools/check-clean-slots.py --config "${TASKFORGE_REAL_CONFIG}"
printf 'Phase 1 verification passed.\n'

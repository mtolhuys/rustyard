#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
cargo fmt --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo check --locked --manifest-path examples/hello/Cargo.toml
bash -n scripts/install.sh scripts/activate.sh

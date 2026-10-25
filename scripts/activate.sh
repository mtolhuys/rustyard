#!/usr/bin/env bash
# Explicit host activation: links only the bundled example, never other projects.
set -euo pipefail
project_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
install_prefix="${RUSTYARD_PREFIX:-$HOME/.local}"
example_url="${1:-https://hello-rust.test}"
"$project_root/scripts/install.sh" "$install_prefix"
runner="$install_prefix/bin/rustyard"
"$runner" doctor
example_root="$project_root/examples/hello"
if [[ ! -f "$example_root/.rustyard.toml" ]]; then
  "$runner" -C "$example_root" init --name hello-rust
fi
"$runner" -C "$example_root" up --yes --url "$example_url/health" --timeout 180
"$runner" verify --url "$example_url/health" --contains ok --timeout 5
"$runner" -C "$example_root" open
printf '\nExample is running. Logs: rustyard -C %q logs --follow\n' "$example_root"

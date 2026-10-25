#!/usr/bin/env bash
set -euo pipefail
project_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
install_prefix="${1:-$HOME/.local}"
if [[ "$install_prefix" != /* ]]; then
  printf 'Installation prefix must be an absolute path.\n' >&2
  exit 1
fi
cargo install --locked --path "$project_root" --root "$install_prefix"
"$install_prefix/bin/rustyard" doctor --tools-only
printf '\nInstalled: %s/bin/rustyard\nEnsure %s/bin is on PATH.\n' "$install_prefix" "$install_prefix"

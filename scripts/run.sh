#!/usr/bin/env bash
set -euo pipefail
folio_root="$(cd "$(dirname "$0")/.." && pwd)"
# Resolve the input before changing directories, so relative paths work anywhere.
if [ "$#" -gt 0 ] && [ "${1#/}" = "$1" ]; then
  set -- "$PWD/$1" "${@:2}"
fi
cd "$folio_root"
export CARGO_HOME="${CARGO_HOME:-$folio_root/.cargo-local}"
exec cargo run --offline --locked --release --manifest-path desktop/Cargo.toml -- "$@"

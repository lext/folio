#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_HOME="${CARGO_HOME:-$PWD/.cargo-local}"
folio_desktop=false
folio_tex=false
for folio_argument in "$@"; do
  case "$folio_argument" in
    --desktop) folio_desktop=true ;;
    --tex) folio_tex=true ;;
    --help|-h)
      echo 'Usage: scripts/check.sh [--desktop] [--tex]'
      echo 'Default: core formatting, unit tests and Clippy; no TeX/PDFium required.'
      echo '--desktop adds desktop checks. --tex enables real TeX/PDF integrations.'
      exit 0 ;;
    *) echo "Unknown option: $folio_argument (try --help)" >&2; exit 2 ;;
  esac
done
cargo fmt --check
cargo test --offline --locked
cargo clippy --offline --locked --all-targets -- -D warnings
if "$folio_tex"; then
  cargo test --offline --locked --test tex_integration -- --ignored
fi
if "$folio_desktop"; then
  if [ "$(uname -s)" = Darwin ]; then
    # Compile the Metal callback dependency as the primary package so Cargo's
    # dependency lint cap cannot hide an invalid foreign runtime declaration.
    cargo rustc --offline --locked --manifest-path desktop/Cargo.toml -p block --lib -- -D future-incompatible
    cargo test --offline --locked --manifest-path vendor/block/Cargo.toml
  fi
  cargo fmt --manifest-path desktop/Cargo.toml --check
  cargo test --offline --locked --manifest-path desktop/Cargo.toml
  cargo clippy --offline --locked --manifest-path desktop/Cargo.toml --all-targets -- -D warnings
  if "$folio_tex"; then
    # Do not compile into an example a contributor may have open in the editor.
    folio_fixture="$(mktemp -d "${TMPDIR:-/tmp}/folio-check.XXXXXX")"
    trap 'rm -rf "$folio_fixture"' EXIT
    cp -R examples/welcome/{main.tex,folio.project.toml,references.bib,sections,figures,tables} "$folio_fixture/"
    cargo run --offline --locked --bin folio-cli -- build "$folio_fixture"
    FOLIO_TEST_PDF="$folio_fixture/.folio/main.pdf" cargo test --offline --locked --manifest-path desktop/Cargo.toml -- --ignored --skip profile_
  fi
fi

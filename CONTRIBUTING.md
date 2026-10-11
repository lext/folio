# Contributing to Folio

Folio is a native Rust editor for local LaTeX projects. Keep changes focused on
fast editing, reliable builds and a minimal interface. Discuss larger changes in
an issue before introducing dependencies or changing the project format.

## Development setup

Use Rust 1.90 or newer, a C compiler and Git. The [README](README.md#build-and-run)
lists the platform libraries. Python 3 is needed only for repository tooling
tests. PDFium and TeX are optional for ordinary Rust unit tests.

From the repository root:

```sh
export CARGO_HOME="$PWD/.cargo-local"
cargo fetch --locked
cargo fetch --locked --manifest-path desktop/Cargo.toml
./scripts/check.sh
python3 -m unittest discover -s scripts/tests -v
./scripts/check.sh --desktop
```

Scripts use `.cargo-local/` unless `CARGO_HOME` is already set. Use the same value
when fetching and checking. Both Cargo lockfiles are committed; checks run with
`--offline --locked`. Update dependencies intentionally and include the affected
lockfile in the change.

For real typesetting, export and PDF tests, install `latexmk`, pdfLaTeX, XeLaTeX,
SyncTeX, BibTeX, Biber, BibLaTeX and `latexpand`, then:

```sh
./scripts/setup-pdfium.sh
./scripts/check.sh --desktop --tex
```

On Debian/Ubuntu the TeX packages are `latexmk`, `texlive-latex-base`,
`texlive-latex-recommended`, `texlive-xetex`, `texlive-extra-utils`,
`texlive-bibtex-extra` and `biber`. MacTeX includes the required tools; with
BasicTeX, use `tlmgr` to install `latexmk fontspec latexpand biblatex biber`.
The check script builds a disposable copy of the welcome project. It does not
typeset into an example you may have open. Manual profiling tests stay excluded.

## Test-driven changes

1. Add a regression for the behavior being changed and observe it fail.
2. Make the smallest implementation that passes, then refactor with tests green.
3. Run formatting, Clippy and the relevant suites above. For input/layout changes,
   exercise actual egui frames, including focus and configurable bindings.
4. Check visible UI changes in the native app. Report which platform was checked
   and any unavailable validation; headless tests do not replace native QA.

Prefer observable behavior over tests that repeat implementation details. Keep
fixtures small and deterministic. Use temporary projects for destructive test
operations. Do not test against a contributor's settings or open documents.

## Code map

| Location | Responsibility |
| --- | --- |
| `src/`, `tests/` | Document/project model, compilation, export, Git, SyncTeX and text indexes |
| `src/bin/folio-cli.rs` | Project and build CLI |
| `desktop/src/app/`, `desktop/src/app.rs` | Workspace orchestration and source UI |
| `desktop/src/language/` | Highlighting, brackets and prose navigation |
| `desktop/src/preview/`, `pdf_worker.rs`, `page_layout.rs` | Preview input, PDF work and geometry |
| `desktop/src/settings.rs`, `folio.toml` | Settings parser, actions and shipped defaults |
| `vendor/block/` | Documented upstream compatibility patch; preserve attribution |
| `scripts/`, `.github/workflows/` | Local checks, packaging and CI |

See [architecture](docs/architecture.md) for worker boundaries and state ownership.
Application shortcuts belong in `folio.toml`; avoid embedding new bindings in UI
code. Keep disk access, compilation and PDF rendering off the UI thread. Changes
to project schema or settings need compatibility tests and guide updates.

## Pull requests

Describe the user-visible problem, resulting behavior and checks performed.
Include a small reproduction for bug fixes and a screenshot for visible UI
changes. Keep generated output, runtime downloads, build caches and personal
projects out of commits. PDF figures are source assets and are not globally
ignored. Preserve the existing MIT license and third-party notices.

CI runs the same scripts on macOS and Linux. Its packaged archives retain
executable modes and macOS symlinks; [GitHub artifact transport](https://github.com/actions/upload-artifact#permission-loss)
does not preserve raw file permissions. A green macOS run does not establish
Linux runtime behavior.

## Packaging

```sh
./scripts/package.sh
```

The script builds the current host in release mode, reads the version from
`desktop/Cargo.toml`, includes PDFium and its notices, and preserves existing
external settings. It produces `dist/Folio.app` plus a macOS ZIP, or a Linux
directory plus a `.tar.gz`. Build from a clean checkout for shared artifacts so
local settings cannot enter the package. macOS builds have an ad-hoc signature,
not Developer ID signing or notarization.

For a release, update the desktop Cargo version and lockfile, run the full checks,
validate native behavior on both platforms, and inspect the resulting archive.
Repository preparation and CI do not publish a GitHub release automatically.

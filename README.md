# Folio

A minimal, local LaTeX editor for macOS and Linux. Written in Rust with
**egui + wgpu**, with no webview, account or server.

Write in one surface; switch to the PDF when you want to read. Folio saves and
compiles changed sources in the background, then reuses the preview until the
project changes again.

- LaTeX, BibTeX, TOML and JSON highlighting, completion and matching brackets.
- Searchable PDF preview with word selection and bidirectional SyncTeX navigation.
- Git change markers against HEAD or a chosen commit, including unsaved edits.
- Ordinary project files, a TOML build manifest, and verified flat LaTeX export.
- Configurable keyboard and mouse bindings; a project browser available on demand.

**Status:** actively developed. The native macOS app has been tested on Apple
Silicon. Linux build and packaging support are supplied; native Linux validation
is still outstanding. GPU acceleration covers UI and page composition; PDFium
rasterizes pages on a background CPU worker.

## Build and run

Requirements: **Rust 1.90+**, a C compiler, Git, and a TeX distribution with
`latexmk` and `synctex`. Bibliography/export features also use BibTeX or Biber and
`latexpand`. On macOS, install Xcode Command Line Tools and MacTeX (or BasicTeX
with the [additional packages](CONTRIBUTING.md#development-setup)).

On Debian/Ubuntu, install native windowing prerequisites:

```sh
sudo apt install build-essential pkg-config libxkbcommon-dev libwayland-dev \
  libxcb-shape0-dev libxcb-xfixes0-dev libssl-dev libegl1-mesa-dev \
  mesa-vulkan-drivers xdg-desktop-portal xdg-desktop-portal-gtk
```

From this repository:

```sh
export CARGO_HOME="$PWD/.cargo-local"
cargo fetch --locked
cargo fetch --locked --manifest-path desktop/Cargo.toml
./scripts/setup-pdfium.sh
./scripts/run.sh examples/welcome
```

The first setup needs network access. Editing and compilation then work offline
with the required TeX packages installed. Setup downloads PDFium **7881** into
`runtime/` and retains its licenses. To use a compatible existing library, set
`FOLIO_PDFIUM_LIB` to its `.dylib` or `.so` path when running Folio.

To create a native bundle and archive:

```sh
./scripts/package.sh
```

Output goes to `dist/`: `Folio.app` plus a ZIP on macOS, or a Linux directory and
`.tar.gz`. Packaging targets the current host architecture. macOS requires 12+
and uses a local ad-hoc signature; builds are not notarized. TeX is not bundled.

## Everyday use

Folio opens **projects**, not loose files. Use Cmd/Ctrl+N to create a project,
Cmd/Ctrl+O to open one, or pass a project directory to `scripts/run.sh`.

| Shortcut | Action |
| --- | --- |
| Cmd/Ctrl+1 / Cmd/Ctrl+2 | Source / PDF |
| Cmd/Ctrl+P | Switch file or PDF |
| Cmd/Ctrl+Shift+P | Search commands |
| Cmd/Ctrl+B | Toggle project browser |
| Cmd/Ctrl+F | Search the current surface |
| Cmd/Ctrl + double-click | Follow selected text between source and PDF |
| Tab / Shift+Tab, then Enter | Cycle completion suggestions, then accept |
| Cmd/Ctrl+D / Cmd/Ctrl+U | Paragraph end / start |
| Cmd/Ctrl+J / Cmd/Ctrl+K | Sentence end / start |
| Space / double-Space in PDF | Fit 90% of available width / fit the whole page |

The file browser and other panels are accounted for when fitting a PDF.
Right-click opens Commands. Line numbers are enabled by default.

Application preferences and bindings live in [folio.toml](folio.toml), loaded
beside the executable, then from `~/.config/folio/folio.toml`, then from bundled
defaults. The project’s `folio.project.toml` selects its main file, TeX engine
and build timeout. Sources, sections, figures and tables remain ordinary files.

## Documentation

- [User guide](docs/guide.md): projects, settings, navigation, Git and flat export.
- [Examples](examples/README.md): introductory, multi-file and performance projects.
- [Contributing](CONTRIBUTING.md): TDD workflow, dependencies, checks and packaging.
- [Architecture](docs/architecture.md): modules, worker boundaries and state ownership.
- [Validation record](docs/validation.md): checks performed and platform limitations.

For a quick core check, run `./scripts/check.sh`. Add `--desktop` for desktop
unit tests and `--tex` for real compiler/PDF integrations. CI uses the same scripts.

## Current limits

Sources must be UTF-8. LaTeX syntax and project completion are supported; a
package-aware LSP client and arbitrary macro semantic analysis are not. SyncTeX
needs a compiler-generated map and can fall back to a source line for generated
or ambiguous text. The PDF viewer focuses on generated LaTeX documents; OCR,
annotations, signatures and cross-page selection are not implemented.

## License

[MIT](LICENSE). Dependencies retain their own terms; see
[third-party components](THIRD_PARTY.md) and the vendored/runtime notices.

# Folio

A minimal, local LaTeX editor for macOS and Ubuntu. Written in Rust with
**egui + wgpu**, with no webview, account or server.

Write in one surface; switch to the PDF when you want to read. Folio saves and
compiles changed sources in the background, then reuses the preview until the
project changes again.

- LaTeX, BibTeX, TOML and JSON highlighting, completion and matching brackets.
- Searchable PDF preview with word selection and bidirectional SyncTeX navigation.
- Git change markers against HEAD or a chosen commit, including unsaved edits.
- Ordinary project files, a TOML build manifest, and verified flat LaTeX export.
- Configurable keyboard and mouse bindings; a project browser available on demand.

**Supported platforms:** macOS 12+ on Apple silicon, and Ubuntu 24.04 and 26.04
on x86_64. Other systems are not supported. There are no prebuilt binaries:
build Folio from source as described below.

**Status:** actively developed. The native macOS app has been tested on Apple
Silicon. CI compiles and tests it on every supported platform; interactive Linux
validation is still outstanding. GPU acceleration covers UI and page composition; PDFium
rasterizes pages on a background CPU worker.

## Build from source

Requirements: **Rust 1.90+**, a C compiler, Git, and a TeX distribution with
`latexmk` and `synctex`. Bibliography/export features also use BibTeX or Biber and
`latexpand`. On macOS, install Xcode Command Line Tools and MacTeX (or BasicTeX
with the [additional packages](CONTRIBUTING.md#development-setup)).

On Ubuntu, install native windowing prerequisites:

```sh
sudo apt install build-essential pkg-config libxkbcommon-dev libwayland-dev \
  libxcb-shape0-dev libxcb-xfixes0-dev libssl-dev libegl-dev \
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

To install a release build instead of running from the checkout:

```sh
./scripts/package.sh
```

Output goes to `dist/`: `Folio.app` on macOS (drag it to Applications), or a
`folio-linux-x86_64` directory on Ubuntu whose `folio` executable can be run in
place. Each comes with a ZIP or `.tar.gz` copy. macOS requires 12+ and uses a
local ad-hoc signature. TeX is not bundled.

## Everyday use

Folio opens **projects**, not loose files. Use Cmd/Ctrl+N to create a project,
Cmd/Ctrl+O to open one, or pass a project directory to `scripts/run.sh`.

| Shortcut | Action |
| --- | --- |
| Cmd/Ctrl+H | Shortcut sheet |
| Cmd/Ctrl+1 … 9 | Go to that tab (1 is `main`, then the sections) |
| Option/Alt+1 / Option/Alt+2 | Source / PDF |
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
defaults. The project’s `folio.project.toml` selects its TeX engine and build
timeout. Sources, sections, figures and tables remain ordinary files.

## Project structure

A project is a folder with `folio.project.toml` at its root. Cmd/Ctrl+N creates
this layout; everything in it is a plain, Git-friendly file:

```text
paper/
├── folio.project.toml      # project manifest: name, engine, timeout
├── main.tex                # the paper; \input{sections/...} pulls in sections
├── supplement.tex          # optional standalone supplement (own preamble)
├── references.bib          # BibTeX/BibLaTeX entries
├── sections/
│   └── introduction.tex    # chapter and section sources
├── figures/                # figure assets (PDF, PNG, ...)
├── tables/                 # table sources
├── bibliography/           # optional: further .bib files
├── styles/                 # optional: local .sty, .cls, .bst
├── data/                   # optional: supporting TOML/JSON/CSV data
├── README.md
├── .gitignore              # ignores .folio/, main.pdf, supplement.pdf
│
├── main.pdf                # build output, replaced only by a successful build
├── supplement.pdf          # build output, when supplement.tex exists
└── .folio/                 # LaTeX working files (aux, log, synctex)
```

The manifest is short and strictly validated; unknown fields are rejected:

```toml
schema = 1

[project]
name = "paper"
main = "main.tex"       # always main.tex

[build]
engine = "pdflatex"     # pdflatex, xelatex or lualatex
timeout_seconds = 120   # 0 to 86400; 0 disables the limit
```

Each build typesets `main.tex`, then `supplement.tex` if present. `preamble.tex`
may also live at the root. See [examples/](examples/README.md) for complete
projects and the [user guide](docs/guide.md) for bibliography and export details.

## Documentation

- [User guide](docs/guide.md): projects, the project file, bibliographies, settings, navigation, Git and flat export.
- [Examples](examples/README.md): introductory, multi-file and performance projects, plus BibTeX and BibLaTeX project templates.
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

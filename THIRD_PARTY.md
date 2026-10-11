# Third-party components

Folio's original source is covered by [MIT](LICENSE). Dependencies retain their
own licenses; Folio's license does not replace them. Exact Rust versions and
registry checksums are recorded in the root and desktop `Cargo.lock` files.

| Component | Use | License / source |
| --- | --- | --- |
| egui / eframe / wgpu | Native interface and GPU composition | MIT or Apache-2.0; [egui](https://github.com/emilk/egui), [wgpu](https://github.com/gfx-rs/wgpu) |
| pdfium-render | Rust PDFium interface | MIT or Apache-2.0; [source](https://github.com/ajrcarey/pdfium-render) |
| PDFium | Native PDF rasterization and text geometry | BSD-style and bundled third-party terms; [source](https://pdfium.googlesource.com/pdfium/) |
| pdfium-binaries | Prebuilt PDFium 7881 runtime | MIT packaging plus PDFium notices; [pinned release](https://github.com/bblanchon/pdfium-binaries/releases/tag/chromium%2F7881) |
| Tree-sitter / tree-sitter-latex | Incremental LaTeX parsing | MIT; [Tree-sitter](https://github.com/tree-sitter/tree-sitter), [grammar](https://github.com/latex-lsp/tree-sitter-latex) |
| block 0.1.6 | macOS native callbacks | MIT; [vendored license](vendor/block/LICENSE), [patch and provenance](vendor/block/PATCH.md) |
| epaint default fonts | Font fallback | MIT/Apache-2.0 plus OFL-1.1 and Ubuntu-font-1.0, as declared by the font package |

`scripts/setup-pdfium.sh` retains the downloaded runtime's `LICENSE` and
`licenses/` directory. Packaging copies those notices, Folio's license and this
index into the distribution. The macOS bundle also includes the vendored block
license. Preserve upstream notices when redistributing dependencies or changing
the runtime. This table is an index, not a replacement for license texts.

The compatibility scanner in `desktop/native/latex_scanner.c` is Folio code
implementing Tree-sitter's external-scanner ABI. TeX, `latexmk`, `latexpand`, Git
and system fonts are used from the host installation and are not bundled.

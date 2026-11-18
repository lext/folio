# LaTeX paper

Build settings are in folio.project.toml.

- `main.tex`: the paper; its sections are in `sections/`.
- `supplement.tex`: Supplementary Information, a separate document built with the paper.
  It is shown after the paper in Folio's preview.
- `figures/`: figure PDFs (included by name) and TikZ sources; `tables/`: one file per table.
- `references.bib`: bibliography for both documents.

Cite supplementary items from the paper as `Supplementary \cref{S-tab:...}`, and
main-text items from the supplement as `\cref{M-fig:...}`. `\tbd{...}` marks text
still to be supplied in red.

Builds write main.pdf and supplement.pdf beside their sources; .folio/ holds working files. Do not edit either.

Build: folio-cli build .
Validate: folio-cli project check .
Export: folio-cli project export . ../submission
Journal archive without the supplement: folio-cli project export . ../journal.zip --without-supplement

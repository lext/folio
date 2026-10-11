# BibTeX project template

A minimal Folio project that cites with classic BibTeX. Copy the folder, or copy
`folio.project.toml` into your own project and keep the same layout.

| File | Role |
| --- | --- |
| `folio.project.toml` | Project settings, with every key explained |
| `main.tex` | `\bibliographystyle{plain}` and `\bibliography{references,bibliography/books}` |
| `references.bib` | The default bibliography file |
| `bibliography/books.bib` | An additional bibliography file |

Open it with `scripts/run.sh examples/bibtex`, or build it from a terminal:

```sh
folio-cli build examples/bibtex
```

The project file has no bibliography option. Folio runs `latexmk`, which runs
BibTeX whenever `main.tex` contains `\bibliography{...}`, then reruns LaTeX until
the citations settle. You need `bibtex` from your TeX distribution; it is part of
every TeX Live and MacTeX installation.

For BibLaTeX with Biber, see [../biblatex](../biblatex/).

# BibLaTeX project template

A minimal Folio project that cites with BibLaTeX and the Biber backend. Copy the
folder, or copy `folio.project.toml` into your own project and keep the same layout.

| File | Role |
| --- | --- |
| `folio.project.toml` | Project settings, with every key explained |
| `main.tex` | `\usepackage[backend=biber]{biblatex}`, `\addbibresource{...}` and `\printbibliography` |
| `references.bib` | The default bibliography file |
| `bibliography/books.bib` | An additional bibliography file |

Open it with `scripts/run.sh examples/biblatex`, or build it from a terminal:

```sh
folio-cli build examples/biblatex
```

The project file has no bibliography option. Folio runs `latexmk`, which runs
Biber whenever `main.tex` loads `biblatex` with `backend=biber`, then reruns LaTeX
until the citations settle. You need the `biblatex` package and the `biber`
program: `tlmgr install biblatex biber` with BasicTeX, or the `biber` and
`texlive-bibtex-extra` packages on Debian/Ubuntu. If Biber is missing, the build
fails and the compiler output names it.

For classic BibTeX, see [../bibtex](../bibtex/).

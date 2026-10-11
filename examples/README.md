# Example projects

Open these directories in Folio, or pass one to `scripts/run.sh`:

| Project | Purpose |
| --- | --- |
| [welcome](welcome/) | Two-page introduction, mathematics, search and navigation |
| [layers](layers/) | Included section, references and BibTeX citations |
| [bibtex](bibtex/) | Template: commented project file and a BibTeX bibliography |
| [biblatex](biblatex/) | Template: commented project file and a BibLaTeX bibliography built with Biber |
| [performance](performance/) | Repeatable 120-page scrolling and zooming fixture |

Each has a `folio.project.toml` and the required folders. Generated output goes
in its ignored `.folio/` directory. The standalone source used by desktop unit
tests lives in `desktop/tests/fixtures/`; it is not an openable project.

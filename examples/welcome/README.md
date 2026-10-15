# LaTeX project

Build settings are in folio.project.toml.
Write sections in sections/, figures in figures/, and table sources in tables/.
Keep bibliography entries in references.bib or bibliography/.
Optional styles/ and data/ hold local packages and supporting data.
Builds write the PDFs to build/main.pdf and build/supplement.pdf; .folio/ holds working files. Do not edit either.

Build: folio-cli build .
Validate: folio-cli project check .
Export: folio-cli project export . ../submission

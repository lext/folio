//! The files of a new project, from `templates/paper/`: a two-column paper with
//! a separate supplement, cross-references between them, a figure, tables and
//! a bibliography, ready for a flat export with or without the supplement.
//! `templates/paper/` is itself an openable project; its `folio.project.toml`
//! is not copied, since a new project's manifest carries its own name and
//! engine.

/// Paths relative to the project root, and their contents.
pub(super) const FILES: &[(&str, &[u8])] = &[
    (
        ".gitignore",
        include_bytes!("../../templates/paper/.gitignore"),
    ),
    (
        "README.md",
        include_bytes!("../../templates/paper/README.md"),
    ),
    (
        "figures/example.pdf",
        include_bytes!("../../templates/paper/figures/example.pdf"),
    ),
    (
        "figures/flow.tex",
        include_bytes!("../../templates/paper/figures/flow.tex"),
    ),
    ("main.tex", include_bytes!("../../templates/paper/main.tex")),
    (
        "references.bib",
        include_bytes!("../../templates/paper/references.bib"),
    ),
    (
        "sections/declarations.tex",
        include_bytes!("../../templates/paper/sections/declarations.tex"),
    ),
    (
        "sections/discussion.tex",
        include_bytes!("../../templates/paper/sections/discussion.tex"),
    ),
    (
        "sections/introduction.tex",
        include_bytes!("../../templates/paper/sections/introduction.tex"),
    ),
    (
        "sections/methods.tex",
        include_bytes!("../../templates/paper/sections/methods.tex"),
    ),
    (
        "sections/results.tex",
        include_bytes!("../../templates/paper/sections/results.tex"),
    ),
    (
        "supplement.tex",
        include_bytes!("../../templates/paper/supplement.tex"),
    ),
    (
        "tables/results.tex",
        include_bytes!("../../templates/paper/tables/results.tex"),
    ),
    (
        "tables/supplementary_details.tex",
        include_bytes!("../../templates/paper/tables/supplementary_details.tex"),
    ),
];

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_str().unwrap();
            if path.is_dir() {
                if name != ".folio" {
                    walk(root, &path, out);
                }
            } else if ![
                "folio.project.toml",
                "main.pdf",
                "supplement.pdf",
                ".DS_Store",
            ]
            .contains(&name)
            {
                out.push(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .replace('\\', "/"),
                );
            }
        }
    }

    #[test]
    fn every_template_file_is_written_into_new_projects() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/paper");
        let mut found = Vec::new();
        walk(&root, &root, &mut found);
        found.sort();
        let listed: Vec<_> = super::FILES.iter().map(|(p, _)| p.to_string()).collect();
        assert_eq!(
            listed, found,
            "update src/project/template.rs to match templates/paper/"
        );
    }
}

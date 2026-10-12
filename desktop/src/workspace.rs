use std::path::{Path, PathBuf};
/// The tab one step right (or left) of `current`, wrapping at either end.
pub fn step(current: usize, tabs: usize, backwards: bool) -> Option<usize> {
    (tabs > 0).then(|| (current.min(tabs - 1) + if backwards { tabs - 1 } else { 1 }) % tabs)
}

/// The files shown as tabs: the root document, every `.tex` file under
/// `sections/` in the order the root inputs it (unreferenced ones last, by
/// path), then any other open file in the order it was opened.
pub fn tabs(root: &Path, root_text: &str, files: &[PathBuf], open: &[PathBuf]) -> Vec<PathBuf> {
    let Some(directory) = root.parent() else {
        return vec![];
    };
    let sections = directory.join("sections");
    let mut ordered: Vec<(usize, &PathBuf)> = files
        .iter()
        .filter(|f| f.starts_with(&sections) && f.extension().is_some_and(|e| e == "tex"))
        .map(|f| {
            let name = f
                .strip_prefix(directory)
                .unwrap_or(f)
                .with_extension("")
                .to_string_lossy()
                .replace('\\', "/");
            let position = [format!("{{{name}}}"), format!("{{{name}.tex}}")]
                .iter()
                .filter_map(|argument| root_text.find(argument))
                .min();
            (position.unwrap_or(usize::MAX), f)
        })
        .collect();
    ordered.sort();
    let mut result = vec![root.to_path_buf()];
    for path in ordered.into_iter().map(|(_, f)| f).chain(open) {
        if !result.contains(path) {
            result.push(path.clone());
        }
    }
    result
}

/// Whitespace-separated terms may match either a title or its path/description.
pub fn matches(query: &str, title: &str, detail: &str) -> bool {
    let haystack = format!("{title} {detail}").to_lowercase();
    query
        .split_whitespace()
        .all(|term| haystack.contains(&term.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stepping_wraps_through_the_tabs_in_both_directions() {
        assert_eq!(
            [0, 1, 2].map(|i| step(i, 3, false)),
            [Some(1), Some(2), Some(0)]
        );
        assert_eq!(
            [0, 1, 2].map(|i| step(i, 3, true)),
            [Some(2), Some(0), Some(1)]
        );
        assert_eq!(step(0, 1, false), Some(0));
        assert_eq!(step(0, 0, false), None);
    }
    #[test]
    fn tabs_follow_the_order_the_root_inputs_its_sections() {
        let p = |s: &str| PathBuf::from("/paper").join(s);
        let files = [
            p("main.tex"),
            p("references.bib"),
            p("sections/appendix.tex"),
            p("sections/draft.tex"),
            p("sections/intro.tex"),
            p("sections/methods.tex"),
            p("sections/notes.md"),
            p("supplement.tex"),
        ];
        let text =
            "\\input{sections/intro}\n\\input{sections/methods.tex}\n\\include{sections/appendix}";
        assert_eq!(
            tabs(
                &p("main.tex"),
                text,
                &files,
                &[p("references.bib"), p("sections/intro.tex")]
            ),
            [
                "main.tex",
                "sections/intro.tex",
                "sections/methods.tex",
                "sections/appendix.tex",
                "sections/draft.tex",
                "references.bib"
            ]
            .map(p)
        );
    }
    #[test]
    fn tabs_cope_with_nested_sections_missing_sections_and_repeated_files() {
        let p = |s: &str| PathBuf::from("/paper").join(s);
        // Nested folders count as sections; look-alike names do not.
        let files = [
            p("main.tex"),
            p("sections/b/part.tex"),
            p("sections/a/part.tex"),
            p("sections-old/x.tex"),
            p("tables/sections/t.tex"),
        ];
        assert_eq!(
            tabs(&p("main.tex"), "\\input{sections/b/part}", &files, &[]),
            ["main.tex", "sections/b/part.tex", "sections/a/part.tex"].map(p)
        );
        // An input of a longer name must not order a shorter one.
        let files = [p("main.tex"), p("sections/in.tex"), p("sections/intro.tex")];
        assert_eq!(
            tabs(&p("main.tex"), "\\input{sections/intro}", &files, &[]),
            ["main.tex", "sections/intro.tex", "sections/in.tex"].map(p)
        );
        // The root is listed once even when open, and before any scan.
        assert_eq!(
            tabs(&p("main.tex"), "", &[], &[p("main.tex"), p("main.tex")]),
            [p("main.tex")]
        );
        assert_eq!(
            tabs(Path::new("/"), "", &[], &[]),
            Vec::<PathBuf>::new(),
            "a root without a folder has no project to list"
        );
    }
    #[test]
    fn launcher_matches_all_terms_across_names_and_paths() {
        assert!(matches("", "chapter.tex", "Open · chapters/chapter.tex"));
        assert!(matches(
            "CHAPTER tex",
            "chapter.tex",
            "Open · chapters/chapter.tex"
        ));
        assert!(matches(
            "résumé open",
            "résumé.tex",
            "Open · chapters/résumé.tex"
        ));
        assert!(!matches(
            "chapter bib",
            "chapter.tex",
            "Open · chapters/chapter.tex"
        ));
        assert!(matches("pdf", "Preview", "main.pdf"));
    }
}

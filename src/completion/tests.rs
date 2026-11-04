use super::*;
use crate::bibliography;

/// Complete at `|` in `text` with the given extra project text, bibliography
/// and files; returns the suggestions and the text after accepting `pick`.
struct Case {
    suggestions: Option<Suggestions>,
    text: String,
}
impl Case {
    fn new(marked: &str, project: &str, bib: &str, files: &[&str]) -> Self {
        let cursor = marked.find('|').expect("caret marker");
        let text = marked.replacen('|', "", 1);
        let active = Index::source(&text);
        let project = Index::source(project);
        let bibliography = Index::bibliography(&bibliography::parse(bib));
        let files: Vec<String> = files.iter().map(|f| f.to_string()).collect();
        let suggestions = suggest_in(
            &text,
            cursor,
            &active,
            &[&active, &project, &bibliography],
            &files,
        );
        Self { suggestions, text }
    }
    fn at(marked: &str) -> Self {
        Self::new(marked, "", "", &[])
    }
    fn labels(&self) -> Vec<&str> {
        self.suggestions.as_ref().map_or(vec![], |s| {
            s.items.iter().map(|i| i.label.as_str()).collect()
        })
    }
    fn first(&self) -> &Item {
        &self.suggestions.as_ref().expect("suggestions").items[0]
    }
    /// The text after accepting the item labelled `label`, with `|` at the caret.
    fn accept(&self, label: &str) -> String {
        let s = self.suggestions.as_ref().expect("suggestions");
        let item = s
            .items
            .iter()
            .find(|i| i.label == label)
            .unwrap_or_else(|| panic!("{label} not in {:?}", self.labels()));
        let mut text = self.text.clone();
        text.replace_range(s.range.clone(), &item.insert);
        text.insert(s.range.start + item.cursor, '|');
        text
    }
}

#[test]
fn argument_items_are_replaced_whole_even_with_stars_and_slashes() {
    let case = Case::at("\\begin{align*|}");
    assert_eq!(case.accept("align*"), "\\begin{align*|}");
    let case = Case::new(
        "\\cite{10.1000/|}",
        "",
        "@article{10.1000/xyz, title={T}}",
        &[],
    );
    assert_eq!(case.accept("10.1000/xyz"), "\\cite{10.1000/xyz|}");
}

#[test]
fn end_offers_the_open_environment_first_and_closes_it() {
    let case = Case::at("\\begin{itemize}\n  \\begin{enumerate}\n  \\end{|");
    assert_eq!(case.labels()[..2], ["enumerate", "itemize"]);
    assert_eq!(
        case.accept("enumerate"),
        "\\begin{itemize}\n  \\begin{enumerate}\n  \\end{enumerate}|"
    );
}

#[test]
fn begin_inserts_the_whole_environment_with_the_caret_inside() {
    let case = Case::at("  \\begin{ite|");
    assert_eq!(
        case.accept("itemize"),
        "  \\begin{itemize}\n    \\item |\n  \\end{itemize}"
    );
    let case = Case::at("\\begin{tab|");
    assert_eq!(
        case.accept("tabular"),
        "\\begin{tabular}{|}\n\n\\end{tabular}"
    );
    // Renaming an existing environment changes the name only.
    let case = Case::at("\\begin{ite|}\n\\end{itemize}");
    assert_eq!(case.accept("itemize"), "\\begin{itemize|}\n\\end{itemize}");
}

#[test]
fn citations_search_metadata_with_folded_accents_and_several_words() {
    let bib = r#"@article{smith2020, title={Deep nets}, author={M{\"u}ller, J.}, year={2020}}
@book{knuth84, title={The TeXbook}, author={Donald Knuth}, year={1984}}"#;
    for query in ["müller", "muller", "MULLER", "deep 2020", "mul nets"] {
        let case = Case::new(&format!("\\cite{{{query}|"), "", bib, &[]);
        assert_eq!(case.labels(), ["smith2020"], "{query}");
    }
    let case = Case::new("\\cite{knuth84, |}", "", bib, &[]);
    assert_eq!(
        case.labels(),
        ["smith2020"],
        "keys already cited are not offered again"
    );
    let all = Case::new("\\cite{|", "", bib, &[]);
    let smith = all
        .suggestions
        .as_ref()
        .unwrap()
        .items
        .iter()
        .find(|i| i.label == "smith2020")
        .unwrap();
    assert_eq!(smith.detail, "Müller, J. · Deep nets · 2020");
}

#[test]
fn every_cite_variant_completes_keys() {
    let bib = "@book{knuth84, title={The TeXbook}}";
    for command in [
        "citealp",
        "citeauthor",
        "Textcite",
        "parencite",
        "footcite",
        "cites",
        "nocite",
    ] {
        let case = Case::new(&format!("\\{command}{{kn|"), "", bib, &[]);
        assert_eq!(case.labels(), ["knuth84"], "{command}");
    }
}

#[test]
fn labels_match_inside_keys_and_show_their_context() {
    let project = "\\section{Results}\n\\begin{figure}\n\\caption{Accuracy over epochs}\n\\label{fig:results}\n\\end{figure}\n\\begin{equation}\\label{eq:loss}x\\end{equation}";
    let case = Case::new("\\ref{results|", project, "", &[]);
    assert_eq!(case.labels(), ["fig:results"]);
    let detail = &case.first().detail;
    assert!(
        detail.starts_with("Figure · Accuracy over epochs"),
        "{detail}"
    );
    let case = Case::new("\\eqref{|", project, "", &[]);
    assert_eq!(case.labels()[0], "eq:loss", "eqref ranks equations first");
    for command in ["cref", "Cref", "autoref", "nameref", "cpageref"] {
        assert!(
            !Case::new(&format!("\\{command}{{fig|"), project, "", &[])
                .labels()
                .is_empty()
        );
    }
    assert_eq!(
        Case::new("\\hyperref[fig|", project, "", &[]).labels(),
        ["fig:results"]
    );
}

#[test]
fn section_labels_describe_the_section() {
    let project = "\\section{Related Work}\n\\label{sec:related}";
    let case = Case::new("\\ref{sec|", project, "", &[]);
    assert!(case.first().detail.starts_with("Section · Related Work"));
}

#[test]
fn label_suggests_a_conventional_key_for_its_context() {
    assert_eq!(
        Case::at("\\section{Related Work}\n\\label{|").labels(),
        ["sec:related-work"]
    );
    assert_eq!(
        Case::at("\\begin{figure}\\label{|}\\end{figure}").labels(),
        ["fig:"]
    );
    assert_eq!(
        Case::at("\\begin{align}x\\label{|}\\end{align}").labels(),
        ["eq:"]
    );
    assert!(Case::at("Text \\label{|").labels().is_empty());
}

#[test]
fn commands_match_case_insensitively_and_by_abbreviation() {
    assert!(Case::at("\\Sec|").labels().contains(&"\\section"));
    assert!(Case::at("\\sbsec|").labels().contains(&"\\subsection"));
    assert!(Case::at("\\textsc|").labels().contains(&"\\textsc"));
    assert_eq!(Case::at("\\sec|").labels()[0], "\\section");
}

#[test]
fn commands_insert_their_arguments_with_the_caret_in_the_first() {
    assert_eq!(Case::at("$\\fra|$").accept("\\frac"), "$\\frac{|}{}$");
    assert_eq!(Case::at("\\textbf|").accept("\\textbf"), "\\textbf{|}");
    assert_eq!(Case::at("\\section|").accept("\\section"), "\\section{|}");
    assert_eq!(Case::at("\\maketit|").accept("\\maketitle"), "\\maketitle|");
    // Arguments already there: replace the name only.
    assert_eq!(Case::at("\\textb|{x}").accept("\\textbf"), "\\textbf|{x}");
}

#[test]
fn math_and_text_commands_rank_by_mode() {
    let math = Case::at("$\\s|$");
    let text = Case::at("\\s|");
    let position =
        |case: &Case, label| case.labels().iter().position(|l| *l == label).unwrap_or(99);
    assert!(position(&math, "\\sum") < position(&math, "\\section"));
    assert!(position(&text, "\\section") < position(&text, "\\sum"));
    let display = Case::at("\\begin{equation}\n\\al|\n\\end{equation}");
    assert_eq!(display.labels()[0], "\\alpha");
}

#[test]
fn package_commands_need_their_package_unless_typed_exactly() {
    let without = Case::at("\\toprule|");
    assert!(
        without
            .first()
            .detail
            .contains("needs \\usepackage{booktabs}")
    );
    assert!(
        !Case::at("\\trule|").labels().contains(&"\\toprule"),
        "no fuzzy matches from unloaded packages"
    );
    let with = Case::new("\\toprule|", "\\usepackage{booktabs}", "", &[]);
    assert_eq!(with.first().detail, "booktabs");
    // Packages a class or package loads count as loaded.
    let mathtools = Case::new("$\\dfr|$", "\\usepackage{mathtools}", "", &[]);
    assert_eq!(
        mathtools.first().detail,
        "{numerator}{denominator} · amsmath"
    );
}

#[test]
fn project_macros_and_environments_complete_with_their_arguments() {
    let project = "\\newcommand{\\R}{\\mathbb{R}}\n\\newcommand\\norm[1]{\\lVert#1\\rVert}\n\\DeclareMathOperator{\\Tr}{Tr}\n\\NewDocumentCommand{\\pair}{mm}{(#1,#2)}\n\\newtheorem{lemma}{Lemma}\n\\newenvironment{note}{}{}";
    let case = Case::new("$\\nor|$", project, "", &[]);
    assert_eq!(case.accept("\\norm"), "$\\norm{|}$");
    assert!(
        case.suggestions.unwrap().items[0]
            .detail
            .contains("1 argument")
    );
    assert_eq!(
        Case::new("$\\pai|$", project, "", &[]).accept("\\pair"),
        "$\\pair{|}{}$"
    );
    assert_eq!(Case::new("$\\Tr|$", project, "", &[]).first().label, "\\Tr");
    let env = Case::new("\\begin{lem|", project, "", &[]);
    assert_eq!(env.first().label, "lemma");
    assert!(env.first().detail.contains("Lemma"));
    assert!(
        Case::new("\\begin{no|", project, "", &[])
            .labels()
            .contains(&"note")
    );
    let label = Case::new("\\begin{lemma}\\label{|}", project, "", &[]);
    assert_eq!(label.labels(), ["lem:"]);
}

#[test]
fn files_complete_with_the_extension_rules_of_each_command() {
    let files = [
        "main.tex",
        "sections/introduction.tex",
        "figures/plot.pdf",
        "figures/photo.png",
        "references.bib",
        "styles/local.sty",
    ];
    let case = Case::new("\\input{intro|", "", "", &files);
    assert_eq!(
        case.accept("sections/introduction"),
        "\\input{sections/introduction|"
    );
    let case = Case::new(
        "\\includegraphics[width=\\linewidth]{plot|}",
        "",
        "",
        &files,
    );
    assert_eq!(case.labels(), ["figures/plot.pdf"]);
    assert_eq!(
        Case::new("\\addbibresource{|", "", "", &files).labels(),
        ["references.bib"]
    );
    assert_eq!(
        Case::new("\\bibliography{|", "", "", &files).labels(),
        ["references"]
    );
    let packages = Case::new("\\usepackage{loc|", "", "", &files);
    assert_eq!(packages.labels()[0], "styles/local");
    assert!(Case::at("\\usepackage{amsm|").labels().contains(&"amsmath"));
    assert!(
        Case::at("\\usepackage{amsmath,boo|}")
            .labels()
            .contains(&"booktabs")
    );
}

#[test]
fn options_colors_and_classes_complete() {
    assert!(
        Case::at("\\includegraphics[wi|]{x}")
            .labels()
            .contains(&"width=")
    );
    assert_eq!(Case::at("\\begin{figure}[|").first().label, "htbp");
    assert!(
        Case::at("\\documentclass[11|]{article}")
            .labels()
            .contains(&"11pt")
    );
    assert!(
        Case::at("\\documentclass{art|")
            .labels()
            .contains(&"article")
    );
    let colors = Case::new(
        "\\textcolor{|",
        "\\definecolor{brand}{HTML}{336699}",
        "",
        &[],
    );
    assert_eq!(colors.first().label, "brand");
    assert!(
        Case::at("\\setlength{\\par|")
            .labels()
            .contains(&"\\parindent")
    );
}

#[test]
fn nothing_is_offered_in_prose_comments_verbatim_or_after_a_line_break() {
    assert!(Case::at("ordinary prose|").suggestions.is_none());
    assert!(Case::at("% \\cite{|").suggestions.is_none());
    assert!(Case::at("\\begin{verbatim}\n\\sec|").suggestions.is_none());
    assert!(
        Case::at("a \\\\sec|").suggestions.is_none(),
        "\\\\ is a line break"
    );
    assert!(Case::at("\\|").suggestions.is_none());
    assert!(Case::at("see [1, |").suggestions.is_none());
}

#[test]
fn frequently_used_project_commands_rank_higher() {
    let project = "\\textsc{a} \\textsc{b} \\textsc{c}";
    let case = Case::new("\\text|", project, "", &[]);
    assert_eq!(case.first().label, "\\textsc");
}

#[test]
fn completion_never_panics_on_any_prefix_of_a_real_document() {
    let text = include_str!("../../examples/welcome/main.tex");
    let active = Index::source(text);
    for (cursor, _) in text.char_indices() {
        let _ = suggest_in(text, cursor, &active, &[&active], &["main.tex".into()]);
    }
}

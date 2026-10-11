use folio_core::{bibliography, completion};

#[test]
fn trailing_commas_preserve_each_entry_and_its_fields() {
    let entries = bibliography::parse(
        r#"
@article{first, title={First}, author={One}, year=2020, % trailing comment
}
@book(second, title="Second", author={Two}, year=2021, )
@misc{third, title={Third}, }
"#,
    );
    assert_eq!(
        entries.iter().map(|e| e.key.as_str()).collect::<Vec<_>>(),
        ["first", "second", "third"]
    );
    assert_eq!(
        (&*entries[0].title, &*entries[0].author, &*entries[0].year),
        ("First", "One", "2020")
    );
    assert_eq!((&*entries[1].title, &*entries[1].author), ("Second", "Two"));
    let suggestions = completion::suggest("\\cite{", 6, "", &entries).unwrap();
    assert_eq!(suggestions.items.len(), 3);
}

#[test]
fn malformed_bib_field_recovers_at_next_entry_without_splitting_unicode() {
    let entries =
        bibliography::parse("@book{first, égaré, title={First},\n@book{second,title={Second}}");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].title, "First");
    assert_eq!(entries[1].title, "Second");
}

#[test]
fn bibtex_understands_nested_fields_strings_and_parentheses() {
    let source = r#"% @book{fake, title={Ignore}}
@string{journal = "A Journal"}
@article{knuth84,
 author = {Donald E. Knuth},
 title = {The {TeX}book, with {nested {braces}}},
 year = "1984", journal = journal # " supplement"
}
@book(lamport94, title="LaTeX: A Document Preparation System", author={Leslie Lamport}, year=1994)
@comment{ @misc{alsofake, title={no}} }
@preamble{"anything"}
@misc{unfinished, title={Still typing}
"#;
    let entries = bibliography::parse(source);
    assert_eq!(
        entries.iter().map(|e| e.key.as_str()).collect::<Vec<_>>(),
        ["knuth84", "lamport94", "unfinished"]
    );
    assert_eq!(entries[0].title, "The TeXbook, with nested braces");
    assert_eq!(entries[0].author, "Donald E. Knuth");
    assert_eq!(entries[1].year, "1994");
}

#[test]
fn resources_ignore_comments_and_accept_biblatex_options() {
    let text = "% \\bibliography{fake}\n\\addbibresource[location=local]{refs/main.bib}\n\\bibliography{old, other}\n\\verb|\\bibliography{alsofake}|";
    assert_eq!(
        bibliography::resources(text),
        ["refs/main.bib", "old.bib", "other.bib"]
    );
}
#[test]
fn citations_allow_macros_and_nested_braces_in_optional_notes() {
    let bib = bibliography::parse("@book{Knuth,title={Book}}");
    for source in ["\\citep[see \\S 2]{", "\\citep[see {\\S 2}][p. 4]{kn"] {
        let result = completion::suggest(source, source.len(), "", &bib).unwrap();
        assert_eq!(result.items[0].insert, "Knuth");
    }
    let project = "\\bibitem{Knuth} text \\label{Sec:First}";
    assert!(completion::suggest("\\cite{kn", 8, project, &[]).is_some());
    assert!(completion::suggest("\\ref{sec", 8, project, &[]).is_some());
}

#[test]
fn citation_completion_replaces_only_current_key_and_shows_metadata() {
    let bib =
        bibliography::parse("@book{knuth84,title={The TeXbook},author={Donald Knuth},year=1984}");
    let text = "α \\citep[see][p. 3]{other, knu}";
    let cursor = text.find("knu").unwrap() + 3;
    let result = completion::suggest(text, cursor, text, &bib).unwrap();
    assert_eq!(&text[result.range.clone()], "knu");
    assert_eq!(result.items[0].insert, "knuth84");
    assert!(result.items[0].detail.contains("The TeXbook"));
    let mut changed = text.to_string();
    changed.replace_range(result.range, &result.items[0].insert);
    assert_eq!(changed, "α \\citep[see][p. 3]{other, knuth84}");
    assert!(completion::suggest("\\cite{Knuth", 11, "", &bib).is_some());
}

#[test]
fn completions_are_contextual_and_use_project_labels() {
    let label = completion::suggest("\\ref{sec:", 9, "\\label{sec:chapter}", &[]).unwrap();
    assert_eq!(label.items[0].insert, "sec:chapter");
    let env = completion::suggest("\\begin{ali", 10, "", &[]).unwrap();
    assert!(env.items.iter().any(|i| i.insert == "align"));
    let command = completion::suggest("\\sect", 5, "", &[]).unwrap();
    assert!(command.items.iter().any(|i| i.insert == "\\section"));
    assert!(completion::suggest("ordinary prose", 14, "", &[]).is_none());
    assert!(completion::suggest("% \\cite{", 8, "", &[]).is_none());
    assert!(completion::suggest("\\begin{verbatim}\n\\cite{", 23, "", &[]).is_none());
}
#[test]
fn cached_citation_queries_keep_metadata_and_return_only_the_first_forty_keys() {
    let entries = (0..500)
        .rev()
        .map(|i| bibliography::Entry {
            key: format!("Key{i:03}"),
            title: "Cached title".into(),
            ..Default::default()
        })
        .collect::<Vec<_>>();
    let index = completion::Index::bibliography(&entries);
    let source = "\\cite{";
    let active = completion::Index::source(source);
    let result = completion::suggest_indexed(source, source.len(), &active, [&index]).unwrap();
    assert_eq!(result.items.len(), 40);
    assert_eq!(result.items[0].insert, "Key000");
    assert_eq!(result.items[39].insert, "Key039");
    assert!(result.items[0].detail.contains("Cached title"));
}

#[test]
#[ignore = "manual completion performance measurement"]
fn profile_cached_project_completion() {
    use std::{hint::black_box, time::Instant};
    let source = "\\cite{key09";
    let project = (0..25000)
        .map(|i| format!("\\label{{section:{i}}} Some source text.\n"))
        .collect::<String>();
    let bib = (0..10000)
        .map(|i| format!("@book{{key{i:05},title={{Title {i}}},author={{Author}},year=2020,}}\n"))
        .collect::<String>();
    let entries = bibliography::parse(&bib);
    let project_index = completion::Index::source(&project);
    let bibliography_index = completion::Index::bibliography(&entries);
    let active = completion::Index::source(source);
    let start = Instant::now();
    for _ in 0..20 {
        black_box(
            completion::suggest(source, source.len(), &project, &bibliography::parse(&bib))
                .unwrap(),
        );
    }
    let uncached = start.elapsed() / 20;
    let start = Instant::now();
    for _ in 0..200 {
        black_box(
            completion::suggest_indexed(
                source,
                source.len(),
                &active,
                [&project_index, &bibliography_index],
            )
            .unwrap(),
        );
    }
    eprintln!(
        "25,000 labels + 10,000 bibliography entries: rebuilding indexes {:?}/query; cached {:?}/query",
        uncached,
        start.elapsed() / 200
    );
}

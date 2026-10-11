use folio_core::navigation::{WordAnchor, source_word};

fn anchor(text: &str, word: &str) -> WordAnchor {
    let byte = text.rfind(word).unwrap();
    WordAnchor::at(text, text[..byte].chars().count()).unwrap()
}
fn selected(text: &str, range: std::ops::Range<usize>) -> String {
    text.chars().skip(range.start).take(range.len()).collect()
}

#[test]
fn pdf_click_selects_the_word_without_punctuation() {
    let text = "A café, then another word.";
    let word = anchor(text, "café");
    assert_eq!(selected(text, word.range), "café");
    assert!(WordAnchor::at(text, 6).is_none(), "comma is not a word");
    assert!(WordAnchor::at(text, 1).is_none(), "space is not a word");
    assert!(WordAnchor::at(text, text.chars().count()).is_none());
}

#[test]
fn context_finds_the_clicked_occurrence_inside_formatted_source() {
    let source = "\\section{Opening}\nFirst word stays; the \\textbf{second word} moves.\n";
    let word = anchor("First word stays; the second word moves.", "word");
    let range = source_word(source, 2, &word).unwrap();
    assert_eq!(selected(source, range.clone()), "word");
    assert_eq!(
        range.start,
        source[..source.rfind("word").unwrap()].chars().count()
    );
}

#[test]
fn unicode_offsets_ligatures_and_wrapped_words_keep_original_ranges() {
    let source = "Élodie writes café and efficient international work.\n";
    for (pdf, needle, expected) in [
        (
            "Élodie writes café and efficient international work.",
            "café",
            "café",
        ),
        (
            "Élodie writes café and eﬃcient international work.",
            "eﬃcient",
            "efficient",
        ),
        (
            "Élodie writes café and efficient inter-\r\nnational work.",
            "national",
            "international",
        ),
    ] {
        let word = anchor(pdf, needle);
        assert_eq!(
            selected(source, source_word(source, 1, &word).unwrap()),
            expected
        );
        if needle == "national" {
            assert_eq!(selected(pdf, word.range), "inter-\r\nnational");
        }
    }
}

#[test]
fn nearby_synctex_line_is_refined_but_comments_commands_and_keys_are_not_targets() {
    let source = "% selected target here\n\\label{target}\n\\target\nThe selected \\emph{target} lives here.\nFollowing line.\n";
    let word = anchor("The selected target lives here.", "target");
    let range = source_word(source, 5, &word).unwrap();
    assert_eq!(selected(source, range.clone()), "target");
    assert_eq!(
        range.start,
        source[..source.rfind("target").unwrap()].chars().count()
    );
    assert!(source_word("% target\n\\label{target}\n\\target", 2, &word).is_none());
}

#[test]
fn uncertain_or_distant_words_do_not_jump_to_an_unrelated_occurrence() {
    let word = anchor("target", "target");
    assert!(
        source_word("target target", 1, &word).is_none(),
        "ambiguous word"
    );
    assert!(source_word("The \\generatedmacro{} appears here.", 1, &word).is_none());
    let source = format!("target\n{}elsewhere", "\n".repeat(50));
    assert!(
        source_word(&source, 52, &word).is_none(),
        "stay near the SyncTeX anchor"
    );
}

#[test]
fn hidden_comment_environments_and_nested_citation_notes_are_not_word_origins() {
    let hint = anchor("selected target here", "target");
    let source = "\\begin{comment}\nselected target here\n\\end{comment}\n\\cite[see \\ref{x} selected target here]{key}\n";
    assert!(source_word(source, 2, &hint).is_none());
    let source = format!(
        "\\begin{{comment}}\n{}selected target here\n\\end{{comment}}",
        "\n".repeat(40)
    );
    assert!(
        source_word(&source, 42, &hint).is_none(),
        "respect blocks starting before the local window"
    );
}

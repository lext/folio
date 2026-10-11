use folio_core::{build::*, document::*, pdf::*, synctex::*, syntax::*};
#[test]
fn non_utf8_sources_fail_clearly_without_rewriting_bytes() {
    let dir = temp();
    let path = dir.join("latin1.tex");
    let bytes = b"caf\xe9";
    fs::write(&path, bytes).unwrap();
    let error = Document::open(&path).unwrap_err().to_string();
    assert!(
        error.contains("UTF-8") && error.contains("latin1.tex"),
        "{error}"
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);
    fs::remove_dir_all(dir).unwrap();
}
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
fn temp() -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "folio-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&p).unwrap();
    p
}
#[test]
fn document_save_is_atomic_and_detects_external_changes() {
    let dir = temp();
    let path = dir.join("chapter α.tex");
    fs::write(&path, "old").unwrap();
    let mut doc = Document::open(&path).unwrap();
    doc.text = "new λ".into();
    assert!(doc.is_dirty());
    fs::write(&path, "external").unwrap();
    assert!(doc.save().is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "external");
    fs::write(&path, "old").unwrap();
    doc.save().unwrap();
    assert!(!doc.is_dirty());
    assert_eq!(fs::read_to_string(&path).unwrap(), "new λ");
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn deleted_file_is_not_silently_recreated_on_save() {
    let dir = temp();
    let path = dir.join("main.tex");
    fs::write(&path, "old").unwrap();
    let mut doc = Document::open(&path).unwrap();
    doc.text.push('!');
    fs::remove_file(&path).unwrap();
    assert!(doc.save().is_err());
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn line_positions_are_unicode_and_crlf_safe() {
    let text = "αβ\r\nhello\n";
    assert_eq!(line_char_range(text, 1), 0..2);
    assert_eq!(line_char_range(text, 2), 4..9);
    assert_eq!(line_char_range(text, 300), 10..10);
    assert_eq!(char_position(text, 5), (2, 2));
}
#[test]
fn lexer_is_lossless_and_understands_escaped_comments_and_verbatim() {
    let text = "\\section{Élan}\\% literal % comment\n$α_2$ \\verb|%$|\n\\begin{verbatim}\n%$\\x\n\\end{verbatim}";
    let tokens = tokenize(text);
    let reconstructed: String = tokens.iter().map(|t| &text[t.range.clone()]).collect();
    assert_eq!(reconstructed, text);
    assert!(
        tokens
            .iter()
            .any(|t| t.kind == TokenKind::Comment && &text[t.range.clone()] == "% comment")
    );
    assert!(
        tokens
            .iter()
            .any(|t| t.kind == TokenKind::Verbatim && text[t.range.clone()].contains("%$"))
    );
    assert!(
        tokens
            .iter()
            .any(|t| t.kind == TokenKind::Math && text[t.range.clone()].contains("α_2"))
    );
}
#[test]
fn lexer_accepts_incomplete_and_arbitrary_package_commands() {
    for text in [
        "\\",
        "$unclosed",
        "\\newcommand{\\foo",
        "\\somePackageMacro*{é}",
        "\\verb|unclosed",
    ] {
        let ts = tokenize(text);
        assert_eq!(ts.iter().map(|t| t.range.len()).sum::<usize>(), text.len());
    }
    assert_eq!(tokenize("\\custom@macro")[0].kind, TokenKind::Command);
}
#[test]
fn structure_ignores_comments_and_verbatim_and_resolves_labels() {
    let text = "% \\section{Fake}\n\\section{Real {nested}}\n\\label{sec:real}\n\\begin{verbatim}\n\\section{No}\n\\end{verbatim}\n\\subsection*{Next}";
    let outline = outline(text);
    assert_eq!(outline.len(), 2);
    assert_eq!(outline[0].title, "Real {nested}");
    assert_eq!(outline[0].line, 2);
    assert!(completions(text, "sec:").iter().any(|c| c == "sec:real"));
}
#[test]
fn synctex_parses_multiple_hits_and_colons_in_paths() {
    let raw = "SyncTeX result begin\nOutput:/tmp/a.pdf\nPage:2\nx:40\ny:50\nh:39\nv:60\nW:120\nH:10\nPage:3\nx:41\ny:52\nSyncTeX result end";
    let hits = parse_view(raw).unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].page, 2);
    assert_eq!(
        hits[0].rect(),
        Rect {
            x: 39.,
            y: 50.,
            width: 120.,
            height: 10.
        }
    );
    let back = parse_edit(
        "SyncTeX result begin\nInput:/tmp/a:b/main.tex\nLine:27\nColumn:-1\nSyncTeX result end",
    )
    .unwrap();
    assert_eq!(back.path, PathBuf::from("/tmp/a:b/main.tex"));
    assert_eq!(back.line, 27);
    assert_eq!(back.column, 0);
    assert!(parse_view("SyncTeX result begin\nPage:0\nx:NaN\ny:2\nSyncTeX result end").is_err());
    assert!(parse_edit("Input:/tmp/a\nLine:0").is_err());
    assert!(parse_view("no result").is_err());
}
fn page() -> PageText {
    PageText {
        width: 200.,
        height: 300.,
        glyphs: "Aé λ"
            .chars()
            .enumerate()
            .map(|(i, ch)| Glyph {
                ch,
                rect: Rect {
                    x: i as f32 * 10.,
                    y: 20.,
                    width: 9.,
                    height: 12.,
                },
            })
            .collect(),
    }
}
#[test]
fn pdf_search_and_selection_use_characters_not_bytes() {
    let page = page();
    assert_eq!(page.find("é λ"), vec![1..4]);
    assert_eq!(page.selected_text(1..4), "é λ");
    assert_eq!(page.hit_test(11., 22.), Some(1));
    assert_eq!(page.hit_test(190., 290.), None);
    assert!(page.find("").is_empty());
}
#[test]
fn pdf_search_normalizes_wrapped_phrases_and_maps_back_to_glyphs() {
    let text = "α foo\r\n  BAR and inter-\r\nnational, well-being";
    let page = PageText {
        glyphs: text
            .chars()
            .map(|ch| Glyph {
                ch,
                rect: Rect::default(),
            })
            .collect(),
        ..Default::default()
    };
    let found = page.find("FOO bar");
    assert_eq!(found.len(), 1);
    assert_eq!(page.selected_text(found[0].clone()), "foo\r\n  BAR");
    let found = page.find("international");
    assert_eq!(found.len(), 1);
    assert_eq!(page.selected_text(found[0].clone()), "inter-\r\nnational");
    assert_eq!(page.find("well-being").len(), 1);
    assert!(page.find("wellbeing").is_empty());
    assert!(page.find("  \n ").is_empty());
}
#[test]
fn pdf_hit_testing_ignores_missing_or_nonfinite_glyph_geometry() {
    let mut page = page();
    page.glyphs.insert(
        0,
        Glyph {
            ch: '\n',
            rect: Rect::default(),
        },
    );
    page.glyphs.insert(
        0,
        Glyph {
            ch: 'x',
            rect: Rect {
                x: f32::NAN,
                ..Rect::default()
            },
        },
    );
    assert_eq!(page.hit_test(0., 0.), None);
    assert_eq!(page.nearest(0., 0.), Some(2));
    assert_eq!(page.nearest(f32::NAN, 0.), None);
}
#[test]
fn unterminated_math_recovers_for_later_outline_and_completions() {
    for source in [
        "$unfinished\n\\section{Visible}\n\\label{later}",
        "\\(unfinished\n\\section{Visible}\n\\label{later}",
    ] {
        assert_eq!(outline(source).len(), 1);
        assert!(completions(source, "later").iter().any(|s| s == "later"));
        assert_eq!(
            tokenize(source)
                .iter()
                .map(|t| &source[t.range.clone()])
                .collect::<String>(),
            source
        );
    }
    assert_eq!(
        tokenize("$a\n+b$")
            .iter()
            .filter(|t| t.kind == TokenKind::Math)
            .count(),
        1
    );
}
#[test]
fn nested_false_conditionals_skip_until_the_matching_fi() {
    let source = "\\iffalse\n\\iftrue ignored\\fi\n\\section{Hidden}\n% \\fi\n\\ifnum1=1 also ignored\\fi\n\\fi\n\\section{Visible}";
    assert_eq!(
        outline(source)
            .iter()
            .map(|e| e.title.as_str())
            .collect::<Vec<_>>(),
        ["Visible"]
    );
}
#[test]
fn diagnostics_ignore_timestamps_and_keep_windows_paths() {
    let result = parse_diagnostics(
        "12:30:45 typesetting\nC:\\work\\chapter.tex:9: Missing brace\n./main.tex:2: Undefined macro\nratio:2:1",
    );
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].path, PathBuf::from("C:\\work\\chapter.tex"));
    let custom = parse_diagnostics("./figures/plot.tikz:7: Missing brace\n2026.10:12:30 timestamp");
    assert_eq!(
        custom.len(),
        1,
        "custom TeX input extensions must still be navigable"
    );
    assert_eq!(custom[0].path, PathBuf::from("./figures/plot.tikz"));
}
#[test]
fn pdf_coordinates_roundtrip_at_zoom_and_hidpi() {
    let view = PageTransform {
        origin_x: 120.,
        origin_y: 30.,
        display_width: 400.,
        page_width: 200.,
        page_height: 300.,
    };
    assert_eq!(view.to_page(140., 70.), Some((10., 20.)));
    assert_eq!(view.to_screen(10., 20.), Some((140., 70.)));
    for width in [0., -1., f32::NAN, f32::INFINITY] {
        let invalid = PageTransform {
            page_width: width,
            ..view
        };
        assert!(invalid.to_screen(10., 20.).is_none());
        assert!(invalid.to_page(140., 70.).is_none());
    }
    assert_eq!(view.to_page(119., 30.), None);
    assert_eq!(view.to_page(140., 631.), None);
}
#[test]
fn navigation_without_a_map_reports_the_missing_synctex_capability() {
    let dir = temp();
    let pdf = dir.join("preview.pdf");
    fs::write(&pdf, "PDF without map").unwrap();
    let error = forward(&pdf, &dir.join("main.tex"), 1, 0)
        .unwrap_err()
        .to_string();
    assert!(error.contains("SyncTeX map"), "{error}");
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn texture_cache_evicts_oldest_and_accounts_replacements() {
    let mut cache = CacheBudget::new(100);
    assert!(cache.insert(1, 40).is_empty());
    cache.insert(2, 40);
    cache.touch(1);
    assert_eq!(cache.insert(3, 40), vec![2]);
    assert_eq!(cache.used(), 80);
    assert_eq!(cache.insert(1, 80), vec![3]);
    assert_eq!(cache.used(), 80);
    assert_eq!(cache.insert(4, 200), vec![1, 4]);
    assert_eq!(cache.used(), 0);
}
#[test]
fn build_arguments_preserve_paths_and_disable_shell_escape() {
    let config = BuildConfig::new(
        PathBuf::from("/tmp/project with spaces/main.tex"),
        Engine::LuaLatex,
    );
    let args = config.arguments();
    assert!(args.contains(&"-lualatex".to_string()));
    assert!(args.contains(&"-synctex=1".to_string()));
    assert!(args.contains(&"-latexoption=-no-shell-escape".to_string()));
    assert_eq!(args.last().unwrap(), "main.tex");
    assert!(
        args.iter()
            .any(|s| s.starts_with("-outdir=") && s.contains("project with spaces"))
    );
}
#[test]
fn pdf_path_preserves_dots_in_document_name() {
    let config = BuildConfig::new(PathBuf::from("/tmp/project/paper.v2.tex"), Engine::PdfLatex);
    assert_eq!(
        config.pdf_path(),
        PathBuf::from("/tmp/project/.folio/paper.v2.pdf")
    );
}
#[test]
fn diagnostics_parse_file_line_without_losing_colons() {
    let result = parse_diagnostics(
        "/tmp/a:b/chapter.tex:42: Undefined control sequence.\n./main.tex:9: Missing $ inserted.\nnoise",
    );
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].path, PathBuf::from("/tmp/a:b/chapter.tex"));
    assert_eq!(result[0].line, 42);
    assert!(result[1].message.contains("Missing"));
}
#[test]
fn project_root_follows_magic_comments_and_rejects_cycles() {
    let dir = temp();
    let root = dir.join("main.tex");
    let chapter = dir.join("chapter.tex");
    fs::write(&root, "\\documentclass{article}").unwrap();
    fs::write(&chapter, "% !TeX root = main.tex\nchapter").unwrap();
    assert_eq!(
        resolve_root(&chapter).unwrap(),
        root.canonicalize().unwrap()
    );
    fs::write(&root, "% !TeX root = chapter.tex").unwrap();
    assert!(resolve_root(&chapter).is_err());
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn outline_supports_short_titles_and_unicode_casefold_search() {
    assert_eq!(
        outline("\\section[Short]{A longer title}")[0].title,
        "A longer title"
    );
    let page = PageText {
        width: 100.,
        height: 100.,
        glyphs: vec![
            Glyph {
                ch: 'İ',
                rect: Rect::default(),
            },
            Glyph {
                ch: 'X',
                rect: Rect::default(),
            },
        ],
    };
    assert_eq!(page.find("i\u{307}x"), vec![0..2]);
}
#[test]
fn self_referencing_root_directive_is_a_valid_main_file() {
    let dir = temp();
    let path = dir.join("main.tex");
    fs::write(&path, "% !TeX root = main.tex\ntext").unwrap();
    assert_eq!(resolve_root(&path).unwrap(), path.canonicalize().unwrap());
    fs::remove_dir_all(dir).unwrap();
}

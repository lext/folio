use eframe::egui::{self, Color32, FontId, TextFormat, text::LayoutJob};
use folio_core::syntax;
use tree_sitter::{InputEdit, Parser, Point, Tree};
mod brackets;
mod highlight;
mod motion;
pub(crate) use highlight::Kind;
use highlight::Style;
pub(crate) use motion::Motion;

pub struct Language {
    kind: Kind,
    parser: Option<Parser>,
    tree: Option<Tree>,
    source: String,
    initialized: bool,
    brackets: brackets::Brackets,
    motions: motion::Index,
    pub errors: Vec<usize>,
    pub outline: Vec<syntax::OutlineEntry>,
    job: LayoutJob,
}
impl Default for Language {
    fn default() -> Self {
        Self::new(Kind::Latex)
    }
}
fn point(s: &str, offset: usize) -> Point {
    let prefix = &s[..offset];
    Point::new(
        prefix.bytes().filter(|b| *b == b'\n').count(),
        prefix.rfind('\n').map_or(offset, |p| offset - p - 1),
    )
}
impl Language {
    pub fn destination(&mut self, source: &str, caret: usize, motion: Motion) -> usize {
        self.update(source);
        self.motions.destination(caret, motion)
    }
    pub fn for_path(path: &std::path::Path) -> Self {
        Self::new(Kind::for_path(path))
    }
    fn new(kind: Kind) -> Self {
        let parser = (kind == Kind::Latex).then(|| {
            let mut parser = Parser::new();
            parser
                .set_language(&tree_sitter_latex::LANGUAGE.into())
                .expect("bundled LaTeX grammar");
            parser
        });
        Self {
            kind,
            parser,
            tree: None,
            source: String::new(),
            initialized: false,
            brackets: brackets::Brackets::default(),
            motions: motion::Index::default(),
            errors: vec![],
            outline: vec![],
            job: LayoutJob::default(),
        }
    }
    pub fn is_latex(&self) -> bool {
        self.kind == Kind::Latex
    }
    pub fn update(&mut self, source: &str) {
        if self.initialized && self.source == source {
            return;
        }
        // A minimal contiguous edit lets Tree-sitter reuse unaffected syntax subtrees.
        if let Some(tree) = &mut self.tree {
            let mut start = self
                .source
                .bytes()
                .zip(source.bytes())
                .take_while(|(a, b)| a == b)
                .count();
            while !self.source.is_char_boundary(start) || !source.is_char_boundary(start) {
                start -= 1;
            }
            let mut old_end = self.source.len();
            let mut new_end = source.len();
            while old_end > start
                && new_end > start
                && self.source.as_bytes()[old_end - 1] == source.as_bytes()[new_end - 1]
            {
                old_end -= 1;
                new_end -= 1;
            }
            while !self.source.is_char_boundary(old_end) || !source.is_char_boundary(new_end) {
                old_end += 1;
                new_end += 1;
            }
            tree.edit(&InputEdit {
                start_byte: start,
                old_end_byte: old_end,
                new_end_byte: new_end,
                start_position: point(&self.source, start),
                old_end_position: point(&self.source, old_end),
                new_end_position: point(source, new_end),
            });
        }
        self.tree = self
            .parser
            .as_mut()
            .and_then(|p| p.parse(source, self.tree.as_ref()));
        self.errors.clear();
        if let Some(tree) = &self.tree {
            let mut cursor = tree.walk();
            loop {
                let n = cursor.node();
                if n.is_error() || n.is_missing() {
                    self.errors.push(n.start_position().row + 1);
                }
                if cursor.goto_first_child() {
                    continue;
                }
                while !cursor.goto_next_sibling() {
                    if !cursor.goto_parent() {
                        break;
                    }
                }
                if cursor.node() == tree.root_node() {
                    break;
                }
            }
        }
        self.errors.sort_unstable();
        self.errors.dedup();
        self.outline = if self.is_latex() {
            syntax::outline(source)
        } else {
            vec![]
        };
        let tokens = highlight::tokenize(self.kind, source);
        self.brackets.update(self.kind, source, &tokens);
        self.motions.update(source, self.kind, &tokens);
        let mut job = LayoutJob::default();
        for token in tokens {
            let color = match token.style {
                Style::Text => crate::theme::TEXT,
                Style::Command | Style::Key => Color32::from_rgb(171, 189, 213),
                Style::Comment => Color32::from_rgb(127, 142, 144),
                Style::Math | Style::Number => Color32::from_rgb(211, 193, 158),
                Style::Delimiter => Color32::from_rgb(149, 159, 173),
                Style::Verbatim | Style::String => Color32::from_rgb(166, 192, 177),
                Style::Keyword => Color32::from_rgb(194, 177, 208),
            };
            job.append(
                &source[token.range],
                0.,
                TextFormat {
                    font_id: FontId::monospace(15.),
                    color,
                    ..Default::default()
                },
            );
        }
        self.job = job;
        self.source = source.into();
        self.initialized = true;
    }
    pub fn paint_brackets(&self, ui: &egui::Ui, output: &egui::text_edit::TextEditOutput) {
        let Some(cursor) = output.cursor_range.filter(|r| r.is_empty()) else {
            return;
        };
        if !output.response.has_focus() {
            return;
        }
        let Some(pair) = self.brackets.at(cursor.primary.index) else {
            return;
        };
        let color = if pair.partner.is_some() {
            crate::theme::ACCENT
        } else {
            Color32::from_rgb(218, 147, 135)
        };
        let painter = ui
            .painter()
            .with_clip_rect(ui.clip_rect().intersect(output.response.rect));
        for range in std::iter::once(pair.active).chain(pair.partner) {
            let from = output
                .galley
                .pos_from_cursor(egui::text::CCursor::new(range.start));
            let to = output
                .galley
                .pos_from_cursor(egui::text::CCursor::new(range.end));
            let rect = egui::Rect::from_min_max(from.left_top(), to.left_bottom())
                .translate(output.galley_pos.to_vec2());
            if painter.clip_rect().intersects(rect) {
                painter.rect_stroke(
                    rect,
                    2.,
                    egui::Stroke::new(1.0_f32, color),
                    egui::StrokeKind::Inside,
                );
            }
        }
    }
    pub fn layout(
        &mut self,
        ui: &egui::Ui,
        text: &str,
        _width: f32,
    ) -> std::sync::Arc<egui::Galley> {
        self.update(text);
        let mut job = self.job.clone();
        let font = egui::TextStyle::Monospace.resolve(ui.style());
        for section in &mut job.sections {
            section.format.font_id = font.clone();
            section.format.line_height = Some(crate::theme::line_height(font.size));
        }
        // Horizontal scrolling preserves one visual row per source line.
        job.wrap.max_width = f32::INFINITY;
        ui.fonts_mut(|f| f.layout_job(job))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prose_motion_index_tracks_unsaved_unicode_edits() {
        let mut language = Language::default();
        assert_eq!(
            language.destination("α one.\n\nβ two.", 1, Motion::ParagraphEnd),
            6
        );
        assert_eq!(
            language.destination("α much longer.\n\nβ two.", 1, Motion::ParagraphEnd),
            14
        );
        assert_eq!(language.destination("α!", 1, Motion::SentenceEnd), 2);
        assert_eq!(language.destination("", 999, Motion::SentenceEnd), 0);
    }
    #[test]
    fn configuration_highlighting_updates_without_latex_diagnostics_or_outline() {
        for (path, source, changed, key, value) in [
            (
                "folio.project.toml",
                "name = 'Folio'",
                "name = 'Edited' # \\section{not a heading}",
                "name",
                "'Edited'",
            ),
            (
                "data/paper.json",
                r#"{"name": "Folio"}"#,
                r#"{"name": "Edited", "tex": "\\\\section{not a heading}"}"#,
                "\"name\"",
                "\"Edited\"",
            ),
        ] {
            let mut lang = Language::for_path(std::path::Path::new(path));
            lang.update(source);
            lang.update(changed);
            let color = |text: &str| {
                let byte = changed.find(text).unwrap();
                lang.job
                    .sections
                    .iter()
                    .find(|s| s.byte_range.contains(&byte))
                    .unwrap()
                    .format
                    .color
            };
            assert_eq!(color(key), Color32::from_rgb(171, 189, 213));
            assert_eq!(color(value), Color32::from_rgb(166, 192, 177));
            assert_eq!(lang.job.text, changed);
            assert!(lang.errors.is_empty());
            assert!(lang.outline.is_empty());
            assert!(
                lang.tree.is_none(),
                "configuration files must not run through the LaTeX parser"
            );
            lang.update("");
            assert!(lang.job.text.is_empty());
        }
    }
    #[test]
    fn source_layout_keeps_gutter_aligned_with_logical_lines() {
        let ctx = egui::Context::default();
        let mut lang = Language::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let galley = lang.layout(
                    ui,
                    "\\section{A long title that must scroll horizontally}\nSecond line",
                    90.,
                );
                assert_eq!(galley.rows.len(), 2);
                assert!(galley.size().x > 90.);
            });
        });
    }
    #[test]
    fn grammar_parses_math_and_user_macros() {
        let mut lang = Language::default();
        lang.update("\\documentclass{article}\n\\begin{document}\n\\section{A} $x^2$ \\custom{value}\n\\end{document}");
        assert!(lang.tree.is_some());
        assert!(lang.errors.is_empty(), "{:?}", lang.errors);
        assert_eq!(lang.outline.len(), 1);
    }
    #[test]
    fn raw_environments_preserve_body_and_closing_delimiter() {
        for env in [
            "comment",
            "verbatim",
            "lstlisting",
            "minted",
            "asy",
            "asydef",
            "pycode",
            "luacode",
            "luacode*",
            "sagesilent",
            "sageblock",
        ] {
            let args = if env == "minted" { "{python}" } else { "" };
            let body = "\nα $ { \\unfinished \\end{wrong}\n";
            let source = format!("\\begin{{{env}}}{args}{body}\\end{{{env}}}\n\\section{{After}}");
            let mut lang = Language::default();
            lang.update(&source);
            let tree = lang.tree.as_ref().unwrap();
            assert!(
                !tree.root_node().has_error(),
                "{env}: {}",
                tree.root_node().to_sexp()
            );
            assert_eq!(lang.outline.last().unwrap().title, "After");
        }
    }
    #[test]
    fn false_block_does_not_end_at_command_prefix() {
        let mut lang = Language::default();
        lang.update("\\iffalse $ { \\final \\fi_suffix \\fi@macro \\fi\n\\section{After}");
        let tree = lang.tree.as_ref().unwrap();
        assert!(
            !tree.root_node().has_error(),
            "{}",
            tree.root_node().to_sexp()
        );
        assert_eq!(lang.outline.last().unwrap().title, "After");
    }
    #[test]
    fn incomplete_raw_input_recovers_after_edit() {
        for source in [
            "\\begin{verbatim}",
            "\\begin{verbatim}abc\\end{ver",
            "\\iffalse\\f",
        ] {
            let mut lang = Language::default();
            lang.update(source);
            lang.update("\\begin{verbatim}\\end{verbatim}\\section{Ready}");
            assert!(
                lang.errors.is_empty(),
                "{source}: {}",
                lang.tree.as_ref().unwrap().root_node().to_sexp()
            );
            assert_eq!(lang.outline.last().unwrap().title, "Ready");
        }
    }
    #[test]
    fn incremental_unicode_edits_match_fresh_parse() {
        let mut lang = Language::default();
        lang.update("\\section{α}");
        lang.update("\\section{βeta}\n$text$");
        let mut fresh = Language::default();
        fresh.update("\\section{βeta}\n$text$");
        assert_eq!(
            lang.tree.as_ref().unwrap().root_node().to_sexp(),
            fresh.tree.as_ref().unwrap().root_node().to_sexp()
        );
        lang.update("");
        assert!(lang.outline.is_empty());
    }
}

//! The section tabs inside the caption, after the project name.
use super::*;

/// The caption's height; the highlighted pill is inset within it.
const HEIGHT: f32 = 38.;
pub(super) struct Chosen(pub PathBuf);
impl App {
    /// Tabs in display order; empty outside a project.
    pub(super) fn tab_paths(&self) -> Vec<PathBuf> {
        let (Some(root), Some(_)) = (&self.root, &self.managed) else {
            return vec![];
        };
        let open: Vec<PathBuf> = self.tabs.iter().map(|t| t.doc.path.clone()).collect();
        // Prefer the buffer, so an \input typed a moment ago already orders.
        let text = self
            .tabs
            .iter()
            .find(|t| &t.doc.path == root)
            .map(|t| t.doc.text.as_str())
            .or_else(|| self.disk_sources.get(root).map(String::as_str))
            .unwrap_or_default();
        crate::workspace::tabs(root, text, &self.files, &open)
    }
    pub(super) fn active_tab(&self, tabs: &[PathBuf]) -> Option<usize> {
        let path = &self.tabs.get(self.active)?.doc.path;
        tabs.iter().position(|t| t == path)
    }
    fn tab_label(&self, path: &std::path::Path) -> String {
        let name = if path.extension().is_some_and(|e| e == "tex") {
            path.file_stem()
        } else {
            path.file_name()
        };
        let dirty = self
            .tabs
            .iter()
            .any(|t| t.doc.path == path && t.doc.is_dirty());
        format!(
            "{}{}",
            name.unwrap_or_default().to_string_lossy(),
            if dirty { " •" } else { "" }
        )
    }
    /// Draws the tabs inside `rect` of the caption, where the file path used
    /// to be: the project name leads into its files, and the current one is
    /// the highlighted tab.
    pub(super) fn tabs_ui(&self, ui: &mut egui::Ui, rect: egui::Rect) -> Option<Chosen> {
        let paths = self.tab_paths();
        if paths.is_empty() || rect.width() <= 0. {
            return None;
        }
        // The PDF is a view of the paper, so the file it was opened from
        // stays the current tab while it is shown.
        let active = self.active_tab(&paths);
        let font = FontId::proportional(theme::browser_font(self.settings.editor.font_size) - 2.);
        let mut chosen = None;
        let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        ui.set_clip_rect(rect.intersect(ui.clip_rect()));
        let ui = &mut ui;
        // Scroll the current tab into view only when it changes, so a strip
        // the writer scrolled by hand stays where they left it.
        let memory = ui.id().with("shown");
        let shown = active;
        let moved = ui.data(|d| d.get_temp(memory)) != Some(shown);
        ui.data_mut(|d| d.insert_temp(memory, shown));
        egui::ScrollArea::horizontal()
            .id_salt("workspace-tabs")
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            // Dragging here must still move the window, as on the rest of the caption.
            .scroll_source(egui::scroll_area::ScrollSource::MOUSE_WHEEL)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 2.;
                ui.horizontal(|ui| {
                    let tab = |ui: &mut egui::Ui, label: String, current: bool| {
                        let p = theme::of(ui);
                        let galley = ui.painter().layout_no_wrap(
                            label,
                            font.clone(),
                            if current { p.text } else { p.secondary },
                        );
                        let (rect, response) = ui.allocate_exact_size(
                            egui::vec2(galley.size().x + 20., HEIGHT),
                            egui::Sense::click(),
                        );
                        let pill = rect.shrink2(egui::vec2(0., 7.));
                        if current {
                            ui.painter().rect_filled(pill, 5., p.selected);
                            if moved {
                                response.scroll_to_me(None);
                            }
                        } else if response.hovered() {
                            ui.painter().rect_filled(pill, 5., p.hover);
                        }
                        ui.painter().galley(
                            egui::pos2(rect.left() + 10., rect.center().y - galley.size().y / 2.),
                            galley,
                            p.text,
                        );
                        response.clicked()
                    };
                    for (i, path) in paths.iter().enumerate() {
                        if tab(ui, self.tab_label(path), active == Some(i)) {
                            chosen = Some(Chosen(path.clone()));
                        }
                    }
                });
            });
        chosen
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use folio_core::project::Project;
    fn text_position(shape: &egui::epaint::Shape, needle: &str) -> Option<egui::Pos2> {
        match shape {
            egui::epaint::Shape::Text(s) if s.galley.job.text == needle => {
                Some(s.pos + s.galley.size() * 0.5)
            }
            egui::epaint::Shape::Vec(shapes) => {
                shapes.iter().find_map(|s| text_position(s, needle))
            }
            _ => None,
        }
    }
    #[test]
    fn sections_are_tabs_reached_by_click_or_by_the_surface_keys() {
        let dir = std::env::temp_dir().join(format!("folio-tabs-{}", std::process::id()));
        Project::create(&dir, "Paper", Engine::PdfLatex).unwrap();
        fs::write(dir.join("sections/methods.tex"), "\\section{Methods}\n").unwrap();
        fs::write(
            dir.join("main.tex"),
            "\\documentclass{article}\n\\begin{document}\n\\input{sections/methods}\n\\input{sections/introduction}\n\\end{document}\n",
        )
        .unwrap();
        let ctx = egui::Context::default();
        let mut app = App::with_context(ctx.clone(), Some(dir.clone()));
        super::super::tests::settle(&mut app, &ctx, |a| !a.scanning && !a.files.is_empty());
        let frame = |app: &mut App, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000., 700.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    app.handle_shortcuts(ctx);
                    app.workspace_ui(ctx);
                },
            )
        };
        let locate = |output: &egui::FullOutput, label: &str| {
            output
                .shapes
                .iter()
                .find_map(|s| text_position(&s.shape, label))
                .unwrap_or_else(|| panic!("{label} tab is missing"))
        };
        let click = |app: &mut App, pos| {
            for pressed in [true, false] {
                frame(
                    app,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
        };
        let name = |app: &App| {
            app.tabs[app.active]
                .doc
                .path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        };
        frame(&mut app, vec![]);
        let output = frame(&mut app, vec![]);
        // Every section is offered before it has been opened, in input order.
        assert_eq!(app.tabs.len(), 1);
        let xs = ["main", "methods", "introduction"].map(|l| locate(&output, l).x);
        assert!(xs.is_sorted(), "{xs:?}");
        // The tabs continue the caption's line; the path is not repeated.
        assert!((locate(&output, "Paper").y - locate(&output, "main").y).abs() < 2.);
        assert!(locate(&output, "Paper").x < locate(&output, "main").x);
        assert!(
            output
                .shapes
                .iter()
                .all(|s| text_position(&s.shape, "main.tex").is_none())
        );
        click(&mut app, locate(&output, "introduction"));
        assert_eq!(name(&app), "introduction.tex");
        assert_eq!(app.layers.layer, Layer::Source);
        let key = |app: &mut App, shift| {
            frame(
                app,
                vec![egui::Event::Key {
                    key: egui::Key::Tab,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers {
                        ctrl: true,
                        shift,
                        ..Default::default()
                    },
                }],
            );
        };
        key(&mut app, true);
        assert_eq!(name(&app), "methods.tex");
        key(&mut app, true);
        assert_eq!(name(&app), "main.tex");
        key(&mut app, false);
        key(&mut app, false);
        assert_eq!(name(&app), "introduction.tex");
        // The PDF is not a tab: stepping wraps past the last file, and from
        // the PDF a step returns to the files.
        key(&mut app, false);
        assert_eq!(name(&app), "main.tex");
        app.layers.layer = Layer::Preview;
        let output = frame(&mut app, vec![]);
        assert!(
            output
                .shapes
                .iter()
                .all(|s| text_position(&s.shape, "PDF").is_none())
        );
        key(&mut app, true);
        assert_eq!(name(&app), "introduction.tex");
        assert_eq!(app.layers.layer, Layer::Source);
        // Cmd/Ctrl+N jumps to the Nth tab; a position past the end is ignored.
        let digit = |app: &mut App, key| {
            frame(
                app,
                vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::COMMAND,
                }],
            );
        };
        digit(&mut app, egui::Key::Num2);
        assert_eq!(name(&app), "methods.tex");
        digit(&mut app, egui::Key::Num1);
        assert_eq!(name(&app), "main.tex");
        digit(&mut app, egui::Key::Num3);
        assert_eq!(name(&app), "introduction.tex");
        digit(&mut app, egui::Key::Num9);
        assert_eq!(name(&app), "introduction.tex");
        // A file opened from elsewhere joins the strip after the sections.
        app.open_document(dir.join("references.bib"));
        let output = frame(&mut app, vec![]);
        assert!(locate(&output, "references.bib").x > locate(&output, "introduction").x);
        drop(app);
        fs::remove_dir_all(dir).unwrap();
    }
    fn frame(
        app: &mut App,
        ctx: &egui::Context,
        width: f32,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 700.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                app.handle_shortcuts(ctx);
                app.workspace_ui(ctx);
            },
        )
    }
    fn texts(output: &egui::FullOutput) -> Vec<(String, egui::Rect)> {
        fn walk(
            shape: &egui::epaint::Shape,
            clip: egui::Rect,
            out: &mut Vec<(String, egui::Rect)>,
        ) {
            match shape {
                egui::epaint::Shape::Text(t) => out.push((
                    t.galley.job.text.clone(),
                    egui::Rect::from_min_size(t.pos, t.galley.size()).intersect(clip),
                )),
                egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, clip, out)),
                _ => {}
            }
        }
        let mut out = vec![];
        for clipped in &output.shapes {
            walk(&clipped.shape, clipped.clip_rect, &mut out);
        }
        out
    }
    #[test]
    fn many_sections_stay_inside_the_caption_and_the_current_tab_is_scrolled_into_view() {
        let dir = std::env::temp_dir().join(format!("folio-tabs-many-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        Project::create(&dir, "Paper", Engine::PdfLatex).unwrap();
        let mut main = String::from("\\documentclass{article}\n\\begin{document}\n");
        for i in 0..40 {
            fs::write(dir.join(format!("sections/part{i:02}.tex")), "text\n").unwrap();
            main.push_str(&format!("\\input{{sections/part{i:02}}}\n"));
        }
        main.push_str("\\end{document}\n");
        fs::write(dir.join("main.tex"), main).unwrap();
        let ctx = egui::Context::default();
        let mut app = App::with_context(ctx.clone(), Some(dir.clone()));
        super::super::tests::settle(&mut app, &ctx, |a| !a.scanning && a.files.len() > 40);
        // A width too small for the strip, and one with no room for it at all.
        for width in [1000., 420., 200.] {
            for _ in 0..3 {
                frame(&mut app, &ctx, width, vec![]);
            }
            let output = frame(&mut app, &ctx, width, vec![]);
            for (text, rect) in texts(&output) {
                if text.starts_with("part") && rect.is_positive() {
                    assert!(
                        rect.right() <= width && rect.top() < 38.,
                        "{text} leaves the caption at width {width}: {rect:?}"
                    );
                }
            }
        }
        // The last section is far off to the right; choosing it must show it.
        let last = dir.join("sections/part39.tex").canonicalize().unwrap();
        app.open_document(last.clone());
        for _ in 0..4 {
            frame(&mut app, &ctx, 1000., vec![]);
        }
        let output = frame(&mut app, &ctx, 1000., vec![]);
        let shown = texts(&output)
            .into_iter()
            .find(|(text, _)| text == "part39")
            .expect("the current tab is painted");
        assert!(
            shown.1.is_positive() && shown.1.width() > 20.,
            "{:?}",
            shown.1
        );
        // Stepping left from the first tab wraps to the last one: the
        // scaffold's introduction, which this main.tex does not input.
        app.open_document(dir.join("main.tex"));
        app.perform_action(&ctx, Action::PreviousSurface);
        assert!(
            app.tabs[app.active]
                .doc
                .path
                .ends_with("sections/introduction.tex")
        );
        app.perform_action(&ctx, Action::PreviousSurface);
        assert_eq!(app.tabs[app.active].doc.path, last);
        drop(app);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn unsaved_files_are_marked_and_tab_keys_do_nothing_without_a_project() {
        let dir = std::env::temp_dir().join(format!("folio-tabs-dirty-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        Project::create(&dir, "Paper", Engine::PdfLatex).unwrap();
        let ctx = egui::Context::default();
        let mut app = App::with_context(ctx.clone(), Some(dir.clone()));
        super::super::tests::settle(&mut app, &ctx, |a| !a.scanning && !a.files.is_empty());
        app.open_document(dir.join("sections/introduction.tex"));
        app.tabs[app.active].doc.text.push_str("more\n");
        frame(&mut app, &ctx, 1000., vec![]);
        let output = frame(&mut app, &ctx, 1000., vec![]);
        let labels: Vec<String> = texts(&output).into_iter().map(|(t, _)| t).collect();
        assert!(labels.iter().any(|t| t == "introduction •"), "{labels:?}");
        assert!(labels.iter().any(|t| t == "main"));
        // The PDF view keeps the file it came from as the current tab.
        app.layers.layer = Layer::Preview;
        let paths = app.tab_paths();
        assert_eq!(app.active_tab(&paths), Some(1));
        drop(app);
        // No project: no tabs, and every tab shortcut is inert.
        let mut app = App::with_context(ctx.clone(), None);
        assert!(app.tab_paths().is_empty());
        for action in Action::ALL {
            if action.tab().is_some()
                || matches!(action, Action::NextSurface | Action::PreviousSurface)
            {
                assert!(!app.action_available(action), "{}", action.name());
                app.perform_action(&ctx, action);
            }
        }
        assert!(app.tabs.is_empty());
        let key = |key, modifiers| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        frame(
            &mut app,
            &ctx,
            1000.,
            vec![
                key(egui::Key::Num1, egui::Modifiers::COMMAND),
                key(egui::Key::Tab, egui::Modifiers::CTRL),
            ],
        );
        assert!(app.tabs.is_empty());
        fs::remove_dir_all(dir).unwrap();
    }
}

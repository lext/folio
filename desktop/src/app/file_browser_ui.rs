use super::*;
use crate::file_browser::{Entry, Kind};

impl App {
    fn open_browser_entry(&mut self, ctx: &egui::Context, entry: &Entry) {
        match entry.kind {
            Kind::Directory => self.browser.toggle_folder(&entry.path),
            Kind::Text => self.open_document(entry.path.clone()),
            Kind::Asset => {
                let path = entry.path.clone();
                let tx = self.tx.clone();
                let ctx = ctx.clone();
                thread::spawn(move || {
                    #[cfg(target_os = "macos")]
                    let program = "open";
                    #[cfg(not(target_os = "macos"))]
                    let program = "xdg-open";
                    let result = std::process::Command::new(program)
                        .arg(&path)
                        .output()
                        .map_err(|e| format!("Cannot open {}: {e}", path.display()))
                        .and_then(|output| {
                            if output.status.success() {
                                Ok(())
                            } else {
                                Err(format!(
                                    "Cannot open {}: {}",
                                    path.display(),
                                    String::from_utf8_lossy(&output.stderr).trim()
                                ))
                            }
                        });
                    let _ = tx.send(Event::AssetOpened(result));
                    ctx.request_repaint();
                });
            }
        }
    }

    pub(super) fn file_browser_ui(&mut self, ctx: &egui::Context) -> bool {
        if !self.browser.open || self.root.is_none() {
            return false;
        }
        let mut chosen = None;
        let mut action = None;
        let panel = egui::SidePanel::left("project-files")
            .default_width(232.)
            .width_range(170. ..=420.)
            .resizable(true)
            .frame(
                egui::Frame::new()
                    .fill(theme::palette(&ctx.style().visuals).sidebar)
                    .inner_margin(12.),
            )
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 2.;
                let (rect, header) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), 32.),
                    egui::Sense::click(),
                );
                ui.painter().text(
                    rect.left_center() + egui::vec2(8., 0.),
                    egui::Align2::LEFT_CENTER,
                    "PROJECT",
                    FontId::proportional(10.),
                    theme::of(ui).muted,
                );
                ui.painter().text(
                    rect.right_center() - egui::vec2(8., 0.),
                    egui::Align2::RIGHT_CENTER,
                    theme::key_label(&self.settings.keybindings.hint(Action::ProjectFiles)),
                    FontId::proportional(10.),
                    theme::of(ui).muted,
                );
                header.context_menu(|ui| {
                    theme::menu(ui);
                    for command in [
                        Action::AddProjectFile,
                        Action::ImportFigure,
                        Action::ProjectFiles,
                    ] {
                        if ui
                            .add_enabled(
                                self.action_available(command),
                                self.settings.keybindings.button(command),
                            )
                            .clicked()
                        {
                            action = Some(command);
                            ui.close();
                        }
                    }
                });
                ui.add_space(8.);
                if self.browser.tree.entries.is_empty() {
                    ui.weak(if self.scanning {
                        "Reading project files…"
                    } else {
                        "No project files"
                    });
                }
                if self.browser.tree.incomplete {
                    ui.weak("Some files are unavailable or the scan limit was reached.");
                }
                let row_height = theme::ROW_HEIGHT;
                let mut scroll = egui::ScrollArea::vertical()
                    .id_salt("project-files-scroll")
                    .auto_shrink([false, false]);
                if !self.browser.visible.is_empty()
                    && let Some(path) = self.browser.scroll_to.take()
                    && let Some(row) = self
                        .browser
                        .visible
                        .iter()
                        .position(|&i| self.browser.tree.entries[i].path == path)
                {
                    // Scroll only when the active row is off screen (or clipped);
                    // re-centering on every file switch makes the list jump.
                    let shown = &self.browser.shown_rows;
                    if row <= shown.start || row + 1 >= shown.end {
                        scroll = scroll.vertical_scroll_offset(
                            (row as f32 * (row_height + ui.spacing().item_spacing.y) - 80.).max(0.),
                        );
                    }
                }
                scroll.show_rows(ui, row_height, self.browser.visible.len(), |ui, rows| {
                    self.browser.shown_rows = rows.clone();
                    for row in rows {
                        let entry = &self.browser.tree.entries[self.browser.visible[row]];
                        let active = self.layers.layer == Layer::Source
                            && self
                                .tabs
                                .get(self.active)
                                .is_some_and(|t| t.doc.path == entry.path);
                        let dirty = self
                            .tabs
                            .iter()
                            .any(|t| t.doc.path == entry.path && t.doc.is_dirty());
                        // Paint rows like every other Folio list: a quiet fill for
                        // hover/selection and palette text, never a framed button.
                        let p = theme::of(ui);
                        let (rect, response) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), row_height),
                            egui::Sense::click(),
                        );
                        response.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::WidgetType::SelectableLabel,
                                ui.is_enabled(),
                                active,
                                &entry.name,
                            )
                        });
                        let hovered = response.hovered() || response.has_focus();
                        if active || hovered {
                            ui.painter().rect_filled(
                                rect,
                                6.,
                                if active { p.selected } else { p.hover },
                            );
                        }
                        if active {
                            ui.painter().rect_filled(
                                egui::Rect::from_center_size(
                                    egui::pos2(rect.left() + 2., rect.center().y),
                                    egui::vec2(2., 14.),
                                ),
                                1.,
                                p.accent,
                            );
                        }
                        let indent = (entry.depth as f32 * 14.).min((rect.width() - 60.).max(0.));
                        // Draw icons directly: the bundled font lacks several
                        // triangle/diamond glyphs on both supported platforms.
                        let icon = egui::pos2(rect.left() + 10. + indent, rect.center().y);
                        let stroke = egui::Stroke::new(
                            1.0_f32,
                            if active || hovered {
                                p.secondary
                            } else {
                                p.muted
                            },
                        );
                        match entry.kind {
                            Kind::Directory => {
                                let points = if self.browser.expanded(&entry.path) {
                                    [egui::vec2(0., -2.), egui::vec2(3., 1.), egui::vec2(6., -2.)]
                                } else {
                                    [egui::vec2(1., -3.), egui::vec2(4., 0.), egui::vec2(1., 3.)]
                                };
                                ui.painter().add(egui::Shape::line(
                                    points.map(|p| icon + p).to_vec(),
                                    stroke,
                                ));
                            }
                            Kind::Asset => {
                                ui.painter().rect_stroke(
                                    egui::Rect::from_min_size(
                                        icon + egui::vec2(0., -4.),
                                        egui::vec2(6., 8.),
                                    ),
                                    1.,
                                    stroke,
                                    egui::StrokeKind::Inside,
                                );
                            }
                            Kind::Text => {}
                        }
                        let mut right = rect.right() - 10.;
                        if dirty {
                            let dot = ui.painter().text(
                                egui::pos2(right, rect.center().y),
                                egui::Align2::RIGHT_CENTER,
                                "•",
                                FontId::proportional(13.),
                                p.accent,
                            );
                            right = dot.left() - 6.;
                        }
                        let left = icon.x + 16.;
                        // Sources read as primary text; folders and files that
                        // open outside Folio stay one step quieter.
                        let color = if active || hovered || entry.kind == Kind::Text {
                            p.text
                        } else {
                            p.secondary
                        };
                        let mut job = egui::text::LayoutJob::simple_singleline(
                            entry.name.clone(),
                            FontId::proportional(13.),
                            color,
                        );
                        job.wrap =
                            egui::text::TextWrapping::truncate_at_width((right - left).max(0.));
                        let name = ui.fonts_mut(|f| f.layout_job(job));
                        ui.painter().galley(
                            egui::pos2(left, rect.center().y - name.size().y / 2.),
                            name,
                            color,
                        );
                        if response.clicked() {
                            chosen = Some(entry.clone());
                        }
                        response.context_menu(|ui| {
                            theme::menu(ui);
                            if entry.kind != Kind::Directory
                                && ui.button("Copy relative path").clicked()
                            {
                                ctx.copy_text(self.relative_path(&entry.path));
                                ui.close();
                            }
                            for command in [Action::AddProjectFile, Action::ImportFigure] {
                                if ui
                                    .add_enabled(
                                        self.action_available(command),
                                        self.settings.keybindings.button(command),
                                    )
                                    .clicked()
                                {
                                    action = Some(command);
                                    ui.close();
                                }
                            }
                        });
                        response.on_hover_text(format!(
                            "{}{}",
                            self.relative_path(&entry.path),
                            if entry.kind == Kind::Asset {
                                "\nOpen in system viewer"
                            } else {
                                ""
                            }
                        ));
                    }
                });
            });
        if let Some(entry) = chosen {
            self.open_browser_entry(ctx, &entry);
            // The rows above were painted before the click was applied; redo
            // the pass so selection, scroll and editor change in one frame.
            ctx.request_discard("Browser selection changed");
            ctx.request_repaint();
        }
        if let Some(action) = action {
            self.perform_action(ctx, action);
        }
        ctx.input(|i| {
            i.pointer
                .hover_pos()
                .is_some_and(|pos| panel.response.rect.contains(pos))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use folio_core::project::Project;

    #[test]
    fn browser_shortcut_and_single_click_open_a_source_ready_for_typing() {
        let dir = std::env::temp_dir().join(format!("folio-browser-click-{}", std::process::id()));
        let project = Project::create(&dir, "Paper", Engine::PdfLatex).unwrap();
        let ctx = egui::Context::default();
        let mut app = App::with_context(ctx.clone(), Some(dir.clone()));
        super::super::tests::settle(&mut app, &ctx, |a| !a.scanning);
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
        fn text_position(shape: &egui::epaint::Shape, needle: &str) -> Option<egui::Pos2> {
            match shape {
                egui::epaint::Shape::Text(s) if s.galley.job.text.trim() == needle => {
                    Some(s.pos + s.galley.size() * 0.5)
                }
                egui::epaint::Shape::Vec(shapes) => {
                    shapes.iter().find_map(|s| text_position(s, needle))
                }
                _ => None,
            }
        }
        let locate = |output: &egui::FullOutput, label| {
            output
                .shapes
                .iter()
                .find_map(|s| text_position(&s.shape, label))
                .unwrap_or_else(|| panic!("{label} is missing"))
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
        frame(&mut app, vec![]);
        frame(
            &mut app,
            vec![egui::Event::Key {
                key: egui::Key::B,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }],
        );
        assert!(app.browser.open);
        frame(&mut app, vec![]);
        let output = frame(&mut app, vec![]);
        click(&mut app, locate(&output, "introduction.tex"));
        assert_eq!(
            app.tabs[app.active].doc.path,
            project.directory.join("sections/introduction.tex")
        );
        assert_eq!(
            ctx.memory(|m| m.focused()),
            Some(egui::Id::new(("source", &app.tabs[app.active].doc.path))),
            "new source must own focus after the click"
        );
        frame(&mut app, vec![egui::Event::Text("immediate-typing".into())]);
        assert!(app.tabs[app.active].doc.text.contains("immediate-typing"));
        let output = frame(&mut app, vec![]);
        assert!(
            output
                .shapes
                .iter()
                .any(|s| text_position(&s.shape, "•").is_some()),
            "dirty buffer must be marked"
        );
        // Closing a folder hides its descendants and leaves the source alone.
        click(&mut app, locate(&output, "sections"));
        let output = frame(&mut app, vec![]);
        assert!(
            !output
                .shapes
                .iter()
                .any(|s| text_position(&s.shape, "introduction.tex").is_some())
        );
        assert!(app.tabs[app.active].doc.text.contains("immediate-typing"));
        frame(
            &mut app,
            vec![egui::Event::Key {
                key: egui::Key::B,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }],
        );
        assert!(!app.browser.open);
        drop(app);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn browser_rows_use_the_configured_palette_without_button_frames() {
        fn collect(shape: &egui::epaint::Shape, out: &mut Vec<egui::epaint::Shape>) {
            match shape {
                egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, out)),
                other => out.push(other.clone()),
            }
        }
        for (mode, p) in [("dark", &theme::DARK), ("light", &theme::LIGHT)] {
            let dir = std::env::temp_dir().join(format!(
                "folio-browser-palette-{mode}-{}",
                std::process::id()
            ));
            let project = Project::create(&dir, "Paper", Engine::PdfLatex).unwrap();
            let ctx = egui::Context::default();
            let settings = Settings::parse(&format!("[theme]\nmode='{mode}'")).unwrap();
            let mut app = App::with_settings(ctx.clone(), Some(dir.clone()), Arc::new(settings));
            super::super::tests::settle(&mut app, &ctx, |a| !a.scanning);
            app.open_document(project.directory.join("main.tex"));
            app.browser.open = true;
            let mut output = None;
            for _ in 0..3 {
                output = Some(ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1000., 700.),
                        )),
                        ..Default::default()
                    },
                    |ctx| app.workspace_ui(ctx),
                ));
            }
            let mut shapes = Vec::new();
            for clipped in &output.unwrap().shapes {
                collect(&clipped.shape, &mut shapes);
            }
            let text_color = |name: &str| {
                shapes
                    .iter()
                    .find_map(|s| match s {
                        egui::epaint::Shape::Text(t) if t.galley.job.text == name => {
                            Some(t.galley.job.sections[0].format.color)
                        }
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("{mode}: {name} is missing"))
            };
            assert_eq!(text_color("supplement.tex"), p.text, "{mode} source row");
            assert_eq!(text_color("sections"), p.secondary, "{mode} folder row");
            assert_eq!(text_color("PROJECT"), p.muted, "{mode} header");
            let fills: Vec<_> = shapes
                .iter()
                .filter_map(|s| match s {
                    egui::epaint::Shape::Rect(r) => Some((r.fill, r.stroke.color, r.stroke.width)),
                    _ => None,
                })
                .collect();
            assert!(
                fills.iter().any(|(fill, ..)| *fill == p.selected),
                "{mode}: the active row is filled with the selection color"
            );
            assert!(
                fills.iter().any(|(fill, ..)| *fill == p.sidebar),
                "{mode}: the panel uses the sidebar color"
            );
            assert!(
                !fills.iter().any(|(fill, stroke, width)| *fill == p.selected
                    && *width > 0.
                    && *stroke != egui::Color32::TRANSPARENT),
                "{mode}: rows are never outlined"
            );
            drop(app);
            fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn project_browser_shortcut_is_configurable_and_preserves_editor_focus() {
        let dir =
            std::env::temp_dir().join(format!("folio-browser-shortcut-{}", std::process::id()));
        Project::create(&dir, "Paper", Engine::PdfLatex).unwrap();
        let ctx = egui::Context::default();
        let settings = Settings::parse("[keybindings]\nproject_files=['F8']").unwrap();
        let mut app = App::with_settings(ctx.clone(), Some(dir.clone()), Arc::new(settings));
        let frame = |app: &mut App, events| {
            let _ = ctx.run(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ctx| {
                    app.handle_shortcuts(ctx);
                    app.workspace_ui(ctx);
                },
            );
        };
        let key = |key, modifiers| {
            vec![egui::Event::Key {
                key,
                modifiers,
                pressed: true,
                repeat: false,
                physical_key: None,
            }]
        };
        frame(&mut app, vec![]);
        frame(&mut app, key(egui::Key::B, egui::Modifiers::COMMAND));
        assert!(!app.browser.open, "overridden shortcut must stop working");
        frame(&mut app, key(egui::Key::F8, egui::Modifiers::NONE));
        assert!(app.browser.open);
        frame(&mut app, key(egui::Key::F8, egui::Modifiers::NONE));
        assert!(!app.browser.open);
        frame(
            &mut app,
            vec![egui::Event::Text("typing-after-browser".into())],
        );
        assert!(
            app.tabs[app.active]
                .doc
                .text
                .contains("typing-after-browser")
        );
        drop(app);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn browser_opens_existing_dirty_buffers_without_reloading_and_clears_on_switch() {
        let dir =
            std::env::temp_dir().join(format!("folio-browser-buffers-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let first = Project::create(&dir.join("first"), "First", Engine::PdfLatex).unwrap();
        let second = Project::create(&dir.join("second"), "Second", Engine::PdfLatex).unwrap();
        let ctx = egui::Context::default();
        let mut app = App::with_context(ctx.clone(), Some(first.directory.clone()));
        super::super::tests::settle(&mut app, &ctx, |a| !a.scanning);
        app.perform_action(&ctx, Action::ProjectFiles);
        let source = first.directory.join("sections/introduction.tex");
        let entry = app
            .browser
            .tree
            .entries
            .iter()
            .find(|e| e.path == source)
            .unwrap()
            .clone();
        app.open_browser_entry(&ctx, &entry);
        app.tabs[app.active]
            .doc
            .text
            .push_str("Unsaved browser edit");
        app.tabs[app.active].edited();
        app.open_document(first.directory.join("main.tex"));
        app.open_browser_entry(&ctx, &entry);
        assert_eq!(app.tabs.len(), 2);
        assert!(
            app.tabs[app.active]
                .doc
                .text
                .ends_with("Unsaved browser edit")
        );
        assert!(app.tabs[app.active].doc.is_dirty());
        assert_eq!(app.layers.layer, Layer::Source);
        assert!(app.focus_source);
        app.open_with_choice(second.directory.clone(), true);
        assert!(
            app.browser.tree.entries.is_empty(),
            "old project's files must disappear immediately"
        );
        super::super::tests::settle(&mut app, &ctx, |a| !a.scanning);
        assert!(
            app.browser
                .tree
                .entries
                .iter()
                .all(|e| e.path.starts_with(&second.directory))
        );
        drop(app);
        fs::remove_dir_all(dir).unwrap();
    }
}

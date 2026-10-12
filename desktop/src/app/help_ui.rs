//! The shortcut sheet: every bound command on one screen, grouped by purpose.
use super::*;

const GROUPS: [(&str, &[Action]); 4] = [
    (
        "Files and tabs",
        &[
            Action::Switcher,
            Action::Tab1,
            Action::NextSurface,
            Action::PreviousSurface,
            Action::ProjectFiles,
            Action::Save,
            Action::Reload,
            Action::AddProjectFile,
            Action::ImportFigure,
            Action::NewProject,
            Action::OpenProject,
            Action::ProjectSettings,
            Action::ValidateProject,
            Action::ExportProject,
        ],
    ),
    (
        "Writing",
        &[
            Action::Find,
            Action::Complete,
            Action::Outline,
            Action::ParagraphDown,
            Action::ParagraphUp,
            Action::SentenceNext,
            Action::SentencePrevious,
            Action::Git,
        ],
    ),
    (
        "PDF",
        &[
            Action::Preview,
            Action::Source,
            Action::Recompile,
            Action::Go,
            Action::PdfFit,
            Action::PdfFitPage,
            Action::PdfNext,
            Action::PdfPrevious,
            Action::PdfZoomIn,
            Action::PdfZoomOut,
            Action::ShowLog,
            Action::CancelBuild,
        ],
    ),
    (
        "Folio",
        &[
            Action::Commands,
            Action::Help,
            Action::UiZoomIn,
            Action::UiZoomOut,
            Action::UiZoomReset,
            Action::Dismiss,
        ],
    ),
];
impl App {
    /// Bound commands as (keys, label) rows under their group heading. A
    /// command without a shortcut is reachable from the palette and omitted.
    pub(super) fn help_rows(&self) -> Vec<(&'static str, Vec<(String, String)>)> {
        let mut groups: Vec<(&'static str, Vec<(String, String)>)> = GROUPS
            .iter()
            .map(|(title, actions)| {
                let mut rows = vec![];
                for action in actions.iter().copied() {
                    let keys = self.settings.keybindings.hint(action);
                    if keys.is_empty() {
                        continue;
                    }
                    let (keys, label) = if action.tab().is_some() {
                        // One row stands for the nine tab positions.
                        let last = self.settings.keybindings.hint(Action::Tab9);
                        (format!("{keys} … {last}"), "Go to tab 1–9".to_owned())
                    } else {
                        (keys, action.label().trim_end_matches('…').to_owned())
                    };
                    rows.push((theme::key_label(&keys), label));
                }
                (*title, rows)
            })
            .collect();
        if let Some(gesture) = &self.settings.mouse.go {
            groups[2].1.push((
                theme::key_label(&gesture.label),
                "Go to selection in the other view".into(),
            ));
        }
        // A command bound in folio.toml but absent above must still be listed.
        let other: Vec<(String, String)> = Action::ALL
            .into_iter()
            .filter(|a| {
                a.is_command()
                    && a.tab().is_none()
                    && !GROUPS.iter().any(|(_, actions)| actions.contains(a))
            })
            .filter_map(|a| {
                let keys = self.settings.keybindings.hint(a);
                (!keys.is_empty()).then(|| (theme::key_label(&keys), a.label().to_owned()))
            })
            .collect();
        if !other.is_empty() {
            groups.push(("Other", other));
        }
        groups.retain(|(_, rows)| !rows.is_empty());
        groups
    }
    pub(super) fn help_ui(&mut self, ctx: &egui::Context) {
        if !self.show_help {
            return;
        }
        let groups = self.help_rows();
        let screen = ctx.content_rect();
        let width = (screen.width() - 80.).clamp(420., 860.);
        egui::Window::new("Shortcuts")
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .min_width(width)
            .max_width(width)
            .max_height(screen.height() - 60.)
            .frame(theme::popup(&ctx.style().visuals).inner_margin(22.))
            .show(ctx, |ui| {
                let p = theme::of(ui);
                let close = [Action::Help, Action::Dismiss]
                    .map(|a| theme::key_label(&self.settings.keybindings.hint(a)))
                    .into_iter()
                    .filter(|keys| !keys.is_empty())
                    .collect::<Vec<_>>()
                    .join(" or ");
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Shortcuts").size(15.).strong().color(p.text));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(theme::caption(ui, format!("{close}   Close")));
                    });
                });
                ui.add_space(10.);
                // Without a floor a window that has no height of its own gives
                // the list a sliver; scroll only when the screen is too short.
                let tallest = (screen.height() - 190.).max(160.);
                egui::ScrollArea::vertical()
                    .max_height(tallest)
                    .min_scrolled_height(tallest)
                    .show(ui, |ui| {
                        // Balance the groups over two columns by row count.
                        let total: usize = groups.iter().map(|(_, rows)| rows.len() + 2).sum();
                        let mut split = groups.len();
                        let mut seen = 0;
                        for (i, (_, rows)) in groups.iter().enumerate() {
                            if seen * 2 >= total {
                                split = i;
                                break;
                            }
                            seen += rows.len() + 2;
                        }
                        ui.columns(2, |columns| {
                            for (i, (title, rows)) in groups.iter().enumerate() {
                                let ui = &mut columns[usize::from(i >= split)];
                                ui.label(theme::caption(ui, title.to_uppercase()));
                                ui.add_space(2.);
                                egui::Grid::new(("help", i))
                                    .num_columns(2)
                                    .spacing([18., 5.])
                                    .min_col_width(118.)
                                    .show(ui, |ui| {
                                        for (keys, label) in rows {
                                            ui.label(
                                                RichText::new(keys)
                                                    .monospace()
                                                    .size(12.5)
                                                    .color(p.accent),
                                            );
                                            ui.label(RichText::new(label).size(13.).color(p.text));
                                            ui.end_row();
                                        }
                                    });
                                ui.add_space(14.);
                            }
                        });
                    });
                ui.separator();
                ui.label(theme::caption(
                    ui,
                    match &self.config_path {
                        Some(path) => format!(
                            "Settings: {} · edit and restart to apply",
                            path.display()
                        ),
                        None => "Bundled defaults · copy folio.toml to ~/.config/folio/ to change them, then restart".into(),
                    },
                ));
                ui.label(theme::caption(
                    ui,
                    "Commands without a shortcut are in the command palette.",
                ));
            });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;
    #[test]
    fn the_sheet_lists_every_bound_command_once_and_follows_the_settings() {
        let ctx = egui::Context::default();
        let app = App::with_context(ctx.clone(), None);
        let groups = app.help_rows();
        let labels: Vec<&str> = groups
            .iter()
            .flat_map(|(_, rows)| rows.iter().map(|(_, label)| label.as_str()))
            .collect();
        for action in Action::ALL {
            if !action.is_command() || action.tab().is_some() {
                continue;
            }
            let bound = !app.settings.keybindings.hint(action).is_empty();
            let label = action.label().trim_end_matches('…');
            assert_eq!(
                labels.iter().filter(|l| **l == label).count(),
                usize::from(bound),
                "{label}"
            );
        }
        assert_eq!(labels.iter().filter(|l| **l == "Go to tab 1–9").count(), 1);
        assert!(groups.iter().all(|(title, _)| *title != "Other"));
        // A shortcut removed in folio.toml leaves the sheet; a new one joins it.
        let settings =
            Settings::parse("[keybindings]\nsave=[]\nexport_project=['Primary+E']").unwrap();
        let app = App::with_settings(ctx, None, Arc::new(settings));
        let rows: Vec<(String, String)> = app
            .help_rows()
            .into_iter()
            .flat_map(|(_, rows)| rows)
            .collect();
        assert!(rows.iter().all(|(_, label)| label != "Save"));
        assert!(rows.iter().any(|(_, label)| label == "Export flat project"));
    }
    #[test]
    fn the_help_key_opens_the_sheet_and_closes_it_again() {
        let ctx = egui::Context::default();
        let mut app = App::with_context(ctx.clone(), None);
        let press = |app: &mut App, key, modifiers| {
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
                        app.help_ui(ctx);
                    },
                )
            };
            frame(
                app,
                vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                }],
            );
            // A window measures itself for one frame before it paints.
            let output = frame(app, vec![]);
            fn texts(shape: &egui::Shape, out: &mut Vec<String>) {
                match shape {
                    egui::Shape::Text(t) => out.push(t.galley.job.text.clone()),
                    egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| texts(s, out)),
                    _ => {}
                }
            }
            let mut out = vec![];
            output.shapes.iter().for_each(|s| texts(&s.shape, &mut out));
            out
        };
        let shown = press(&mut app, egui::Key::H, egui::Modifiers::COMMAND);
        assert!(app.show_help);
        assert!(shown.iter().any(|t| t == "FILES AND TABS"), "{shown:?}");
        assert!(shown.iter().any(|t| t == "Shortcuts"));
        press(&mut app, egui::Key::H, egui::Modifiers::COMMAND);
        assert!(!app.show_help);
        press(&mut app, egui::Key::H, egui::Modifiers::COMMAND);
        press(&mut app, egui::Key::Escape, egui::Modifiers::NONE);
        assert!(!app.show_help);
    }
    #[test]
    fn every_group_is_visible_without_scrolling_on_an_ordinary_screen() {
        let ctx = egui::Context::default();
        let mut app = App::with_context(ctx.clone(), None);
        app.show_help = true;
        let mut output = None;
        // A window measures itself before it settles on a size.
        for _ in 0..3 {
            output = Some(ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400., 900.),
                    )),
                    ..Default::default()
                },
                |ctx| app.help_ui(ctx),
            ));
        }
        fn texts(shape: &egui::Shape, out: &mut Vec<(String, egui::Rect)>) {
            match shape {
                egui::Shape::Text(t) => {
                    out.push((t.galley.job.text.clone(), t.visual_bounding_rect()))
                }
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| texts(s, out)),
                _ => {}
            }
        }
        let mut visible = vec![];
        for clipped in &output.unwrap().shapes {
            let mut found = vec![];
            texts(&clipped.shape, &mut found);
            visible.extend(
                found
                    .into_iter()
                    .filter(|(_, rect)| clipped.clip_rect.contains_rect(*rect))
                    .map(|(text, _)| text),
            );
        }
        for wanted in [
            "FILES AND TABS",
            "WRITING",
            "PDF",
            "FOLIO",
            "Save",
            "Keyboard shortcuts",
            "Dismiss panel or search",
        ] {
            assert!(visible.iter().any(|t| t == wanted), "{wanted}: {visible:?}");
        }
    }
    #[test]
    fn help_stays_reachable_from_the_palette_when_its_key_is_unbound() {
        let ctx = egui::Context::default();
        let settings = Settings::parse("[keybindings]\nhelp=[]").unwrap();
        let mut app = App::with_settings(ctx.clone(), None, Arc::new(settings));
        assert!(app.action_available(Action::Help));
        app.perform_action(&ctx, Action::Help);
        assert!(app.show_help);
        let rows: Vec<(String, String)> = app
            .help_rows()
            .into_iter()
            .flat_map(|(_, rows)| rows)
            .collect();
        assert!(
            rows.iter()
                .all(|(_, label)| label != "Keyboard shortcuts" && label != Action::Help.label())
        );
        // The sheet still paints, and names the remaining way to close it.
        let mut labels = vec![];
        for _ in 0..2 {
            labels.clear();
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(640., 480.),
                    )),
                    ..Default::default()
                },
                |ctx| app.help_ui(ctx),
            );
            fn walk(shape: &egui::Shape, out: &mut Vec<String>) {
                match shape {
                    egui::Shape::Text(t) => out.push(t.galley.job.text.clone()),
                    egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, out)),
                    _ => {}
                }
            }
            output
                .shapes
                .iter()
                .for_each(|s| walk(&s.shape, &mut labels));
        }
        assert!(
            labels
                .iter()
                .any(|t| t.contains("Close") && t.contains("esc")),
            "{labels:?}"
        );
        assert!(labels.iter().any(|t| t.contains("Bundled defaults")));
    }
}

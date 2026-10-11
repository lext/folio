use crate::{
    settings::{Action, Keybindings},
    theme,
};
use eframe::egui;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Surfaces,
    Commands,
    Outline,
}
impl Mode {
    fn prompt(self) -> &'static str {
        match self {
            Self::Surfaces => "Switch to a file or PDF…",
            Self::Commands => "Run a command…",
            Self::Outline => "Go to section…",
        }
    }
}
#[derive(Clone, Debug)]
pub enum Target {
    File(PathBuf),
    Preview,
    Action(Action),
    Line(usize),
}
pub struct Entry {
    pub title: String,
    pub detail: String,
    pub target: Target,
}
pub struct Launcher {
    pub mode: Mode,
    pub query: String,
    pub selected: usize,
    entries: Vec<Entry>,
    focus: bool,
    filtered_query: Option<String>,
    matches: Vec<usize>,
    shown_rows: std::ops::Range<usize>,
}
pub enum Outcome {
    Keep,
    Dismiss,
    Open(Target),
}
impl Launcher {
    pub fn new(mode: Mode, entries: Vec<Entry>) -> Self {
        Self {
            mode,
            entries,
            query: String::new(),
            selected: 0,
            focus: true,
            filtered_query: None,
            matches: Vec::new(),
            shown_rows: 0..0,
        }
    }
    fn refilter(&mut self) {
        if self.filtered_query.as_deref() == Some(self.query.as_str()) {
            return;
        }
        self.matches = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                crate::workspace::matches(&self.query, &e.title, &e.detail).then_some(i)
            })
            .collect();
        self.filtered_query = Some(self.query.clone());
        self.selected = 0;
    }
    pub fn ui(&mut self, ctx: &egui::Context, keys: &Keybindings) -> Outcome {
        if keys.consume(ctx, Action::LauncherDismiss) {
            return Outcome::Dismiss;
        }
        let next = keys.consume(ctx, Action::LauncherNext);
        let previous = keys.consume(ctx, Action::LauncherPrevious);
        let accept = keys.consume(ctx, Action::LauncherAccept);
        let mut outcome = Outcome::Keep;
        let id = egui::Id::new("workspace-launcher");
        let modal = egui::Modal::new(id)
            .area(egui::Modal::default_area(id).anchor(
                egui::Align2::CENTER_TOP,
                egui::vec2(0., ctx.content_rect().height() * 0.16),
            ))
            .frame(theme::popup().inner_margin(10.))
            .backdrop_color(egui::Color32::from_black_alpha(100))
            .show(ctx, |ui| {
                ui.set_width(600_f32.min(ctx.content_rect().width() - 48.).max(240.));
                ui.spacing_mut().item_spacing = egui::vec2(8., 2.);
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(theme::caption(match self.mode {
                                Mode::Surfaces => "SURFACES",
                                Mode::Commands => "COMMANDS",
                                Mode::Outline => "OUTLINE",
                            }));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(theme::caption(theme::key_label(
                                        &keys.hint(Action::LauncherDismiss),
                                    )));
                                },
                            );
                        });
                    });
                let input = egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.query)
                                .id(egui::Id::new("launcher-query"))
                                .return_key(None)
                                .font(egui::FontId::proportional(17.))
                                .frame(false)
                                .margin(egui::Vec2::ZERO)
                                .hint_text(self.mode.prompt())
                                .desired_width(f32::INFINITY),
                        )
                    })
                    .inner;
                if self.focus {
                    input.request_focus();
                    self.focus = false;
                }
                self.refilter();
                self.selected = self.selected.min(self.matches.len().saturating_sub(1));
                if !self.matches.is_empty() {
                    if next {
                        self.selected = (self.selected + 1) % self.matches.len();
                    }
                    if previous {
                        self.selected =
                            (self.selected + self.matches.len() - 1) % self.matches.len();
                    }
                    if accept {
                        outcome =
                            Outcome::Open(self.entries[self.matches[self.selected]].target.clone());
                    }
                }
                ui.add_space(8.);
                ui.separator();
                ui.add_space(6.);
                let visible_rows = (ctx.content_rect().height() * 0.42 / 46.)
                    .floor()
                    .clamp(3., 6.);
                let max_height = visible_rows * 46. - 2.;
                let mut scroll = egui::ScrollArea::vertical()
                    .id_salt("launcher-results")
                    .max_height(max_height);
                if (next || previous || input.changed())
                    && !self.shown_rows.contains(&self.selected)
                {
                    scroll = scroll.vertical_scroll_offset(
                        (self.selected as f32 * 46. - max_height / 2.).max(0.),
                    );
                }
                if self.matches.is_empty() {
                    ui.add_space(12.);
                    ui.label(egui::RichText::new("No matching results").color(theme::SECONDARY));
                    ui.add_space(12.);
                } else {
                    scroll.show_rows(ui, 44., self.matches.len(), |ui, rows| {
                        self.shown_rows = rows.clone();
                        for row in rows {
                            let entry = &self.entries[self.matches[row]];
                            let (detail, shortcut) = if matches!(entry.target, Target::Action(_)) {
                                ("", entry.detail.as_str())
                            } else {
                                (entry.detail.as_str(), "")
                            };
                            let response = theme::row(
                                ui,
                                &entry.title,
                                detail,
                                shortcut,
                                self.selected == row,
                            );
                            if response.clicked() {
                                outcome = Outcome::Open(entry.target.clone());
                            }
                        }
                    });
                }
                ui.add_space(6.);
                ui.separator();
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(theme::caption(format!(
                                "{}  navigate",
                                theme::key_label(&keys.hint(Action::LauncherNext))
                            )));
                            ui.add_space(10.);
                            ui.label(theme::caption(format!(
                                "{}  open",
                                theme::key_label(&keys.hint(Action::LauncherAccept))
                            )));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(theme::caption(format!(
                                        "{} results",
                                        self.matches.len()
                                    )));
                                },
                            );
                        });
                    });
            });
        if modal.backdrop_response.clicked() {
            Outcome::Dismiss
        } else {
            outcome
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn large_palettes_bound_paint_work_and_reveal_keyboard_selection() {
        let ctx = egui::Context::default();
        let keys = crate::settings::Settings::default().keybindings;
        let mut launcher = Launcher::new(
            Mode::Surfaces,
            (0..4000)
                .map(|i| Entry {
                    title: format!("chapter-{i:04}.tex"),
                    detail: format!("sections/chapter-{i:04}.tex"),
                    target: Target::File(PathBuf::from(format!("chapter-{i:04}.tex"))),
                })
                .collect(),
        );
        let frame = |launcher: &mut Launcher, events| {
            let mut outcome = Outcome::Keep;
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000., 700.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| outcome = launcher.ui(ctx, &keys),
            );
            (output, outcome)
        };
        let key = |key| {
            vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]
        };
        frame(&mut launcher, vec![]);
        let (output, _) = frame(&mut launcher, vec![]);
        let count = output.shapes.iter().filter(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.job.text.starts_with("chapter-"))).count();
        assert!(
            (1..=10).contains(&count),
            "painted {count} results for a 4000-file project"
        );
        frame(&mut launcher, key(egui::Key::ArrowUp));
        frame(&mut launcher, vec![]);
        assert!(
            launcher.shown_rows.contains(&3999),
            "wrapped keyboard selection must scroll into view"
        );
        let (_, outcome) = frame(&mut launcher, key(egui::Key::Enter));
        assert!(
            matches!(outcome, Outcome::Open(Target::File(p)) if p == std::path::Path::new("chapter-3999.tex"))
        );
        frame(
            &mut launcher,
            vec![egui::Event::Text("chapter-0001".into())],
        );
        let (_, outcome) = frame(&mut launcher, key(egui::Key::Enter));
        assert!(
            matches!(outcome, Outcome::Open(Target::File(p)) if p == std::path::Path::new("chapter-0001.tex")),
            "filtering must reset the former offscreen selection"
        );
    }

    #[test]
    fn a_result_can_be_selected_from_the_right_side_of_its_row() {
        let ctx = egui::Context::default();
        let keys = crate::settings::Settings::default().keybindings;
        let mut launcher = Launcher::new(
            Mode::Surfaces,
            vec![Entry {
                title: "chapter.tex".into(),
                detail: "sections/chapter.tex".into(),
                target: Target::File(PathBuf::from("sections/chapter.tex")),
            }],
        );
        let frame = |launcher: &mut Launcher, events| {
            let mut result = Outcome::Keep;
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000., 700.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| result = launcher.ui(ctx, &keys),
            );
            (output, result)
        };
        fn locate(shape: &egui::Shape) -> Option<egui::Pos2> {
            match shape {
                egui::Shape::Text(s) if s.galley.job.text == "chapter.tex" => Some(s.pos),
                egui::Shape::Vec(shapes) => shapes.iter().find_map(locate),
                _ => None,
            }
        }
        frame(&mut launcher, vec![]);
        let (output, _) = frame(&mut launcher, vec![]);
        let pos =
            output.shapes.iter().find_map(|s| locate(&s.shape)).unwrap() + egui::vec2(410., 6.);
        let mut opened = false;
        for pressed in [true, false] {
            let (_, outcome) = frame(
                &mut launcher,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        pressed,
                        button: egui::PointerButton::Primary,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
            opened |= matches!(outcome, Outcome::Open(Target::File(p)) if p == std::path::Path::new("sections/chapter.tex"));
        }
        assert!(
            opened,
            "the entire row must open its result, including the space after its label"
        );
    }
}

use super::*;
use crate::{
    launcher::{Entry, Launcher, Mode, Outcome, Target},
    workspace::Surface,
};

impl App {
    pub(super) fn action_available(&self, action: Action) -> bool {
        match action {
            action if action.motion().is_some() => {
                self.tabs.get(self.active).is_some() && self.layers.layer == Layer::Source
            }
            Action::ProjectFiles => self.root.is_some(),
            Action::Preview | Action::Recompile | Action::Go => {
                self.root.is_some() && !self.exporting
            }
            Action::NewProject | Action::OpenProject => {
                !self.building && !self.exporting && !self.project_busy
            }
            Action::ExportProject => {
                self.managed.is_some() && !self.building && !self.exporting && !self.project_busy
            }
            Action::ProjectSettings
            | Action::ValidateProject
            | Action::AddProjectFile
            | Action::ImportFigure => {
                self.managed.is_some() && !self.project_busy && !self.exporting
            }
            Action::Source | Action::NextSurface | Action::PreviousSurface | Action::Save => {
                !self.tabs.is_empty()
            }
            Action::Outline | Action::Complete | Action::Git | Action::Reload => {
                !self.tabs.is_empty() && self.layers.layer == Layer::Source
            }
            Action::CancelBuild => self.building || self.exporting,
            Action::PdfZoomIn
            | Action::PdfZoomOut
            | Action::PdfFit
            | Action::PdfFitPage
            | Action::PdfNext
            | Action::PdfPrevious => self.layers.layer == Layer::Preview,
            _ => true,
        }
    }
    pub(super) fn perform_action(&mut self, ctx: &egui::Context, action: Action) {
        if !self.action_available(action) {
            return;
        }
        if !action.is_pdf_fit() {
            self.preview.handle_fit_shortcuts(ctx, false);
        }
        if let Some(motion) = action.motion() {
            let tab = &mut self.tabs[self.active];
            let destination = tab.language.destination(&tab.doc.text, tab.cursor, motion);
            tab.jump = Some(destination..destination);
            tab.motion_jump = true;
            self.completion = None;
            self.focus_source = true;
            self.nav_token += 1;
            self.pending_go = None;
            return;
        }
        match action {
            Action::Switcher => self.open_launcher(Mode::Surfaces),
            Action::Commands => self.open_launcher(Mode::Commands),
            Action::Outline => self.open_launcher(Mode::Outline),
            Action::ProjectFiles => {
                self.browser.open = !self.browser.open;
                self.completion = None;
                self.focus_source = self.layers.layer == Layer::Source;
                if self.browser.open
                    && let Some(tab) = self.tabs.get(self.active)
                {
                    self.browser.reveal(&tab.doc.path);
                }
            }
            Action::Source => self.source(),
            Action::Preview => self.enter_preview(ctx, false),
            Action::NextSurface | Action::PreviousSurface => {
                let current = if self.layers.layer == Layer::Preview {
                    Surface::Preview
                } else {
                    Surface::Source(self.active)
                };
                match current.cycle(self.tabs.len(), action == Action::PreviousSurface) {
                    Some(Surface::Source(i)) => {
                        self.active = i;
                        self.source();
                    }
                    Some(Surface::Preview) => self.enter_preview(ctx, false),
                    None => {}
                }
            }
            Action::Go => self.go(ctx),
            Action::NewProject => self.project_dialog(ctx, true),
            Action::OpenProject => self.project_dialog(ctx, false),
            Action::ProjectSettings => {
                if let Some(p) = &self.managed {
                    self.open_document(p.directory.join(MANIFEST));
                }
            }
            Action::ValidateProject => self.validate_project(ctx),
            Action::ExportProject => self.project_popup = Some(ProjectPopup::Export),
            Action::AddProjectFile => {
                self.project_popup = Some(ProjectPopup::AddFile {
                    path: "sections/untitled.tex".into(),
                    error: String::new(),
                    focus: true,
                })
            }
            Action::ImportFigure => self.import_figure(ctx),
            Action::Save => {
                self.save_all();
            }
            Action::Recompile => {
                self.layers.edited();
                self.enter_preview(ctx, false);
            }
            Action::Find => {
                self.completion = None;
                self.focus_source = false;
                if self.layers.layer == Layer::Source {
                    self.show_find = true;
                    self.focus_find = true;
                } else {
                    self.preview.show_find();
                }
            }
            Action::Complete => self.complete(),
            Action::ShowLog => self.show_log = !self.show_log,
            Action::Git => {
                self.show_git = !self.show_git;
                self.completion = None;
            }
            Action::CancelBuild => self.cancel.store(true, Ordering::Relaxed),
            Action::Reload => self.reload_dialog = true,
            Action::Help => self.show_help = true,
            Action::PdfZoomIn
            | Action::PdfZoomOut
            | Action::PdfFit
            | Action::PdfFitPage
            | Action::PdfNext
            | Action::PdfPrevious => self.preview.command(ctx, action),
            Action::UiZoomIn => {
                ctx.set_zoom_factor(((ctx.zoom_factor() + 0.1) * 10.).round().min(50.) / 10.)
            }
            Action::UiZoomOut => {
                ctx.set_zoom_factor(((ctx.zoom_factor() - 0.1) * 10.).round().max(2.) / 10.)
            }
            Action::UiZoomReset => ctx.set_zoom_factor(1.),
            _ => {}
        }
    }
    pub(super) fn open_launcher(&mut self, mode: Mode) {
        if self.launcher.as_ref().is_some_and(|l| l.mode == mode) {
            self.launcher = None;
            self.focus_source = self.layers.layer == Layer::Source;
            return;
        }
        self.completion = None;
        self.focus_source = false;
        self.focus_find = false;
        let mut entries = Vec::new();
        match mode {
            Mode::Surfaces => {
                for tab in &self.tabs {
                    entries.push(Entry {
                        title: format!(
                            "{}{}",
                            tab.doc
                                .path
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy(),
                            if tab.doc.is_dirty() { " •" } else { "" }
                        ),
                        detail: format!("Open · {}", self.relative_path(&tab.doc.path)),
                        target: Target::File(tab.doc.path.clone()),
                    });
                }
                if let Some(root) = &self.root {
                    entries.push(Entry {
                        title: "PDF preview".into(),
                        detail: root
                            .with_extension("pdf")
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                        target: Target::Preview,
                    });
                }
                for path in &self.files {
                    if !self.tabs.iter().any(|t| &t.doc.path == path) {
                        entries.push(Entry {
                            title: path
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned(),
                            detail: self.relative_path(path),
                            target: Target::File(path.clone()),
                        });
                    }
                }
                for action in [Action::NewProject, Action::OpenProject] {
                    entries.push(Entry {
                        title: action.label().into(),
                        detail: self.settings.keybindings.hint(action),
                        target: Target::Action(action),
                    });
                }
            }
            Mode::Commands => {
                for action in Action::ALL.into_iter().filter(|a| {
                    a.is_command() && *a != Action::Commands && self.action_available(*a)
                }) {
                    entries.push(Entry {
                        title: action.label().into(),
                        detail: self.settings.keybindings.hint(action),
                        target: Target::Action(action),
                    });
                }
            }
            Mode::Outline => {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.language.update(&tab.doc.text);
                    for entry in &tab.language.outline {
                        entries.push(Entry {
                            title: entry.title.clone(),
                            detail: format!("Line {}", entry.line),
                            target: Target::Line(entry.line),
                        });
                    }
                }
            }
        }
        self.launcher = Some(Launcher::new(mode, entries));
    }
    pub(super) fn relative_path(&self, path: &std::path::Path) -> String {
        self.root
            .as_ref()
            .and_then(|r| r.parent())
            .and_then(|r| path.strip_prefix(r).ok())
            .unwrap_or(path)
            .display()
            .to_string()
    }
    fn launcher_ui(&mut self, ctx: &egui::Context) {
        let outcome = self
            .launcher
            .as_mut()
            .map(|l| l.ui(ctx, &self.settings.keybindings));
        match outcome {
            Some(Outcome::Dismiss) => {
                self.launcher = None;
                self.focus_source = self.layers.layer == Layer::Source;
                ctx.request_discard("Release launcher focus before the next input frame");
            }
            Some(Outcome::Open(target)) => {
                self.launcher = None;
                self.focus_source = self.layers.layer == Layer::Source;
                match target {
                    Target::File(path) => self.open_document(path),
                    Target::Preview => self.enter_preview(ctx, false),
                    Target::Action(action) => self.perform_action(ctx, action),
                    Target::Line(line) => {
                        if let Some(tab) = self.tabs.get_mut(self.active) {
                            tab.jump = Some(document::line_char_range(&tab.doc.text, line));
                            self.source();
                        }
                    }
                }
                // Drop the old modal's layer before accepting input for the
                // chosen surface or search field on the next native frame.
                ctx.request_discard("Release launcher focus before the next input frame");
                ctx.request_repaint();
            }
            _ => {}
        }
    }
    fn caption_ui(&self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("workspace-caption")
            .exact_height(38.)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(theme::palette(&ctx.style().visuals).canvas))
            .show(ctx, |ui| {
                let (rect, response) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), 38.),
                    egui::Sense::click_and_drag(),
                );
                if response.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                if response.double_clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(
                        !ctx.input(|i| i.viewport().maximized.unwrap_or(false)),
                    ));
                }
                let Some(project) = &self.managed else {
                    return;
                };
                // Keep the native macOS traffic lights; only the old titlebar
                // background/title are hidden. Linux keeps its WM decorations.
                let left = rect.left() + if cfg!(target_os = "macos") { 88. } else { 24. };
                let right = rect.right() - 24.;
                let meta = if self.layers.layer == Layer::Preview {
                    format!("PDF   {}", self.preview.position_label())
                } else {
                    theme::key_label(&self.settings.keybindings.hint(Action::Switcher))
                };
                let meta_rect = ui.painter().text(
                    egui::pos2(right, rect.center().y),
                    egui::Align2::RIGHT_CENTER,
                    meta,
                    FontId::proportional(11.),
                    theme::of(ui).muted,
                );
                let project_width = 160_f32.min((rect.width() * 0.22).max(0.));
                let project_clip = egui::Rect::from_min_max(
                    egui::pos2(left, rect.top()),
                    egui::pos2(left + project_width, rect.bottom()),
                );
                let label = ui.painter().with_clip_rect(project_clip).text(
                    egui::pos2(left, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    &project.manifest.name,
                    FontId::proportional(12.),
                    theme::of(ui).secondary,
                );
                let next = label.right().min(project_clip.right()) + 18.;
                ui.painter().text(
                    egui::pos2(next, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    "/",
                    FontId::proportional(12.),
                    theme::of(ui).border,
                );
                let path = if self.layers.layer == Layer::Preview {
                    "main.pdf".into()
                } else {
                    self.tabs
                        .get(self.active)
                        .map(|t| {
                            format!(
                                "{}{}",
                                self.relative_path(&t.doc.path),
                                if t.doc.is_dirty() { "  •" } else { "" }
                            )
                        })
                        .unwrap_or_default()
                };
                let clip = egui::Rect::from_min_max(
                    egui::pos2(next + 18., rect.top()),
                    egui::pos2(meta_rect.left() - 24., rect.bottom()),
                );
                ui.painter().with_clip_rect(clip).text(
                    egui::pos2(next + 18., rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    path,
                    FontId::proportional(12.),
                    theme::of(ui).text,
                );
            });
    }
    pub(super) fn workspace_ui(&mut self, ctx: &egui::Context) {
        // Consume launcher navigation before a focused editor can see Enter/Tab.
        self.launcher_ui(ctx);
        self.caption_ui(ctx);
        let in_browser = self.file_browser_ui(ctx);
        self.layer_ui(ctx);
        if self.launcher.is_none()
            && !self.go_gesture.modified_pointer(ctx)
            && !self.show_help
            && !self.show_git
            && !self.reload_dialog
            && !self.close_dialog
            && self.project_popup.is_none()
            && self.pending_project.is_none()
            && !in_browser
            && ctx.input(|i| i.pointer.secondary_clicked())
        {
            self.open_launcher(Mode::Commands);
            ctx.request_repaint();
        }
        if self.show_git {
            let mut open = true;
            egui::Window::new("Git comparison")
                .open(&mut open)
                .default_width(480.)
                .default_height(260.)
                .vscroll(true)
                .show(ctx, |ui| {
                    if let Some(tab) = self.tabs.get_mut(self.active) {
                        let (line, _) = document::char_position(&tab.doc.text, tab.cursor);
                        if let Some(line) = self.git.comparison_ui(ui, line) {
                            tab.jump = Some(document::line_char_range(&tab.doc.text, line));
                            self.show_git = false;
                            self.source();
                        }
                    }
                });
            self.show_git &= open;
        }
        let title = if self.layers.layer == Layer::Preview {
            let file = self
                .root
                .as_ref()
                .map(|r| {
                    r.with_extension("pdf")
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned()
                })
                .unwrap_or_else(|| "Preview".into());
            let position = self.preview.position_label();
            if position.is_empty() {
                format!("{file} — Folio")
            } else {
                format!("{file} · {position} — Folio")
            }
        } else if let Some(tab) = self.tabs.get(self.active) {
            format!(
                "{}{} — Folio",
                tab.doc
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy(),
                if tab.doc.is_dirty() { " •" } else { "" }
            )
        } else {
            "Folio".into()
        };
        let title = if let Some(project) = &self.managed {
            title.replace(" — Folio", &format!(" — {} — Folio", project.manifest.name))
        } else {
            title
        };
        if title != self.window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.window_title = title;
        }
        self.feedback_ui(ctx);
    }
    fn feedback_ui(&mut self, ctx: &egui::Context) {
        if self.last_status != self.status {
            self.last_status.clone_from(&self.status);
            self.notice_until = Instant::now() + Duration::from_secs(4);
        }
        let stale = self.layers.layer == Layer::Preview && self.layers.stale();
        if self.building
            || self.exporting
            || stale
            || (!self.status.is_empty() && Instant::now() < self.notice_until)
        {
            egui::Area::new("workspace-feedback".into())
                .anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(14., -12.))
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    theme::popup(ui.visuals())
                        .inner_margin(egui::Margin::symmetric(12, 8))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                if self.building || self.exporting {
                                    let (rect, _) = ui.allocate_exact_size(
                                        egui::vec2(10., 10.),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter().circle_filled(
                                        rect.center(),
                                        2.5,
                                        theme::of(ui).accent,
                                    );
                                }
                                let message = if self.exporting {
                                    "Exporting and checking compilation…"
                                } else if self.building {
                                    "Typesetting…"
                                } else if stale {
                                    "Preview has pending changes"
                                } else {
                                    &self.status
                                };
                                if ui
                                    .add(
                                        egui::Button::new(egui::RichText::new(message).small())
                                            .frame(false),
                                    )
                                    .clicked()
                                {
                                    self.show_log = true;
                                }
                            });
                        });
                });
            if !self.building && !self.exporting && !stale {
                ctx.request_repaint_after(
                    self.notice_until.saturating_duration_since(Instant::now()),
                );
            }
        }
    }
}

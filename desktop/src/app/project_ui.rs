use super::*;
use std::path::{Component, Path};

pub(super) enum ProjectPopup {
    Export,
    AddFile {
        path: String,
        error: String,
        focus: bool,
    },
    Message {
        title: String,
        body: String,
    },
}

impl App {
    pub(super) fn project_target(&mut self, path: PathBuf, discard: bool) -> Option<PathBuf> {
        let result = (|| -> std::io::Result<_> {
            let path = path.canonicalize()?;
            let project = ManagedProject::load(&path)?;
            let switching = self
                .managed
                .as_ref()
                .is_none_or(|p| p.directory != project.directory);
            let target = project.directory.join("main.tex");
            if switching {
                if self.building || self.exporting || self.project_busy {
                    return Err(std::io::Error::other(
                        "Finish or cancel the current build/export before switching projects.",
                    ));
                }
                // Check readability before the user can discard any old buffers.
                Document::open(&target)?;
                if !discard && self.tabs.iter().any(|t| t.doc.is_dirty()) {
                    self.pending_project = Some(path);
                    return Ok(None);
                }
                self.reset_workspace();
                self.root = Some(project.directory.join("main.tex"));
                self.managed = Some(project);
            }
            Ok(Some(target))
        })();
        match result {
            Ok(path) => path,
            Err(e) => {
                self.project_message("Cannot open project", e.to_string());
                None
            }
        }
    }
    fn reset_workspace(&mut self) {
        self.tabs.clear();
        self.active = 0;
        self.root = None;
        self.files.clear();
        self.browser.clear();
        self.disk_sources.clear();
        self.disk_indexes.clear();
        self.project_stamp = None;
        self.scan_key = None;
        self.scan_generation += 1;
        self.scanning = false;
        self.preview.clear();
        self.git = crate::git_ui::Git::new(self.ctx.clone());
        self.layers = Layers::default();
        self.launcher = None;
        self.project_popup = None;
        self.pending_project = None;
        self.show_log = false;
        self.show_find = false;
        self.show_git = false;
        self.find.clear();
        self.log.clear();
        self.diagnostics.clear();
        self.source();
    }
    pub(super) fn finish_project_switch(&mut self, save: bool) {
        if save && !self.save_all() {
            return;
        }
        if let Some(path) = self.pending_project.take() {
            self.open_with_choice(path, true);
        }
    }
    pub(super) fn project_message(&mut self, title: &str, body: String) {
        self.status = title.into();
        self.project_popup = Some(ProjectPopup::Message {
            title: title.into(),
            body,
        });
        self.completion = None;
        self.focus_source = false;
    }
    pub(super) fn project_dialog(&mut self, ctx: &egui::Context, new: bool) {
        self.project_busy = true;
        let (tx, ctx, settings) = (self.tx.clone(), ctx.clone(), self.settings.clone());
        thread::spawn(move || {
            let dialog = rfd::FileDialog::new();
            let selected = if new {
                dialog
                    .set_title("Create a project folder")
                    .set_file_name("Untitled paper")
                    .save_file()
            } else {
                dialog.set_title("Open Folio project").pick_folder()
            };
            let result = selected
                .map(|path| {
                    if new {
                        let name = path.file_name().unwrap_or_default().to_string_lossy();
                        ManagedProject::create_with_timeout(
                            &path,
                            &name,
                            settings.engine,
                            settings.build_timeout,
                        )
                        .map(|p| p.directory)
                    } else {
                        ManagedProject::load(&path).map(|p| p.directory)
                    }
                })
                .transpose()
                .map_err(|e| e.to_string());
            let _ = tx.send(Event::ProjectChosen(result));
            ctx.request_repaint();
        });
    }
    pub(super) fn project_chosen(&mut self, result: Result<Option<PathBuf>, String>) {
        self.project_busy = false;
        match result {
            Ok(Some(path)) => self.open(path),
            Ok(None) => {}
            Err(e) => self.project_message("Project could not be opened", e),
        }
    }
    pub(super) fn validate_project(&mut self, ctx: &egui::Context) {
        if !self.save_all() {
            return;
        }
        let Some(project) = self.managed.clone() else {
            return;
        };
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        self.status = "Checking project…".into();
        thread::spawn(move || {
            let result = ManagedProject::load(&project.directory).and_then(|p| {
                let files = p.files()?;
                Ok(format!("{}\n{} project files · {}\n\nStructure and build settings are valid. Export also checks compilation.", p.manifest.name, files.len(), p.manifest.engine.label()))
            }).map_err(|e| e.to_string());
            let _ = tx.send(Event::ProjectChecked(project.directory, result));
            ctx.request_repaint();
        });
    }
    pub(super) fn project_checked(&mut self, directory: PathBuf, result: Result<String, String>) {
        if self
            .managed
            .as_ref()
            .is_some_and(|p| p.directory == directory)
        {
            match result {
                Ok(message) => self.project_message("Project valid", message),
                Err(e) => self.project_message("Project needs attention", e),
            }
        }
    }
    fn export_dialog(&mut self, ctx: &egui::Context) {
        let Some(p) = &self.managed else {
            return;
        };
        let parent = p.directory.parent().unwrap_or(Path::new(".")).to_owned();
        let name = format!(
            "{}-export",
            p.directory
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        );
        self.project_busy = true;
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        thread::spawn(move || {
            let path = rfd::FileDialog::new()
                .set_title("Export to a new folder")
                .set_directory(parent)
                .set_file_name(name)
                .save_file();
            let _ = tx.send(Event::ExportDestination(path));
            ctx.request_repaint();
        });
    }
    pub(super) fn start_export(&mut self, ctx: &egui::Context, path: PathBuf) {
        if self.building || self.exporting || !self.save_all() {
            return;
        }
        let Some(project) = self.managed.clone() else {
            return;
        };
        self.exporting = true;
        self.cancel = Arc::new(AtomicBool::new(false));
        self.status = "Exporting and checking compilation…".into();
        let (tx, ctx, mode, cancel) = (
            self.tx.clone(),
            ctx.clone(),
            self.export_mode,
            self.cancel.clone(),
        );
        self.build_thread = Some(thread::spawn(move || {
            let result = export::flat(&project, &path, mode, cancel).map_err(|e| e.to_string());
            let _ = tx.send(Event::Exported(result));
            ctx.request_repaint();
        }));
    }
    pub(super) fn export_finished(&mut self, result: Result<export::ExportResult, String>) {
        self.exporting = false;
        match result {
            Ok(result) => self.project_message("Export complete", format!("{}\n\n{} files · main.tex and main.pdf\nCompilation checked. Build instructions are in BUILD.txt.", result.directory.display(), result.files)),
            Err(e) => self.project_message("Export stopped", e),
        }
    }
    pub(super) fn import_figure(&mut self, ctx: &egui::Context) {
        let Some(project) = self.managed.clone() else {
            return;
        };
        self.project_busy = true;
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        thread::spawn(move || {
            let result = rfd::FileDialog::new()
                .set_title("Import figure")
                .add_filter("Figure", &["pdf", "png", "jpg", "jpeg", "eps", "svg"])
                .pick_file()
                .map(|path| {
                    let relative = Path::new("figures").join(path.file_name().unwrap_or_default());
                    project.write_new(&relative, &fs::read(&path)?)
                })
                .transpose()
                .map_err(|e| e.to_string());
            let _ = tx.send(Event::FigureImported(result));
            ctx.request_repaint();
        });
    }
    pub(super) fn figure_imported(&mut self, result: Result<Option<PathBuf>, String>) {
        self.project_busy = false;
        match result {
            Ok(Some(path)) => {
                self.refresh_project();
                self.layers.edited();
                self.project_message(
                    "Figure imported",
                    format!(
                        "Use in your source:\n\\includegraphics{{figures/{}}}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ),
                );
            }
            Ok(None) => {}
            Err(e) => self.project_message("Figure was not imported", e),
        }
    }
    pub(super) fn add_project_file(&mut self, relative: &str) -> Result<(), String> {
        let path = Path::new(relative.trim());
        if !path.components().all(|c| matches!(c, Component::Normal(_)))
            || !path.extension().is_some_and(|e| {
                ["tex", "bib", "sty", "cls", "csv", "tsv", "md"]
                    .iter()
                    .any(|x| e == *x)
            })
        {
            return Err(
                "Choose a text file such as sections/methods.tex or tables/results.tex".into(),
            );
        }
        let p = self.managed.as_ref().ok_or("Open a project first")?;
        let path = p.write_new(path, b"\n").map_err(|e| e.to_string())?;
        self.open_document(path);
        self.layers.edited();
        Ok(())
    }
    pub(super) fn project_ui(&mut self, ctx: &egui::Context) {
        if self.pending_project.is_some() {
            egui::Modal::new("switch-project".into()).show(ctx, |ui| {
                ui.heading("Switch project?");
                ui.label("Save the current project's unsaved files before switching.");
                ui.horizontal(|ui| {
                    if ui.button("Save and switch").clicked() {
                        self.finish_project_switch(true);
                    }
                    if ui.button("Discard and switch").clicked() {
                        self.finish_project_switch(false);
                    }
                    if ui.button("Keep editing").clicked() {
                        self.pending_project = None;
                    }
                });
            });
        }
        let Some(mut popup) = self.project_popup.take() else {
            return;
        };
        let mut close = false;
        egui::Modal::new("project-options".into()).show(ctx, |ui| {
            ui.set_width(450.);
            match &mut popup {
                ProjectPopup::Export => {
                    ui.heading("Export flat project");
                    ui.label("Sections and tables in main.tex. Figures beside it.");
                    ui.add_space(8.);
                    ui.radio_value(
                        &mut self.export_mode,
                        Bibliography::Keep,
                        "Keep bibliography files (.bib)",
                    );
                    ui.radio_value(
                        &mut self.export_mode,
                        Bibliography::Inline,
                        "Embed compiled bibliography in main.tex",
                    );
                    ui.add_space(8.);
                    ui.weak("Saves your files, builds a copy, then checks the exported project.");
                    ui.horizontal(|ui| {
                        if ui.button("Choose folder…").clicked() {
                            self.export_dialog(ctx);
                            close = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                }
                ProjectPopup::AddFile { path, error, focus } => {
                    ui.heading("Add project file");
                    ui.weak("sections/methods.tex · tables/results.tex · bibliography/refs.bib");
                    let length = path.chars().count();
                    let mut field = egui::TextEdit::singleline(path)
                        .id_salt("project-file-path")
                        .desired_width(f32::INFINITY)
                        .show(ui);
                    if *focus {
                        field.response.request_focus();
                        field
                            .state
                            .cursor
                            .set_char_range(Some(egui::text::CCursorRange::two(
                                egui::text::CCursor::new(0),
                                egui::text::CCursor::new(length),
                            )));
                        field.state.store(ctx, field.response.id);
                        *focus = false;
                        ctx.request_repaint();
                    }
                    if !error.is_empty() {
                        ui.colored_label(theme::of(ui).error, error.as_str());
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Create").clicked() {
                            match self.add_project_file(path) {
                                Ok(()) => close = true,
                                Err(e) => *error = e,
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                }
                ProjectPopup::Message { title, body } => {
                    ui.heading(title.as_str());
                    egui::ScrollArea::vertical()
                        .max_height(300.)
                        .show(ui, |ui| {
                            ui.label(body.as_str());
                        });
                    ui.horizontal(|ui| {
                        if ui.button("Done").clicked() {
                            close = true;
                        }
                        if ui.button("Copy details").clicked() {
                            ctx.copy_text(body.clone());
                        }
                    });
                }
            }
        });
        if !close {
            self.project_popup = Some(popup);
        } else {
            self.focus_source = self.layers.layer == Layer::Source;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use folio_core::project::{MANIFEST, Project};

    #[test]
    fn loose_file_open_is_refused_without_discarding_the_active_project() {
        let dir =
            std::env::temp_dir().join(format!("folio-project-only-ui-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let loose = dir.join("loose.tex");
        fs::write(&loose, "loose source").unwrap();
        let ctx = egui::Context::default();
        let mut app = App::with_context(ctx.clone(), Some(loose.clone()));
        assert!(app.tabs.is_empty());
        assert!(app.root.is_none());
        assert!(app.status.contains("project"));
        let project = Project::create(&dir.join("paper"), "Paper", Engine::PdfLatex).unwrap();
        app.project_popup = None;
        app.open(project.directory.clone());
        app.tabs[0].doc.text.push_str("Unsaved edits");
        app.open(loose);
        assert_eq!(app.managed.as_ref().unwrap().directory, project.directory);
        assert_eq!(app.tabs.len(), 1);
        assert!(app.tabs[0].doc.text.ends_with("Unsaved edits"));
        assert!(app.pending_project.is_none());
        drop(app);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn source_open_and_inverse_navigation_cannot_escape_the_current_project() {
        let dir =
            std::env::temp_dir().join(format!("folio-project-boundary-{}", std::process::id()));
        let project = Project::create(&dir, "Paper", Engine::PdfLatex).unwrap();
        let outside = dir.with_extension("tex");
        fs::write(&outside, "outside source").unwrap();
        let ctx = egui::Context::default();
        let mut app = App::with_context(ctx, Some(dir.clone()));
        app.open_document(outside.clone());
        assert_eq!(app.tabs.len(), 1);
        app.jump_source_word(
            SourceLocation {
                path: outside.clone(),
                line: 1,
                column: 0,
            },
            None,
        );
        assert_eq!(app.tabs.len(), 1);
        assert_eq!(
            app.tabs[app.active].doc.path,
            project.directory.join("main.tex")
        );
        #[cfg(unix)]
        {
            let link = project.directory.join("sections/link.tex");
            std::os::unix::fs::symlink(&outside, &link).unwrap();
            app.open_document(link);
            assert_eq!(app.tabs.len(), 1);
        }
        drop(app);
        fs::remove_dir_all(dir).unwrap();
        fs::remove_file(outside).unwrap();
    }

    #[test]
    fn welcome_screen_offers_project_creation_and_opening_only() {
        let ctx = egui::Context::default();
        let mut app = App::with_context(ctx.clone(), None);
        fn text(shape: &egui::Shape, labels: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(s) => labels.push(s.galley.job.text.clone()),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| text(s, labels)),
                _ => {}
            }
        }
        let mut labels = Vec::new();
        for _ in 0..2 {
            labels.clear();
            let output = ctx.run(egui::RawInput::default(), |ctx| app.workspace_ui(ctx));
            for shape in output.shapes {
                text(&shape.shape, &mut labels);
            }
        }
        assert!(labels.iter().any(|s| s.contains("New project")));
        assert!(labels.iter().any(|s| s.contains("Open project")));
        assert!(
            !labels
                .iter()
                .any(|s| s.contains("Open file") || s.contains("New file"))
        );
        for action in Action::ALL
            .into_iter()
            .filter(|a| a.is_command() && app.action_available(*a))
        {
            assert!(
                ![
                    "Open file…",
                    "New file…",
                    "Set active file as main",
                    "Save a copy…",
                    "Use pdfLaTeX",
                    "Use XeLaTeX",
                    "Use LuaLaTeX"
                ]
                .contains(&action.label())
            );
        }
    }

    #[test]
    fn add_file_dialog_takes_typing_immediately_without_editing_the_source() {
        let dir = std::env::temp_dir().join(format!("folio-project-focus-{}", std::process::id()));
        Project::create(&dir, "Paper", Engine::PdfLatex).unwrap();
        let ctx = egui::Context::default();
        let mut app = App::with_context(ctx.clone(), Some(dir.clone()));
        let original = app.tabs[0].doc.text.clone();
        let frame = |app: &mut App, events| {
            let _ = ctx.run(
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
                    app.project_ui(ctx);
                },
            );
        };
        frame(&mut app, vec![]);
        app.perform_action(&ctx, Action::AddProjectFile);
        frame(&mut app, vec![]);
        frame(
            &mut app,
            vec![egui::Event::Text("tables/measurements.tex".into())],
        );
        match &app.project_popup {
            Some(ProjectPopup::AddFile { path, .. }) => assert_eq!(path, "tables/measurements.tex"),
            _ => panic!("Dialog closed unexpectedly"),
        }
        assert_eq!(app.tabs[0].doc.text, original);
        drop(app);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn cycling_surfaces_during_export_does_not_queue_an_unstarted_build() {
        let dir =
            std::env::temp_dir().join(format!("folio-project-export-cycle-{}", std::process::id()));
        Project::create(&dir, "Paper", Engine::PdfLatex).unwrap();
        let ctx = egui::Context::default();
        let mut app = App::with_context(ctx.clone(), Some(dir.clone()));
        super::super::tests::settle(&mut app, &ctx, |a| !a.scanning);
        app.exporting = true;
        app.perform_action(&ctx, Action::NextSurface);
        assert_eq!(app.layers.layer, Layer::Source);
        assert!(!app.preview_pending);
        assert!(!app.building);
        app.exporting = false;
        drop(app);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn managed_project_open_uses_manifest_and_settings_are_an_editable_surface() {
        let dir = std::env::temp_dir().join(format!("folio-managed-ui-{}", std::process::id()));
        let project = Project::create(&dir, "Test paper", Engine::XeLatex).unwrap();
        let ctx = egui::Context::default();
        let mut app = App::with_context(ctx.clone(), Some(dir.clone()));
        super::super::tests::settle(&mut app, &ctx, |a| !a.scanning);
        assert_eq!(app.root, Some(project.directory.join("main.tex")));
        assert_eq!(
            app.managed.as_ref().unwrap().manifest.engine,
            Engine::XeLatex
        );
        assert!(
            !Action::ALL
                .iter()
                .any(|a| matches!(a.name(), "set_main" | "engine_pdflatex"))
        );
        assert!(app.action_available(Action::ExportProject));
        app.perform_action(&ctx, Action::ProjectSettings);
        assert_eq!(
            app.tabs[app.active].doc.path,
            project.directory.join(MANIFEST)
        );
        app.tabs[app.active].doc.text =
            app.tabs[app.active].doc.text.replace("xelatex", "lualatex");
        assert!(app.save_all());
        super::super::tests::settle(&mut app, &ctx, |a| !a.scanning);
        assert_eq!(
            app.managed.as_ref().unwrap().manifest.engine,
            Engine::LuaLatex
        );
        app.perform_action(&ctx, Action::ExportProject);
        assert!(matches!(app.project_popup, Some(ProjectPopup::Export)));
        drop(app);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn switching_projects_preserves_dirty_buffers_until_a_deliberate_choice() {
        let base = std::env::temp_dir().join(format!("folio-switch-ui-{}", std::process::id()));
        fs::create_dir_all(&base).unwrap();
        let a = Project::create(&base.join("a"), "A", Engine::PdfLatex).unwrap();
        let b = Project::create(&base.join("b"), "B", Engine::XeLatex).unwrap();
        let ctx = egui::Context::default();
        let mut app = App::with_context(ctx.clone(), Some(a.directory.clone()));
        app.tabs[0].doc.text.push_str("\n% unsaved change");
        app.open(b.directory.clone());
        assert_eq!(app.root, Some(a.directory.join("main.tex")));
        assert_eq!(app.pending_project, Some(b.directory.clone()));
        assert!(app.tabs[0].doc.is_dirty());
        // Save and switch writes all old buffers before clearing their surfaces.
        app.finish_project_switch(true);
        assert_eq!(app.root, Some(b.directory.join("main.tex")));
        assert_eq!(app.tabs.len(), 1);
        assert!(app.pending_project.is_none());
        assert!(
            fs::read_to_string(a.directory.join("main.tex"))
                .unwrap()
                .contains("unsaved change")
        );
        app.building = true;
        app.open(a.directory.clone());
        assert_eq!(
            app.root,
            Some(b.directory.join("main.tex")),
            "don't mix build results from different projects"
        );
        app.building = false;
        super::super::tests::settle(&mut app, &ctx, |a| !a.scanning);
        drop(app);
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn malformed_manifest_does_not_silently_fall_back_to_loose_file_builds() {
        let dir = std::env::temp_dir().join(format!("folio-invalid-ui-{}", std::process::id()));
        let p = Project::create(&dir, "Test", Engine::PdfLatex).unwrap();
        let ctx = egui::Context::default();
        let mut app = App::with_context(ctx.clone(), Some(dir.clone()));
        fs::write(dir.join(MANIFEST), "schema = 99").unwrap();
        app.refresh_project();
        super::super::tests::settle(&mut app, &ctx, |a| !a.scanning);
        assert!(app.managed.is_some());
        assert!(app.managed.as_ref().unwrap().build_config().is_err());
        app.perform_action(&ctx, Action::ProjectSettings);
        assert_eq!(app.tabs[app.active].doc.path, p.directory.join(MANIFEST));
        drop(app);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn project_files_are_created_in_their_roles_and_dirty_switch_save_conflicts_are_safe() {
        let base =
            std::env::temp_dir().join(format!("folio-project-file-ui-{}", std::process::id()));
        fs::create_dir_all(&base).unwrap();
        let a = Project::create(&base.join("a"), "A", Engine::PdfLatex).unwrap();
        let b = Project::create(&base.join("b"), "B", Engine::PdfLatex).unwrap();
        let ctx = egui::Context::default();
        let mut app = App::with_context(ctx.clone(), Some(a.directory.clone()));
        assert!(app.add_project_file("table.tex").is_err());
        app.add_project_file("tables/results.tex").unwrap();
        assert_eq!(
            app.tabs[app.active].doc.path,
            a.directory.join("tables/results.tex")
        );
        assert!(app.add_project_file("tables/results.tex").is_err());
        app.tabs[app.active].doc.text = "unsaved table".into();
        fs::write(a.directory.join("tables/results.tex"), "external edit").unwrap();
        app.open(b.directory.clone());
        app.finish_project_switch(true);
        assert_eq!(app.root, Some(a.directory.join("main.tex")));
        assert!(app.pending_project.is_some());
        assert_eq!(app.tabs[app.active].doc.text, "unsaved table");
        assert_eq!(
            fs::read_to_string(a.directory.join("tables/results.tex")).unwrap(),
            "external edit"
        );
        app.finish_project_switch(false);
        assert_eq!(app.root, Some(b.directory.join("main.tex")));
        super::super::tests::settle(&mut app, &ctx, |a| !a.scanning);
        drop(app);
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    #[ignore = "requires latexmk, TeX, latexpand and PDFium"]
    fn desktop_exports_saved_buffers_and_verifies_pdf_content_in_both_modes() {
        use crate::pdf_worker::bind;
        let base =
            std::env::temp_dir().join(format!("folio-desktop-export-{}", std::process::id()));
        fs::create_dir_all(&base).unwrap();
        let p = Project::create(&base.join("paper"), "Paper", Engine::PdfLatex).unwrap();
        fs::write(p.directory.join("references.bib"), "@book{knuth,author={Donald Knuth},title={The TeXbook},year={1984},publisher={Addison-Wesley}}").unwrap();
        let ctx = egui::Context::default();
        let mut app = App::with_context(ctx.clone(), Some(p.directory.clone()));
        app.open_document(p.directory.join("sections/introduction.tex"));
        app.tabs[app.active].doc.text =
            "\\section{Methods}\nSaved buffer content.\\cite{knuth}\n\\input{tables/results}\n"
                .into();
        p.write_new(std::path::Path::new("tables/results.tex"), b"Result: 42.")
            .unwrap();
        let pdfium = bind().unwrap();
        for mode in [Bibliography::Keep, Bibliography::Inline] {
            app.export_mode = mode;
            app.start_export(&ctx, base.join(format!("export-{mode:?}")));
            assert!(app.exporting);
            super::super::tests::settle(&mut app, &ctx, |a| !a.exporting);
            match &app.project_popup {
                Some(ProjectPopup::Message { title, body }) => {
                    assert_eq!(title, "Export complete", "{body}")
                }
                _ => panic!("Missing export result"),
            }
            let pdf = pdfium
                .load_pdf_from_file(&base.join(format!("export-{mode:?}/main.pdf")), None)
                .unwrap();
            let text = pdf
                .pages()
                .iter()
                .map(|page| page.text().unwrap().all())
                .collect::<String>();
            for expected in [
                "Saved buffer content",
                "Result: 42",
                "The TeXbook",
                "Donald Knuth",
            ] {
                assert!(text.contains(expected), "Missing {expected}: {text}");
            }
            assert!(!app.tabs[app.active].doc.is_dirty());
        }
        app.project_popup = None;
        app.start_export(&ctx, base.join("cancelled"));
        app.perform_action(&ctx, Action::CancelBuild);
        super::super::tests::settle(&mut app, &ctx, |a| !a.exporting);
        assert!(!base.join("cancelled").exists());
        assert!(!fs::read_dir(&base).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".folio-export-")
        }));
        drop(app);
        fs::remove_dir_all(base).unwrap();
    }
}

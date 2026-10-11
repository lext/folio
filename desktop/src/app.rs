mod file_browser_ui;
#[cfg(test)]
mod motion_tests;
#[cfg(test)]
mod pdf_fit_tests;
mod project_ui;
mod workspace_ui;
use crate::{
    language::Language,
    layers::{Layer, Layers},
    preview::Preview,
    settings::{Action, Settings},
    theme,
};
use eframe::egui::{self, FontId, RichText};
#[cfg(test)]
use folio_core::build::Engine;
use folio_core::{
    bibliography,
    build::{self, BuildResult, Diagnostic},
    completion::{self, Suggestions},
    document::{self, Document},
    export::{self, Bibliography},
    navigation::{self, WordAnchor},
    project::{MANIFEST, Project as ManagedProject},
    synctex::{self, PdfLocation, SourceLocation},
};
use project_ui::ProjectPopup;
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::{Duration, Instant},
};

#[cfg(test)]
const SAMPLE: &str = include_str!("../tests/fixtures/welcome.tex");
struct Tab {
    doc: Document,
    language: Language,
    cursor: usize,
    selection: std::ops::Range<usize>,
    jump: Option<std::ops::Range<usize>>,
    motion_jump: bool,
    revision: u64,
    indexed_revision: Option<u64>,
    index: Arc<completion::Index>,
    line_count: usize,
}
impl Tab {
    fn edited(&mut self) {
        self.revision += 1;
        self.line_count = self.doc.text.bytes().filter(|b| *b == b'\n').count() + 1;
    }
    fn adopt_disk(&mut self, disk: &str) -> bool {
        let adopted = self.doc.adopt_disk(disk);
        if adopted {
            self.edited();
            let length = self.doc.text.chars().count();
            self.cursor = self.cursor.min(length);
            self.selection = self.selection.start.min(length)..self.selection.end.min(length);
        }
        adopted
    }
    fn refresh_index(&mut self) {
        if self.indexed_revision == Some(self.revision) {
            return;
        }
        self.index = Arc::new(if !self.language.is_latex() {
            completion::Index::default()
        } else if self
            .doc
            .path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("bib"))
        {
            completion::Index::bibliography(&bibliography::parse(&self.doc.text))
        } else {
            completion::Index::source(&self.doc.text)
        });
        self.indexed_revision = Some(self.revision);
    }
}
struct Completion {
    suggestions: Suggestions,
    selected: usize,
    anchor: egui::Pos2,
}
type SourceTarget = (u64, PathBuf, usize, usize);
enum Event {
    ProjectChosen(Result<Option<PathBuf>, String>),
    ProjectChecked(PathBuf, Result<String, String>),
    ExportDestination(Option<PathBuf>),
    Exported(Result<export::ExportResult, String>),
    FigureImported(Result<Option<PathBuf>, String>),
    AssetOpened(Result<(), String>),
    Scanned(u64, u64, crate::project::Snapshot),
    Build(Result<BuildResult, String>),
    Source(u64, Result<SourceLocation, String>, Option<WordAnchor>),
    Destination(u64, Result<Vec<PdfLocation>, String>),
}
pub struct App {
    launcher: Option<crate::launcher::Launcher>,
    show_git: bool,
    last_status: String,
    notice_until: Instant,
    window_title: String,
    settings: Arc<Settings>,
    config_path: Option<PathBuf>,
    config_error: Option<String>,
    trace: Option<crate::performance::FrameTrace>,
    tabs: Vec<Tab>,
    active: usize,
    root: Option<PathBuf>,
    managed: Option<ManagedProject>,
    pending_project: Option<PathBuf>,
    project_popup: Option<ProjectPopup>,
    project_busy: bool,
    export_mode: Bibliography,
    exporting: bool,
    files: Vec<PathBuf>,
    browser: crate::file_browser::Browser,
    disk_sources: HashMap<PathBuf, String>,
    disk_indexes: HashMap<PathBuf, Arc<completion::Index>>,
    ctx: egui::Context,
    preview_pending: bool,
    project_stamp: Option<u64>,
    last_scan: Instant,
    scan_generation: u64,
    scanning: bool,
    preview: Preview,
    git: crate::git_ui::Git,
    layers: Layers,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    building: bool,
    build_thread: Option<thread::JoinHandle<()>>,
    cancel: Arc<AtomicBool>,
    status: String,
    log: String,
    diagnostics: Vec<Diagnostic>,
    show_log: bool,
    close_dialog: bool,
    reload_dialog: bool,
    allow_close: bool,
    find: String,
    show_find: bool,
    focus_find: bool,
    focus_source: bool,
    completion: Option<Completion>,
    caret: egui::Pos2,
    nav_token: u64,
    go_gesture: crate::navigation::GoGesture,
    pending_go: Option<SourceTarget>,
    show_help: bool,
}
impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        path: Option<PathBuf>,
        loaded: crate::settings::Loaded,
        error: Option<String>,
    ) -> Self {
        let mut app = Self::with_settings(cc.egui_ctx.clone(), path, loaded.settings);
        app.config_path = loaded.path;
        app.config_error = error;
        app.trace = crate::performance::FrameTrace::from_env(cc);
        app
    }
    #[cfg(test)]
    fn with_context(ctx: egui::Context, path: Option<PathBuf>) -> Self {
        Self::with_settings(ctx, path, Arc::new(Settings::default()))
    }
    fn with_settings(ctx: egui::Context, path: Option<PathBuf>, settings: Arc<Settings>) -> Self {
        // Dispatch UI zoom through our configurable action registry. Egui's
        // built-in handler would otherwise keep its old shortcuts active.
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
        theme::apply(&ctx, settings.editor.font_size, settings.theme);
        let (tx, rx) = mpsc::channel();
        let mut app = Self {
            launcher: None,
            show_git: false,
            last_status: String::new(),
            notice_until: Instant::now(),
            window_title: String::new(),
            settings: settings.clone(),
            config_path: None,
            config_error: None,
            trace: None,
            tabs: vec![],
            active: 0,
            root: None,
            managed: None,
            pending_project: None,
            project_popup: None,
            project_busy: false,
            export_mode: Bibliography::Keep,
            exporting: false,
            files: vec![],
            browser: Default::default(),
            disk_sources: HashMap::new(),
            disk_indexes: HashMap::new(),
            ctx: ctx.clone(),
            preview_pending: false,
            project_stamp: None,
            last_scan: Instant::now(),
            scan_generation: 0,
            scanning: false,
            git: crate::git_ui::Git::new(ctx.clone()),
            preview: Preview::with_settings(ctx, settings.clone()),
            layers: Layers::default(),
            tx,
            rx,
            building: false,
            build_thread: None,
            cancel: Arc::new(AtomicBool::new(false)),
            status: String::new(),
            log: String::new(),
            diagnostics: vec![],
            show_log: false,
            close_dialog: false,
            reload_dialog: false,
            allow_close: false,
            find: String::new(),
            show_find: false,
            focus_find: false,
            focus_source: true,
            completion: None,
            caret: egui::Pos2::ZERO,
            nav_token: 0,
            go_gesture: crate::navigation::GoGesture::new(settings.mouse.go.clone()),
            pending_go: None,
            show_help: false,
        };
        if let Some(path) = path {
            app.open(path);
        }
        app
    }
    fn source(&mut self) {
        self.preview_pending = false;
        self.nav_token += 1;
        self.pending_go = None;
        self.layers.source();
        if let Some(tab) = self.tabs.get(self.active) {
            self.browser.reveal(&tab.doc.path);
        }
        self.focus_source = true;
        self.completion = None;
    }
    fn open(&mut self, path: PathBuf) {
        self.open_with_choice(path, false);
    }
    fn open_with_choice(&mut self, path: PathBuf, discard: bool) {
        if let Some(path) = self.project_target(path, discard) {
            self.open_document(path);
        }
    }
    fn open_document(&mut self, path: PathBuf) {
        self.completion = None;
        let Ok(canonical) = path.canonicalize() else {
            self.status = format!("Cannot open {}", path.display());
            return;
        };
        if !self
            .managed
            .as_ref()
            .is_some_and(|project| canonical.starts_with(&project.directory))
        {
            self.status =
                "Open a project first. Source files must belong to the current project.".into();
            return;
        }
        if let Some(index) = self.tabs.iter().position(|t| t.doc.path == canonical) {
            self.active = index;
            self.source();
            return;
        }
        match Document::open(&canonical) {
            Ok(doc) => {
                self.tabs.push(Tab {
                    line_count: doc.text.bytes().filter(|b| *b == b'\n').count() + 1,
                    revision: 0,
                    indexed_revision: None,
                    index: Arc::default(),
                    language: Language::for_path(&doc.path),
                    doc,
                    cursor: 0,
                    selection: 0..0,
                    jump: None,
                    motion_jump: false,
                });
                self.active = self.tabs.len() - 1;
                self.source();
                self.refresh_project();
                self.status.clear();
            }
            Err(e) => self.status = e.to_string(),
        }
    }
    fn scan_buffers(&self) -> Vec<(PathBuf, String)> {
        self.tabs
            .iter()
            .map(|t| (t.doc.path.clone(), t.doc.text.clone()))
            .collect()
    }
    fn apply_project(&mut self, snapshot: crate::project::Snapshot) {
        if self.project_stamp.is_some_and(|old| old != snapshot.stamp) {
            self.layers.edited();
        }
        if let Some(Ok(project)) = snapshot.managed.as_ref() {
            self.managed = Some(project.clone());
        }
        self.project_stamp = Some(snapshot.stamp);
        self.files = snapshot
            .files
            .into_iter()
            .filter(|path| {
                self.managed
                    .as_ref()
                    .is_some_and(|project| path.starts_with(&project.directory))
            })
            .collect();
        self.disk_sources = snapshot.sources;
        self.disk_indexes = snapshot.indexes;
        // Follow external edits (git pull, another editor) in buffers without
        // unsaved changes. Edited buffers keep their text and conflict on save.
        for tab in &mut self.tabs {
            if let Some(disk) = self.disk_sources.get(&tab.doc.path) {
                tab.adopt_disk(disk);
            }
        }
        if let Some(directory) = self.root.as_ref().and_then(|r| r.parent()) {
            self.browser.replace(directory.to_owned(), snapshot.tree);
        }
        if self
            .launcher
            .as_ref()
            .is_some_and(|l| l.mode == crate::launcher::Mode::Surfaces)
        {
            let previous = self.launcher.take().unwrap();
            self.open_launcher(previous.mode);
            if let Some(launcher) = &mut self.launcher {
                launcher.query = previous.query;
                launcher.selected = previous.selected;
            }
        }
    }
    fn refresh_project(&mut self) {
        self.scan_generation += 1;
        self.scanning = false;
        self.request_project_scan(&self.ctx.clone());
    }
    fn request_project_scan(&mut self, ctx: &egui::Context) {
        if self.scanning {
            return;
        }
        self.last_scan = Instant::now();
        let Some(project) = self.managed.clone() else {
            return;
        };
        self.scan_generation += 1;
        self.scanning = true;
        let generation = self.scan_generation;
        let revision = self.layers.revision;
        let buffers = self.scan_buffers();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let snapshot = crate::project::scan(&project.directory.join("main.tex"), &buffers);
            let _ = tx.send(Event::Scanned(generation, revision, snapshot));
            ctx.request_repaint();
        });
    }
    fn save_all(&mut self) -> bool {
        // An unedited buffer cannot conflict: take the newer file instead of
        // refusing to save, without waiting for the next background scan.
        let mut reloaded = false;
        for tab in self.tabs.iter_mut().filter(|tab| !tab.doc.is_dirty()) {
            if let Ok(disk) = fs::read_to_string(&tab.doc.path) {
                reloaded |= tab.adopt_disk(&disk);
            }
        }
        if reloaded {
            // The preview must be rebuilt from the text that was just loaded.
            self.layers.edited();
        }
        let mut errors: Vec<_> = self
            .tabs
            .iter()
            .filter_map(|tab| {
                tab.doc
                    .check_disk()
                    .err()
                    .map(|e| format!("{}: {e}", tab.doc.path.display()))
            })
            .collect();
        if !errors.is_empty() {
            self.status = format!("Save failed:\n{}", errors.join("\n"));
            return false;
        }
        let changed = self.tabs.iter().any(|tab| tab.doc.is_dirty());
        for tab in &mut self.tabs {
            if let Err(e) = tab.doc.save() {
                errors.push(format!("{}: {e}", tab.doc.path.display()));
            }
        }
        if changed {
            self.refresh_project();
        }
        if !errors.is_empty() {
            self.status = format!("Save failed:\n{}", errors.join("\n"));
            return false;
        }
        self.status = "Saved".into();
        true
    }
    fn enter_preview(&mut self, ctx: &egui::Context, go: bool) {
        if self.root.is_none() || self.exporting {
            return;
        }
        self.nav_token += 1;
        self.pending_go = None;
        self.completion = None;
        if go && let Some(tab) = self.tabs.get(self.active) {
            let (line, _) = document::char_position(&tab.doc.text, tab.selection.start);
            // SyncTeX defines 0 as an unspecified column. TeX engines generally
            // record lines; do not guess character/byte units for Unicode text.
            self.pending_go = Some((self.nav_token, tab.doc.path.clone(), line, 0));
        }
        // Persist first; dependency indexing completes off the event thread.
        if !self.save_all() {
            self.pending_go = None;
            return;
        }
        if self.layers.layer == Layer::Source {
            self.preview.activate();
        }
        if self.scanning && self.layers.stale() {
            self.layers.layer = Layer::Preview;
            self.preview_pending = true;
        } else if self.layers.enter_preview() {
            self.start_build(ctx);
        } else if !self.layers.stale() {
            self.status.clear();
        }
    }
    fn start_build(&mut self, ctx: &egui::Context) {
        if self.building || self.exporting {
            return;
        }
        let Some(project) = self.managed.clone() else {
            self.layers.finished(false);
            return;
        };
        self.cancel = Arc::new(AtomicBool::new(false));
        self.building = true;
        self.status = "Typesetting…".into();
        self.show_log = false;
        let cancel = self.cancel.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        self.build_thread = Some(thread::spawn(move || {
            let result = project
                .build_config()
                .and_then(|config| build::compile(&config, cancel))
                .map_err(|e| e.to_string());
            let _ = tx.send(Event::Build(result));
            ctx.request_repaint();
        }));
    }
    fn go(&mut self, ctx: &egui::Context) {
        if self.layers.layer == Layer::Source {
            self.enter_preview(ctx, true);
            return;
        }
        let Some((page, x, y)) = self.preview.go_target() else {
            self.status = format!("Select PDF text, then {}", self.settings.go_hint());
            return;
        };
        let Some(pdf) = self.preview.path.clone() else {
            return;
        };
        let word = self.preview.go_word();
        self.nav_token += 1;
        let token = self.nav_token;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let _ = tx.send(Event::Source(
                token,
                synctex::inverse(&pdf, page, x, y).map_err(|e| e.to_string()),
                word,
            ));
            ctx.request_repaint();
        });
    }
    fn jump_source(&mut self, path: PathBuf, line: usize) {
        let canonical = path.canonicalize().ok();
        self.open_document(path);
        if let Some(tab) = self.tabs.get_mut(self.active)
            && canonical.as_ref() == Some(&tab.doc.path)
        {
            tab.jump = Some(document::line_char_range(&tab.doc.text, line));
        }
    }
    fn jump_source_word(&mut self, location: SourceLocation, word: Option<WordAnchor>) {
        let canonical = location.path.canonicalize().ok();
        self.open_document(location.path);
        if let Some(tab) = self.tabs.get_mut(self.active)
            && canonical.as_ref() == Some(&tab.doc.path)
        {
            let range = word
                .as_ref()
                .and_then(|word| navigation::source_word(&tab.doc.text, location.line, word));
            tab.jump = Some(range.unwrap_or_else(|| {
                let start = document::line_char_range(&tab.doc.text, location.line).start;
                start..start
            }));
        }
    }
    fn poll(&mut self, ctx: &egui::Context) {
        self.preview.poll(ctx);
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::ProjectChosen(result) => self.project_chosen(result),
                Event::ProjectChecked(directory, result) => self.project_checked(directory, result),
                Event::ExportDestination(path) => {
                    self.project_busy = false;
                    if let Some(path) = path {
                        self.start_export(ctx, path);
                    }
                }
                Event::Exported(result) => self.export_finished(result),
                Event::FigureImported(result) => self.figure_imported(result),
                Event::AssetOpened(Err(error)) => self.status = error,
                Event::AssetOpened(Ok(())) => {}
                Event::Scanned(generation, revision, snapshot)
                    if generation == self.scan_generation =>
                {
                    self.scanning = false;
                    if revision == self.layers.revision {
                        self.apply_project(snapshot);
                    } else {
                        // A scan must not overwrite newer unsaved bibliography state.
                        self.last_scan = Instant::now() - Duration::from_secs(3);
                    }
                }
                Event::Build(result) => {
                    self.building = false;
                    match result {
                        Ok(result) => {
                            self.log = result.log;
                            self.diagnostics = result.diagnostics;
                            self.layers.finished(result.success);
                            if result.success {
                                self.status =
                                    format!("Typeset in {:.2}s", result.elapsed.as_secs_f32());
                                self.preview.load(result.pdf);
                            } else {
                                self.status = if result.cancelled {
                                    "Typesetting cancelled"
                                } else {
                                    "Typesetting failed · previous PDF retained"
                                }
                                .into();
                                self.pending_go = None;
                                self.show_log = true;
                            }
                        }
                        Err(e) => {
                            self.layers.finished(false);
                            self.pending_go = None;
                            self.status = format!("Typesetting failed: {e}");
                            self.log.clone_from(&self.status);
                            self.show_log = true;
                        }
                    }
                }
                Event::Source(token, result, word) if token == self.nav_token => match result {
                    Ok(location) => self.jump_source_word(location, word),
                    Err(e) => self.status = e,
                },
                Event::Destination(token, result)
                    if token == self.nav_token && self.layers.layer == Layer::Preview =>
                {
                    match result {
                        Ok(locations) => {
                            if let Some(location) = locations.into_iter().next() {
                                self.preview.jump(location);
                            }
                        }
                        Err(e) => self.status = e,
                    }
                }
                _ => {}
            }
        }
        if self.preview.load_failed() && self.layers.built.is_some() {
            self.pending_go = None;
            self.layers.preview_failed();
        }
        if self.preview_pending && !self.scanning && !self.exporting {
            self.preview_pending = false;
            if self.layers.layer == Layer::Preview && self.layers.enter_preview() {
                self.start_build(ctx);
            }
        }
        if !self.building
            && !self.layers.stale()
            && !self.preview.is_loading()
            && self.layers.layer == Layer::Preview
            && let Some((token, source, line, column)) = self.pending_go.take()
            && let Some(pdf) = self.preview.path.clone()
        {
            let tx = self.tx.clone();
            let ctx = ctx.clone();
            thread::spawn(move || {
                let _ = tx.send(Event::Destination(
                    token,
                    synctex::forward(&pdf, &source, line, column).map_err(|e| e.to_string()),
                ));
                ctx.request_repaint();
            });
        }
    }
    fn complete(&mut self) {
        if !self
            .tabs
            .get(self.active)
            .is_some_and(|t| t.language.is_latex())
        {
            self.completion = None;
            return;
        }
        for tab in &mut self.tabs {
            tab.refresh_index();
        }
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let cursor = document::char_to_byte(&tab.doc.text, tab.cursor);
        let disk = self
            .disk_indexes
            .iter()
            .filter(|(p, _)| !self.tabs.iter().any(|t| &t.doc.path == *p))
            .map(|(_, index)| index.as_ref());
        let indexes = disk.chain(self.tabs.iter().map(|t| t.index.as_ref()));
        self.completion = completion::suggest_indexed(&tab.doc.text, cursor, &tab.index, indexes)
            .map(|suggestions| Completion {
                suggestions,
                selected: 0,
                anchor: self.caret,
            });
    }
    fn accept_completion(&mut self, index: usize) {
        if let Some(c) = self.completion.take()
            && let Some(item) = c.suggestions.items.get(index)
            && let Some(tab) = self.tabs.get_mut(self.active)
        {
            let range = c.suggestions.range;
            if range.end <= tab.doc.text.len()
                && tab.doc.text.is_char_boundary(range.start)
                && tab.doc.text.is_char_boundary(range.end)
            {
                tab.doc.text.replace_range(range.clone(), &item.insert);
                tab.edited();
                let cursor = tab.doc.text[..range.start + item.insert.len()]
                    .chars()
                    .count();
                tab.jump = Some(cursor..cursor);
                self.layers.edited();
            }
        }
    }
    fn find_next(&mut self) {
        if self.find.is_empty() {
            return;
        }
        if let Some(tab) = self.tabs.get_mut(self.active) {
            let from = document::char_to_byte(&tab.doc.text, tab.cursor.saturating_add(1));
            if let Some(byte) = tab.doc.text[from..]
                .find(&self.find)
                .map(|i| i + from)
                .or_else(|| tab.doc.text.find(&self.find))
            {
                let start = tab.doc.text[..byte].chars().count();
                tab.jump = Some(start..start + self.find.chars().count());
            }
        }
    }
    fn find_previous(&mut self) {
        if self.find.is_empty() {
            return;
        }
        if let Some(tab) = self.tabs.get_mut(self.active) {
            let from = document::char_to_byte(
                &tab.doc.text,
                if tab.selection.is_empty() {
                    tab.cursor
                } else {
                    tab.selection.start
                },
            );
            if let Some(byte) = tab.doc.text[..from]
                .rfind(&self.find)
                .or_else(|| tab.doc.text.rfind(&self.find))
            {
                let start = tab.doc.text[..byte].chars().count();
                tab.jump = Some(start..start + self.find.chars().count());
            }
        }
    }
    fn source_ui(&mut self, ui: &mut egui::Ui) -> bool {
        if self.tabs.is_empty() {
            ui.add_space((ui.available_height() * 0.23).max(24.));
            ui.horizontal(|ui| {
                ui.add_space(((ui.available_width() - 360.) / 2.).max(0.));
                ui.vertical(|ui| {
                    ui.set_width(360_f32.min(ui.available_width()));
                    ui.label(RichText::new("Folio").size(42.).color(theme::of(ui).text));
                    ui.add_space(8.);
                    ui.label(
                        RichText::new("A place for your next paper.")
                            .size(14.)
                            .color(theme::of(ui).secondary),
                    );
                    ui.add_space(32.);
                    for action in [Action::NewProject, Action::OpenProject] {
                        let response = ui
                            .add_enabled_ui(self.action_available(action), |ui| {
                                theme::row(
                                    ui,
                                    action.label().trim_end_matches('…'),
                                    "",
                                    &self.settings.keybindings.hint(action),
                                    false,
                                )
                            })
                            .inner;
                        if response.clicked() {
                            self.perform_action(ui.ctx(), action);
                        }
                    }
                    ui.add_space(24.);
                    ui.label(theme::caption(
                        ui,
                        format!(
                            "{}   Switch surface",
                            theme::key_label(&self.settings.keybindings.hint(Action::Switcher))
                        ),
                    ));
                    ui.label(theme::caption(
                        ui,
                        format!(
                            "{}   Commands · or right-click anywhere",
                            theme::key_label(&self.settings.keybindings.hint(Action::Commands))
                        ),
                    ));
                });
            });
            return false;
        }
        let mut searched = false;
        if self.show_find {
            theme::popup(ui.visuals()).inner_margin(10.).show(ui, |ui| {
                ui.horizontal(|ui| {
                    let find_id = ui.make_persistent_id("source-find");
                    if self.focus_find {
                        if ui.memory(|m| m.allows_interaction(ui.layer_id())) {
                            ui.memory_mut(|m| m.request_focus(find_id));
                            self.focus_find = false;
                        } else {
                            ui.ctx().request_repaint();
                        }
                    }
                    let focused = ui.memory(|m| m.has_focus(find_id));
                    let next = focused
                        && self
                            .settings
                            .keybindings
                            .consume(ui.ctx(), Action::FindNext);
                    let previous = focused
                        && self
                            .settings
                            .keybindings
                            .consume(ui.ctx(), Action::FindPrevious);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.find)
                            .id(find_id)
                            .return_key(None)
                            .frame(false)
                            .font(FontId::proportional(14.))
                            .hint_text("Find in source…")
                            .desired_width(240.),
                    );
                    theme::search_hint(ui, &self.settings.keybindings);
                    if next {
                        self.find_next();
                    }
                    if previous {
                        self.find_previous();
                    }
                    searched = next || previous;
                });
            });
            ui.add_space(12.);
        }
        let tab = &mut self.tabs[self.active];
        let id = egui::Id::new(("source", &tab.doc.path));
        let jump = tab.jump.take();
        let motion_jump = std::mem::take(&mut tab.motion_jump);
        if let Some(range) = &jump {
            let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(range.end),
                    egui::text::CCursor::new(range.start),
                )));
            state.store(ui.ctx(), id);
            self.focus_source = !searched;
            tab.cursor = range.start;
            tab.selection = range.clone();
        }
        let mut restore_focus = false;
        if self.focus_source {
            // Modal focus is released at the end of its last frame. Keep the
            // pending request until the source layer can actually receive it.
            if ui.memory(|m| m.allows_interaction(ui.layer_id())) && self.launcher.is_none() {
                ui.memory_mut(|m| m.request_focus(id));
                self.focus_source = false;
                restore_focus = true;
            } else {
                ui.ctx().request_repaint();
            }
        }
        let mut edited = false;
        let mut go_requested = false;
        let navigation_pointer = self.go_gesture.modified_pointer(ui.ctx());
        // Long lines wrap, so the source scrolls vertically only and can
        // never slide under the line numbers.
        egui::ScrollArea::vertical()
            .id_salt(("source-scroll", &tab.doc.path))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    let font = FontId::monospace(self.settings.editor.font_size);
                    let height = theme::line_height(font.size);
                    let (gutter, _) = ui.allocate_exact_size(
                        egui::vec2(
                            if self.settings.editor.line_numbers {
                                (tab.line_count.to_string().len() as f32 * font.size * 0.65 + 18.)
                                    .max(36.)
                            } else {
                                8.
                            },
                            height,
                        ),
                        egui::Sense::hover(),
                    );
                    let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, width| {
                        tab.language.layout(ui, text.as_str(), width)
                    };
                    let output = egui::TextEdit::multiline(&mut tab.doc.text)
                        .interactive(!navigation_pointer && self.launcher.is_none())
                        .id(id)
                        .font(font.clone())
                        .code_editor()
                        .frame(false)
                        .desired_width(f32::INFINITY)
                        .desired_rows(36)
                        .lock_focus(true)
                        .layouter(&mut layouter)
                        .show(ui);
                    // A logical line may span several wrapped rows. `tops` holds
                    // the screen y of each line's first row, plus the text bottom.
                    let mut tops = Vec::with_capacity(tab.line_count + 1);
                    let mut starts_line = true;
                    for row in &output.galley.rows {
                        if starts_line {
                            tops.push(output.galley_pos.y + row.pos.y);
                        }
                        starts_line = row.ends_with_newline;
                    }
                    let lines = tops.len();
                    tops.push(output.galley_pos.y + output.galley.size().y);
                    let clip = ui.clip_rect();
                    let first = tops[1..].partition_point(|bottom| *bottom <= clip.top());
                    let last = tops[..lines].partition_point(|top| *top < clip.bottom());
                    for line in (first..last).filter(|_| self.settings.editor.line_numbers) {
                        let job = egui::text::LayoutJob::single_section(
                            (line + 1).to_string(),
                            egui::TextFormat {
                                font_id: font.clone(),
                                color: theme::of(ui).muted,
                                line_height: Some(height),
                                ..Default::default()
                            },
                        );
                        let number = ui.fonts_mut(|f| f.layout_job(job));
                        ui.painter().galley(
                            egui::pos2(gutter.right() - 8. - number.size().x, tops[line]),
                            number,
                            theme::of(ui).muted,
                        );
                    }
                    self.git.gutter(ui, gutter.right(), &tops, first, last);
                    // A click in the file browser is outside this TextEdit.
                    // Honor the explicit open request after it processes that
                    // click, so the very next keystroke reaches the new buffer.
                    let response = output.response.interact(egui::Sense::click_and_drag());
                    if restore_focus || (navigation_pointer && response.contains_pointer()) {
                        response.request_focus();
                    }
                    if self.go_gesture.clicked(&response) {
                        // An existing selection wins. With only a caret, jump
                        // from the clicked source position instead.
                        if tab.selection.is_empty()
                            && let Some(pos) = response.interact_pointer_pos()
                        {
                            let cursor = output.galley.cursor_from_pos(pos - output.galley_pos);
                            tab.cursor = cursor.index;
                            tab.selection = cursor.index..cursor.index;
                        }
                        go_requested = true;
                    }
                    if let Some(range) = output.cursor_range {
                        let selection = range.as_sorted_char_range();
                        if tab.cursor != range.primary.index && !output.response.changed() {
                            self.completion = None;
                        }
                        tab.cursor = range.primary.index;
                        tab.selection = selection;
                        let rect = output
                            .galley
                            .pos_from_cursor(range.primary)
                            .translate(output.galley_pos.to_vec2());
                        self.caret = rect.left_bottom();
                        if jump.is_some() {
                            ui.scroll_to_rect(
                                rect.expand(6.),
                                if motion_jump {
                                    None
                                } else {
                                    Some(egui::Align::Center)
                                },
                            );
                        }
                    }
                    tab.language.paint_brackets(ui, &output);
                    if self.show_find && !output.response.has_focus() && !tab.selection.is_empty() {
                        let start = output
                            .galley
                            .pos_from_cursor(egui::text::CCursor::new(tab.selection.start));
                        let end = output
                            .galley
                            .pos_from_cursor(egui::text::CCursor::new(tab.selection.end));
                        let rect = egui::Rect::from_min_max(start.left_top(), end.right_bottom())
                            .translate(output.galley_pos.to_vec2());
                        ui.painter()
                            .rect_filled(rect, 2., theme::tint(theme::of(ui).accent, 40));
                        if searched {
                            ui.scroll_to_rect(rect.expand(6.), Some(egui::Align::Center));
                        }
                    }
                    edited = output.response.changed();
                    if edited {
                        tab.edited();
                        self.layers.edited();
                        self.nav_token += 1;
                        self.pending_go = None;
                    }
                });
            });
        if edited && self.settings.editor.auto_complete {
            self.complete();
        }
        go_requested
    }
    fn completion_ui(&mut self, ctx: &egui::Context) {
        let Some(c) = &self.completion else {
            return;
        };
        let mut accept = None;
        let pos = egui::pos2(
            c.anchor.x.min(ctx.content_rect().right() - 460.).max(8.),
            c.anchor.y.min(ctx.content_rect().bottom() - 250.) + 5.,
        );
        egui::Area::new("completion-menu".into())
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .show(ctx, |ui| {
                theme::popup(ui.visuals()).inner_margin(6.).show(ui, |ui| {
                    ui.set_width(420.);
                    ui.spacing_mut().item_spacing.y = 2.;
                    egui::ScrollArea::vertical()
                        .max_height(230.)
                        .show(ui, |ui| {
                            for (i, item) in c.suggestions.items.iter().enumerate() {
                                let detail =
                                    if ["Command", "Environment"].contains(&item.detail.as_str()) {
                                        ""
                                    } else {
                                        &item.detail
                                    };
                                let response =
                                    theme::row(ui, &item.insert, detail, "", i == c.selected);
                                if i == c.selected {
                                    response.scroll_to_me(None);
                                }
                                if response.clicked() {
                                    accept = Some(i);
                                }
                            }
                        });
                });
            });
        if let Some(index) = accept {
            self.accept_completion(index);
        }
    }
    fn layer_ui(&mut self, ctx: &egui::Context) {
        let go_requested = egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::palette(&ctx.style().visuals).canvas)
                    .inner_margin(egui::Margin::symmetric(24, 20)),
            )
            .show(ctx, |ui| {
                if self.layers.layer == Layer::Source {
                    self.source_ui(ui)
                } else {
                    self.preview.ui(ui)
                }
            })
            .inner;
        if go_requested {
            self.go(ctx);
            ctx.request_repaint();
        }
    }
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if self.pending_project.is_some() || self.project_popup.is_some() {
            self.preview.handle_fit_shortcuts(ctx, false);
            if self.settings.keybindings.consume(ctx, Action::Dismiss) {
                self.pending_project = None;
                self.project_popup = None;
                self.focus_source = self.layers.layer == Layer::Source;
            }
            return;
        }
        for action in [Action::Switcher, Action::Commands] {
            if self.settings.keybindings.consume(ctx, action) {
                self.perform_action(ctx, action);
            }
        }
        if self.launcher.is_some() {
            self.preview.handle_fit_shortcuts(ctx, false);
            return;
        }
        for action in Action::ALL {
            if action.is_command()
                && !action.is_pdf_fit()
                && !matches!(action, Action::Switcher | Action::Commands)
                && self.action_available(action)
                && (action.motion().is_none() || self.source_has_focus(ctx))
                && self.settings.keybindings.consume(ctx, action)
            {
                self.perform_action(ctx, action);
            }
        }
        self.preview.handle_fit_shortcuts(
            ctx,
            self.layers.layer == Layer::Preview
                && !self.close_dialog
                && !self.reload_dialog
                && !self.project_busy
                && !self.show_help
                && !self.show_git,
        );
        if self.completion.is_none() && self.settings.keybindings.consume(ctx, Action::Dismiss) {
            self.show_find = false;
            self.preview.hide_find();
            self.show_log = false;
            self.show_git = false;
            self.show_help = false;
            self.git.dismiss_detail();
            self.focus_source = self.layers.layer == Layer::Source;
        }
        if let Some(c) = &mut self.completion {
            if self
                .settings
                .keybindings
                .consume(ctx, Action::CompletionNext)
            {
                c.selected = (c.selected + 1) % c.suggestions.items.len();
            }
            if self
                .settings
                .keybindings
                .consume(ctx, Action::CompletionPrevious)
            {
                c.selected =
                    (c.selected + c.suggestions.items.len() - 1) % c.suggestions.items.len();
            }
            let accept = self
                .settings
                .keybindings
                .consume(ctx, Action::CompletionAccept);
            if accept {
                let index = c.selected;
                self.accept_completion(index);
            }
            if self
                .settings
                .keybindings
                .consume(ctx, Action::CompletionDismiss)
            {
                self.completion = None;
            }
        }
    }
    fn source_has_focus(&self, ctx: &egui::Context) -> bool {
        !self.reload_dialog
            && !self.close_dialog
            && !self.project_busy
            && self.tabs.get(self.active).is_some_and(|tab| {
                ctx.memory(|m| m.has_focus(egui::Id::new(("source", &tab.doc.path))))
            })
    }
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let update_start = Instant::now();
        self.poll(ctx);
        if self.layers.layer == Layer::Source
            && let Some(tab) = self.tabs.get(self.active)
        {
            self.git.tick(ctx, &tab.doc.path, &tab.doc.text);
        }
        if ctx.input(|i| i.viewport().close_requested())
            && !self.allow_close
            && self.tabs.iter().any(|t| t.doc.is_dirty())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_dialog = true;
        }
        self.handle_shortcuts(ctx);
        if self.last_scan.elapsed() > Duration::from_secs(2) {
            self.request_project_scan(ctx);
        }
        // Publish external edits when their background scan arrives. Preview
        // buffers were saved on entry, so this path need not scan/save again.
        if self.layers.layer == Layer::Preview
            && !self.building
            && !self.exporting
            && !self.scanning
            && self.layers.can_auto_build()
            && self.layers.enter_preview()
        {
            self.start_build(ctx);
        }
        ctx.request_repaint_after(Duration::from_secs(2));
        if self.show_log {
            egui::TopBottomPanel::bottom("build-log")
                .resizable(true)
                .default_height(150.)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.strong("Typesetting");
                        if ui.small_button("×").clicked() {
                            self.show_log = false;
                        }
                    });
                    let mut target = None;
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for d in &self.diagnostics {
                            if ui
                                .selectable_label(
                                    false,
                                    format!("{}:{}  {}", d.path.display(), d.line, d.message),
                                )
                                .clicked()
                            {
                                target = Some((d.path.clone(), d.line));
                            }
                        }
                        ui.collapsing("Compiler output", |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.log.as_str())
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(f32::INFINITY),
                            );
                        });
                    });
                    if let Some((mut path, line)) = target {
                        if path.is_relative()
                            && let Some(root) = &self.root
                        {
                            path = root.parent().unwrap().join(path);
                        }
                        self.jump_source(path, line);
                    }
                });
        }
        self.workspace_ui(ctx);
        self.project_ui(ctx);
        if self.layers.layer == Layer::Source {
            self.completion_ui(ctx);
            self.git.detail(ctx);
        }
        if self.reload_dialog {
            egui::Window::new("Reload from disk?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label("Replace this buffer with the saved file?");
                    ui.horizontal(|ui| {
                        if ui.button("Reload").clicked() {
                            if let Some(tab) = self.tabs.get_mut(self.active) {
                                match Document::open(&tab.doc.path) {
                                    Ok(doc) => {
                                        tab.doc = doc;
                                        tab.edited();
                                        tab.jump = Some(0..0);
                                        self.layers.edited();
                                    }
                                    Err(e) => self.status = e.to_string(),
                                }
                            }
                            self.completion = None;
                            self.reload_dialog = false;
                        }
                        if ui.button("Keep editing").clicked() {
                            self.reload_dialog = false;
                        }
                    });
                });
        }
        if self.close_dialog {
            egui::Window::new("Save your changes?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label("Your project has unsaved edits.");
                    ui.horizontal(|ui| {
                        if ui.button("Save and quit").clicked() && self.save_all() {
                            self.allow_close = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if ui.button("Discard and quit").clicked() {
                            self.allow_close = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if ui.button("Keep editing").clicked() {
                            self.close_dialog = false;
                        }
                    });
                });
        }
        if self.show_help {
            egui::Window::new("Shortcuts")
                .open(&mut self.show_help)
                .default_width(520.)
                .vscroll(true)
                .show(ctx, |ui| {
                    // Editor font preferences must not enlarge the help panel.
                    ui.style_mut().text_styles.insert(
                        egui::TextStyle::Monospace, FontId::monospace(14.),
                    );
                    if let Some(gesture) = &self.settings.mouse.go {
                        ui.horizontal_wrapped(|ui| {
                            ui.monospace(&gesture.label);
                            ui.label("Go to selection in other layer");
                        });
                    }
                    for action in Action::ALL {
                        let key = self.settings.keybindings.hint(action);
                        if key.is_empty() { continue; }
                        let action = action.label();
                        ui.horizontal_wrapped(|ui| {
                            ui.monospace(key);
                            ui.label(action);
                        });
                    }
                    ui.separator();
                    if let Some(path) = &self.config_path {
                        ui.label(format!("Settings: {}", path.display()));
                    } else {
                        ui.label("Using bundled defaults. Place folio.toml beside the executable or in ~/.config/folio.");
                    }
                    ui.weak("Edit folio.toml and restart to apply changes.");
                });
        }
        if let Some(error) = self.config_error.clone() {
            egui::Window::new("Settings could not be loaded")
                .collapsible(false)
                .resizable(true)
                .show(ctx, |ui| {
                    ui.label(error);
                    ui.label("Folio is using its default settings. Correct the file and restart.");
                    if ui.button("Continue").clicked() {
                        self.config_error = None;
                    }
                });
        }
        if let Some(trace) = &mut self.trace {
            trace.record(
                ctx,
                self.layers.layer == Layer::Preview && self.preview.path.is_some(),
                update_start,
                frame.info().cpu_usage,
            );
        }
    }
}
impl Drop for App {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(worker) = self.build_thread.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_configuration_files_open_with_their_language_and_no_tex_completion() {
        let dir = std::env::temp_dir().join(format!("folio config syntax {}", std::process::id()));
        ManagedProject::create(&dir, "Syntax paper", Engine::PdfLatex).unwrap();
        fs::create_dir_all(dir.join("data")).unwrap();
        let ctx = egui::Context::default();
        let mut app = App::with_context(ctx.clone(), Some(dir.clone()));
        for (name, source, string) in [
            (
                "data/options.toml",
                "name = 'Folio'\nmacro = '\\sec'",
                "'Folio'",
            ),
            (
                "data/options.json",
                "{\"name\": \"Folio\", \"macro\": \"\\\\sec\"}",
                "\"Folio\"",
            ),
        ] {
            let path = dir.join(name);
            fs::write(&path, source).unwrap();
            app.open_document(path.clone());
            let output = ctx.run(egui::RawInput::default(), |ctx| app.workspace_ui(ctx));
            let galley = output
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Text(text) if text.galley.job.text == source => Some(&text.galley),
                    _ => None,
                })
                .unwrap();
            let byte = source.find(string).unwrap();
            assert_eq!(
                galley
                    .job
                    .sections
                    .iter()
                    .find(|s| s.byte_range.contains(&byte))
                    .unwrap()
                    .format
                    .color,
                theme::DARK.string
            );
            let tab = &mut app.tabs[app.active];
            tab.cursor = source[..source.find("sec").unwrap() + 3].chars().count();
            app.complete();
            assert!(
                app.completion.is_none(),
                "{name} should not offer LaTeX commands"
            );
            let snapshot = crate::project::scan(app.root.as_ref().unwrap(), &[]);
            assert!(
                snapshot.files.contains(&path.canonicalize().unwrap()),
                "the surface switcher must include JSON and TOML"
            );
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn bracket_highlights_follow_unicode_glyphs_and_disappear_for_selections() {
        let dir = std::env::temp_dir().join(format!("folio bracket paint {}", std::process::id()));
        ManagedProject::create(&dir, "Bracket paper", Engine::PdfLatex).unwrap();
        let source = "α \\textbf{β}";
        fs::write(dir.join("main.tex"), source).unwrap();
        let ctx = egui::Context::default();
        let mut app = App::with_context(ctx.clone(), Some(dir.clone()));
        let open = source.chars().position(|c| c == '{').unwrap();
        let close = source.chars().count() - 1;
        for selected in [false, true, false] {
            app.tabs[app.active].jump = Some(if selected {
                open..close + 1
            } else {
                open..open
            });
            let output = ctx.run(egui::RawInput::default(), |ctx| app.workspace_ui(ctx));
            let highlights: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Rect(r) if r.stroke.color == theme::DARK.accent => Some(r.rect),
                    _ => None,
                })
                .collect();
            if selected {
                assert!(highlights.is_empty());
                continue;
            }
            assert_eq!(
                highlights.len(),
                2,
                "both matching brackets must be visible"
            );
            let text = output
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.job.text == source => Some(t),
                    _ => None,
                })
                .unwrap();
            for (highlight, index) in highlights.iter().zip([open, close]) {
                let from = text.galley.pos_from_cursor(egui::text::CCursor::new(index));
                let to = text
                    .galley
                    .pos_from_cursor(egui::text::CCursor::new(index + 1));
                let center = egui::pos2((from.left() + to.left()) / 2., from.center().y)
                    + text.pos.to_vec2();
                assert!(
                    highlight.contains(center),
                    "highlight {highlight:?} missed bracket {index} at {center:?}"
                );
                assert!(highlight.width() < app.settings.editor.font_size * 1.5);
            }
        }
        fs::remove_dir_all(dir).unwrap();
    }

    fn painted_pdf_page(
        app: &mut App,
        ctx: &egui::Context,
        size: egui::Vec2,
    ) -> (egui::Rect, egui::Rect) {
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            },
            |ctx| app.workspace_ui(ctx),
        );
        let (page, _) = output
            .shapes
            .into_iter()
            .filter_map(|shape| match shape.shape {
                egui::Shape::Rect(rect) if rect.fill == egui::Color32::WHITE => {
                    Some((rect.rect, shape.clip_rect))
                }
                _ => None,
            })
            .max_by(|(a, ac), (b, bc)| a.intersect(*ac).area().total_cmp(&b.intersect(*bc).area()))
            .expect("the cached PDF must paint immediately");
        (page, app.preview.canvas)
    }

    #[test]
    fn switching_to_pdf_fits_ninety_percent_of_the_remaining_width() {
        let ctx = egui::Context::default();
        let mut app = App::with_context(ctx.clone(), None);
        // A cached project/preview avoids unrelated file IO and compilation.
        app.root = Some(PathBuf::from("paper/main.tex"));
        app.layers.built = Some(app.layers.revision);
        let size = egui::vec2(1000., 760.);
        for page_size in [(600., 800.), (900., 600.)] {
            app.preview.set_test_pages(vec![page_size; 3]);
            for browser in [false, true] {
                app.source();
                app.browser.open = browser;
                ctx.data_mut(|data| {
                    data.insert_persisted(
                        egui::Id::new("project-files"),
                        egui::containers::panel::PanelState {
                            rect: egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(420., 760.),
                            ),
                        },
                    )
                });
                app.perform_action(&ctx, Action::Preview);
                let (page, clip) = painted_pdf_page(&mut app, &ctx, size);
                assert!(
                    (page.width() / clip.width() - 0.9).abs() < 0.001,
                    "page {page:?} must use 90% of canvas {clip:?}; browser={browser}"
                );
                assert!((page.center().x - clip.center().x).abs() < 1.);
                assert!((page.width() / page.height() - page_size.0 / page_size.1).abs() < 0.001);
                if browser {
                    assert!(
                        page.left() > 420.,
                        "the browser must be excluded from the PDF canvas"
                    );
                }
                assert!(!app.building, "fitting must reuse the cached PDF");

                app.preview.command(&ctx, Action::PdfZoomIn);
                let (zoomed, _) = painted_pdf_page(&mut app, &ctx, size);
                assert!((zoomed.width() / page.width() - 1.2).abs() < 0.001);
                app.enter_preview(&ctx, false);
                assert_eq!(
                    painted_pdf_page(&mut app, &ctx, size).0.size(),
                    zoomed.size(),
                    "remaining in PDF must keep manual zoom"
                );
                app.source();
                app.enter_preview(&ctx, false);
                assert_eq!(
                    painted_pdf_page(&mut app, &ctx, size).0.size(),
                    page.size(),
                    "returning from source must refit after manual zoom"
                );
            }
        }
    }

    #[test]
    fn fitted_pdf_tracks_browser_and_window_resizes_without_losing_its_page() {
        let ctx = egui::Context::default();
        let mut app = App::with_context(ctx.clone(), None);
        app.root = Some(PathBuf::from("paper/main.tex"));
        app.layers.built = Some(app.layers.revision);
        app.preview.set_test_pages(vec![(900., 600.); 3]);
        app.enter_preview(&ctx, false);
        let wide = painted_pdf_page(&mut app, &ctx, egui::vec2(1200., 850.)).0;
        app.preview.command(&ctx, Action::PdfNext);
        painted_pdf_page(&mut app, &ctx, egui::vec2(1200., 850.));
        for (browser, size) in [
            (false, egui::vec2(1200., 600.)),
            (true, egui::vec2(1000., 760.)),
            (true, egui::vec2(1000., 600.)),
            (false, egui::vec2(1000., 600.)),
        ] {
            app.browser.open = browser;
            let (page, clip) = painted_pdf_page(&mut app, &ctx, size);
            assert!(
                (page.width() / clip.width() - 0.9).abs() < 0.001,
                "after resizing: page {page:?}, canvas {clip:?}"
            );
            assert!(
                page.width() <= wide.width(),
                "fitting must respond to the smaller canvas"
            );
            assert_eq!(app.preview.page, 2);
        }
    }

    #[test]
    fn palette_find_transfers_typing_to_search_in_each_surface() {
        let dir = std::env::temp_dir().join(format!("folio palette find {}", std::process::id()));
        ManagedProject::create(&dir, "Test paper", Engine::PdfLatex).unwrap();
        let path = dir.join("main.tex");
        fs::write(&path, SAMPLE).unwrap();
        for layer in [Layer::Source, Layer::Preview] {
            let ctx = egui::Context::default();
            let mut app = App::with_context(ctx.clone(), Some(dir.clone()));
            app.layers.layer = layer;
            let frame = |app: &mut App, events| {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(900., 700.),
                        )),
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
            frame(
                &mut app,
                key(
                    egui::Key::P,
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                ),
            );
            frame(&mut app, vec![egui::Event::Text("Find".into())]);
            frame(&mut app, key(egui::Key::Enter, egui::Modifiers::NONE));
            frame(&mut app, vec![egui::Event::Text("needle".into())]);
            assert_eq!(
                app.tabs[0].doc.text, SAMPLE,
                "search text must never enter the source after choosing Find"
            );
            let query = match layer {
                Layer::Source => &app.find,
                Layer::Preview => &app.preview.query,
            };
            assert_eq!(query, "needle", "Find must accept typing in {layer:?}");
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn launcher_owns_typing_and_uses_configured_accept_and_dismiss() {
        let dir = std::env::temp_dir().join(format!("folio launcher {}", std::process::id()));
        ManagedProject::create(&dir, "Test paper", Engine::PdfLatex).unwrap();
        let main = dir.join("main.tex");
        let chapter = dir.join("sections/chapter.tex");
        fs::write(&main, SAMPLE).unwrap();
        fs::write(&chapter, "% !TeX root = main.tex\nChapter\n").unwrap();
        let ctx = egui::Context::default();
        let settings =
            Settings::parse("[keybindings]\nlauncher_accept=['F11']\nlauncher_dismiss=['F12']")
                .unwrap();
        let mut app = App::with_settings(ctx.clone(), Some(dir.clone()), Arc::new(settings));
        settle(&mut app, &ctx, |a| !a.scanning);
        app.tabs[0].jump = Some(0..0);
        let frame = |app: &mut App, events| {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900., 700.),
                    )),
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
        frame(&mut app, key(egui::Key::P, egui::Modifiers::COMMAND));
        frame(&mut app, vec![egui::Event::Text("chapter".into())]);
        assert_eq!(app.launcher.as_ref().unwrap().query, "chapter");
        assert_eq!(
            app.tabs[0].doc.text, SAMPLE,
            "launcher query must never enter the document"
        );
        frame(&mut app, key(egui::Key::Enter, egui::Modifiers::NONE));
        assert!(
            app.launcher.is_some(),
            "the replaced accept binding is inactive"
        );
        frame(&mut app, key(egui::Key::F11, egui::Modifiers::NONE));
        assert!(app.launcher.is_none());
        assert_eq!(
            app.tabs[app.active].doc.path,
            chapter.canonicalize().unwrap()
        );
        frame(&mut app, key(egui::Key::P, egui::Modifiers::COMMAND));
        frame(&mut app, vec![egui::Event::Text("no such file".into())]);
        frame(&mut app, key(egui::Key::F11, egui::Modifiers::NONE));
        assert!(
            app.launcher.is_some(),
            "empty results must not activate an old selection"
        );
        frame(&mut app, key(egui::Key::F12, egui::Modifiers::NONE));
        assert!(app.launcher.is_none());
        frame(&mut app, vec![]);
        frame(&mut app, vec![egui::Event::Text("resumed".into())]);
        assert!(
            app.tabs[app.active].doc.text.contains("resumed"),
            "dismissal must restore editor focus"
        );
        assert_eq!(app.tabs[0].doc.text, SAMPLE);
        drop(app);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn line_numbers_share_the_source_baseline_with_looser_line_spacing() {
        let dir = std::env::temp_dir().join(format!("folio-baselines-{}", std::process::id()));
        ManagedProject::create(&dir, "Paper", Engine::PdfLatex).unwrap();
        let text = "First line\nSecond line\nThird line";
        fs::write(dir.join("main.tex"), text).unwrap();
        let ctx = egui::Context::default();
        let mut app = App::with_context(ctx.clone(), Some(dir.clone()));
        let mut shapes = Vec::new();
        for _ in 0..2 {
            shapes = ctx
                .run(egui::RawInput::default(), |ctx| app.workspace_ui(ctx))
                .shapes;
        }
        let texts: Vec<_> = shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Text(t) => Some(t),
                _ => None,
            })
            .collect();
        let source = texts.iter().find(|t| t.galley.job.text == text).unwrap();
        for line in 0..3 {
            let number = texts
                .iter()
                .find(|t| t.galley.job.text == (line + 1).to_string())
                .unwrap();
            let source_y = source.pos.y
                + source.galley.rows[line].pos.y
                + source.galley.rows[line].glyphs[0].pos.y;
            let number_y =
                number.pos.y + number.galley.rows[0].pos.y + number.galley.rows[0].glyphs[0].pos.y;
            assert!(
                (source_y - number_y).abs() < 0.5,
                "line {}: source {source_y}, gutter {number_y}",
                line + 1
            );
        }
        drop(app);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn long_source_lines_wrap_beside_a_pinned_gutter_that_numbers_logical_lines() {
        let dir = std::env::temp_dir().join(format!("folio-soft-wrap-{}", std::process::id()));
        ManagedProject::create(&dir, "Paper", Engine::PdfLatex).unwrap();
        let text = format!("Short line\n{}\nLast line", "wrapped words ".repeat(60));
        fs::write(dir.join("main.tex"), &text).unwrap();
        let ctx = egui::Context::default();
        let mut app = App::with_context(ctx.clone(), Some(dir.clone()));
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 700.));
        let mut shapes = Vec::new();
        for _ in 0..3 {
            shapes = ctx
                .run(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        ..Default::default()
                    },
                    |ctx| app.workspace_ui(ctx),
                )
                .shapes;
        }
        let texts: Vec<_> = shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Text(t) => Some(t),
                _ => None,
            })
            .collect();
        let source = texts.iter().find(|t| t.galley.job.text == text).unwrap();
        let rows = &source.galley.rows;
        assert!(rows.len() > 3, "the long line must wrap into several rows");
        assert!(
            source.pos.x + source.galley.size().x <= screen.right(),
            "wrapped text stays inside the window"
        );
        let number = |n: usize| {
            texts
                .iter()
                .find(|t| t.galley.job.text == n.to_string())
                .unwrap_or_else(|| panic!("line number {n} is missing"))
        };
        assert!(
            !texts
                .iter()
                .any(|t| t.galley.job.text == "4" && t.pos.x < source.pos.x),
            "wrapped rows must not be numbered as extra lines"
        );
        // Line 3 starts on the row after the wrapped line's last row.
        let third = rows.iter().rposition(|r| r.ends_with_newline).unwrap() + 1;
        for (n, row) in [(1, 0), (2, 1), (3, third)] {
            let label = number(n);
            assert!(
                (label.pos.y - (source.pos.y + rows[row].pos.y)).abs() < 0.5,
                "line {n} must sit beside its first row"
            );
            assert!(
                label.pos.x + label.galley.size().x <= source.pos.x,
                "line {n}: text must never be painted under the gutter"
            );
        }
        drop(app);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn writing_surface_keeps_line_numbers_without_action_buttons() {
        let dir = std::env::temp_dir().join(format!("folio-quiet-ui-{}", std::process::id()));
        ManagedProject::create(&dir, "Quiet paper", Engine::PdfLatex).unwrap();
        let ctx = egui::Context::default();
        let mut app = App::with_context(ctx.clone(), Some(dir.clone()));
        settle(&mut app, &ctx, |a| !a.scanning);
        fn collect(shape: &egui::Shape, labels: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(s) => labels.push(s.galley.job.text.clone()),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, labels)),
                _ => {}
            }
        }
        for browser in [false, true] {
            app.browser.open = browser;
            let mut labels = Vec::new();
            for _ in 0..2 {
                labels.clear();
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1000., 700.),
                        )),
                        ..Default::default()
                    },
                    |ctx| app.workspace_ui(ctx),
                );
                for shape in output.shapes {
                    collect(&shape.shape, &mut labels);
                }
            }
            assert!(
                labels.iter().any(|s| s == "1"),
                "line numbers must remain visible"
            );
            for control in ["Files", "+", "×", "Build", "Compile"] {
                assert!(
                    !labels.iter().any(|s| s == control),
                    "permanent {control} control is still visible"
                );
            }
            if browser {
                assert!(labels.iter().any(|s| s.contains("introduction.tex")));
            }
        }
        drop(app);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn focused_workspace_has_no_permanent_app_chrome() {
        let ctx = egui::Context::default();
        let mut app = App::with_context(ctx.clone(), None);
        let mut texts = Vec::new();
        fn collect(shape: &egui::epaint::Shape, texts: &mut Vec<String>) {
            match shape {
                egui::epaint::Shape::Text(s) => texts.push(s.galley.job.text.clone()),
                egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, texts)),
                _ => {}
            }
        }
        for _ in 0..2 {
            texts.clear();
            let output = ctx.run(egui::RawInput::default(), |ctx| app.workspace_ui(ctx));
            for shape in &output.shapes {
                collect(&shape.shape, &mut texts);
            }
        }
        for removed in ["Source", "Preview", "···", "Git"] {
            assert!(
                !texts.iter().any(|text| text == removed),
                "permanent {removed} control is still visible"
            );
        }
        assert!(
            texts.iter().any(|t| t.contains("Switch")),
            "empty workspace must expose the launcher"
        );
    }

    #[test]
    fn keyboard_cycles_source_buffers_and_cached_pdf_without_syncing_selection() {
        let dir = std::env::temp_dir().join(format!("folio surfaces {}", std::process::id()));
        ManagedProject::create(&dir, "Test paper", Engine::PdfLatex).unwrap();
        let a = dir.join("main.tex");
        let b = dir.join("sections/chapter.tex");
        fs::write(&a, SAMPLE).unwrap();
        fs::write(&b, "% !TeX root = main.tex\nSecond source\n").unwrap();
        let ctx = egui::Context::default();
        let mut app = App::with_context(ctx.clone(), Some(dir.clone()));
        app.open_document(b);
        app.active = 0;
        app.tabs[0].selection = 2..7;
        app.layers.built = Some(app.layers.revision);
        let press = |app: &mut App, key, modifiers| {
            let _ = ctx.run(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key,
                        modifiers,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                    }],
                    ..Default::default()
                },
                |ctx| app.handle_shortcuts(ctx),
            );
        };
        press(&mut app, egui::Key::Tab, egui::Modifiers::CTRL);
        assert_eq!(app.active, 1);
        assert_eq!(app.layers.layer, Layer::Source);
        press(&mut app, egui::Key::Tab, egui::Modifiers::CTRL);
        assert_eq!(app.layers.layer, Layer::Preview);
        assert!(
            app.pending_go.is_none(),
            "switching does not run SyncTeX Go"
        );
        assert!(!app.building, "unchanged preview must be reused");
        press(&mut app, egui::Key::Tab, egui::Modifiers::CTRL);
        assert_eq!(app.active, 0);
        assert_eq!(app.layers.layer, Layer::Source);
        assert_eq!(app.tabs[0].selection, 2..7);
        press(&mut app, egui::Key::Num2, egui::Modifiers::COMMAND);
        assert_eq!(app.layers.layer, Layer::Preview);
        press(&mut app, egui::Key::Num1, egui::Modifiers::COMMAND);
        assert_eq!(app.layers.layer, Layer::Source);
        assert_eq!(app.active, 0);
        drop(app);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn search_can_move_backwards_with_a_configured_key_and_without_buttons() {
        let dir =
            std::env::temp_dir().join(format!("folio-search-previous-{}", std::process::id()));
        ManagedProject::create(&dir, "Search paper", Engine::PdfLatex).unwrap();
        fs::write(dir.join("main.tex"), "alpha then alpha").unwrap();
        let ctx = egui::Context::default();
        let settings = Settings::parse("[keybindings]\nfind_previous=['F10']").unwrap();
        let mut app = App::with_settings(ctx.clone(), Some(dir.clone()), Arc::new(settings));
        let frame = |app: &mut App, events| {
            ctx.run(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ctx| {
                    app.handle_shortcuts(ctx);
                    app.workspace_ui(ctx);
                },
            )
        };
        app.tabs[0].jump = Some(11..16);
        frame(&mut app, vec![]);
        app.find = "alpha".into();
        app.show_find = true;
        app.focus_find = true;
        frame(&mut app, vec![]);
        frame(
            &mut app,
            vec![egui::Event::Key {
                key: egui::Key::F10,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(app.tabs[0].selection, 0..5);
        frame(
            &mut app,
            vec![egui::Event::Key {
                key: egui::Key::F10,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(
            app.tabs[0].selection,
            11..16,
            "search must keep focus for repeated navigation"
        );
        assert!(!app.tabs[0].doc.is_dirty());
        drop(app);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn search_next_uses_configured_key_only_in_the_focused_search_field() {
        let dir = std::env::temp_dir().join(format!("folio search keys {}", std::process::id()));
        ManagedProject::create(&dir, "Test paper", Engine::PdfLatex).unwrap();
        let path = dir.join("main.tex");
        fs::write(&path, "start alpha end alpha").unwrap();
        let ctx = egui::Context::default();
        let settings = Settings::parse("[keybindings]\nfind_next=['F9']").unwrap();
        let mut app = App::with_settings(ctx.clone(), Some(dir.clone()), Arc::new(settings));
        app.tabs[0].jump = Some(0..0);
        let frame = |app: &mut App, events| {
            let _ = ctx.run(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ctx| app.layer_ui(ctx),
            );
        };
        let key = || {
            vec![egui::Event::Key {
                key: egui::Key::F9,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]
        };
        frame(&mut app, vec![]);
        app.show_find = true;
        app.find = "alpha".into();
        // Visible search field alone must not swallow editor input.
        frame(&mut app, key());
        assert_eq!(app.tabs[0].selection, 0..0);
        app.focus_find = true;
        frame(&mut app, vec![]);
        frame(&mut app, key());
        assert_eq!(app.tabs[0].selection, 6..11);
        assert!(!app.tabs[0].doc.is_dirty());
        drop(app);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn completion_tab_cycles_and_enter_accepts_without_editing_or_losing_focus() {
        let dir = std::env::temp_dir().join(format!("folio completion tab {}", std::process::id()));
        ManagedProject::create(&dir, "Test paper", Engine::PdfLatex).unwrap();
        fs::write(dir.join("main.tex"), "\\s").unwrap();
        // The same input regression can verify a packaged user's settings,
        // which override bundled defaults and survive release packaging.
        let settings = std::env::var_os("FOLIO_TEST_COMPLETION_CONFIG")
            .map(|path| Settings::parse(&fs::read_to_string(path).unwrap()).unwrap())
            .unwrap_or_default();
        let ctx = egui::Context::default();
        let mut app = App::with_settings(ctx.clone(), Some(dir.clone()), Arc::new(settings));
        app.tabs[0].jump = Some(2..2);
        let frame = |app: &mut App, key: Option<(egui::Key, egui::Modifiers)>| {
            let _ = ctx.run(
                egui::RawInput {
                    modifiers: key.map_or(egui::Modifiers::NONE, |(_, modifiers)| modifiers),
                    events: key
                        .into_iter()
                        .map(|(key, modifiers)| egui::Event::Key {
                            key,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers,
                        })
                        .collect(),
                    ..Default::default()
                },
                |ctx| {
                    app.handle_shortcuts(ctx);
                    app.workspace_ui(ctx);
                    app.completion_ui(ctx);
                },
            );
        };
        frame(&mut app, None);
        app.complete();
        // Show the menu before navigating it. This also lets egui record the
        // editor's focus lock after the initial project-open focus transfer.
        frame(&mut app, None);
        let count = app.completion.as_ref().unwrap().suggestions.items.len();
        assert!(count > 1);
        for (key, modifiers, selected) in [
            (egui::Key::Tab, egui::Modifiers::NONE, 1),
            (egui::Key::Tab, egui::Modifiers::SHIFT, 0),
            (egui::Key::Tab, egui::Modifiers::SHIFT, count - 1),
            (egui::Key::Tab, egui::Modifiers::NONE, 0),
            (egui::Key::ArrowDown, egui::Modifiers::NONE, 1),
        ] {
            frame(&mut app, Some((key, modifiers)));
            assert_eq!(
                app.completion.as_ref().map(|c| c.selected),
                Some(selected),
                "{modifiers:?}+{key:?} must cycle, not accept"
            );
            assert_eq!(
                app.tabs[0].doc.text, "\\s",
                "navigation must not indent the source"
            );
            let source = egui::Id::new(("source", &app.tabs[0].doc.path));
            assert!(
                ctx.memory(|m| m.has_focus(source)),
                "navigation must keep editor focus"
            );
        }
        let completion = app.completion.as_ref().unwrap();
        let mut expected = app.tabs[0].doc.text.clone();
        expected.replace_range(
            completion.suggestions.range.clone(),
            &completion.suggestions.items[completion.selected].insert,
        );
        frame(&mut app, Some((egui::Key::Enter, egui::Modifiers::NONE)));
        assert!(app.completion.is_none());
        assert_eq!(
            app.tabs[0].doc.text, expected,
            "accepting must not also insert a newline"
        );
        drop(app);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn disabled_toolkit_zoom_stays_disabled_and_completion_uses_custom_keys() {
        let dir =
            std::env::temp_dir().join(format!("folio completion keys {}", std::process::id()));
        ManagedProject::create(&dir, "Test paper", Engine::PdfLatex).unwrap();
        let path = dir.join("main.tex");
        fs::write(&path, SAMPLE).unwrap();
        let ctx = egui::Context::default();
        let settings = Settings::parse("[keybindings]\nui_zoom_in=[]\ncomplete=['F1']\ncompletion_next=['F2']\ncompletion_accept=['F3']\ncompletion_dismiss=['F4']\n[editor]\nfont_size=21").unwrap();
        let mut app = App::with_settings(ctx.clone(), Some(dir.clone()), Arc::new(settings));
        app.tabs[0].doc.text = "\\s".into();
        app.tabs[0].cursor = 2;
        let press = |app: &mut App, key, modifiers| {
            let _ = ctx.run(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers,
                    }],
                    ..Default::default()
                },
                |ctx| app.handle_shortcuts(ctx),
            );
        };
        press(&mut app, egui::Key::Plus, egui::Modifiers::COMMAND);
        press(&mut app, egui::Key::F1, egui::Modifiers::NONE);
        assert_eq!(ctx.zoom_factor(), 1.);
        assert_eq!(egui::TextStyle::Monospace.resolve(&ctx.style()).size, 21.);
        assert!(app.completion.as_ref().unwrap().suggestions.items.len() > 1);
        press(&mut app, egui::Key::ArrowDown, egui::Modifiers::NONE);
        assert_eq!(app.completion.as_ref().unwrap().selected, 0);
        press(&mut app, egui::Key::F2, egui::Modifiers::NONE);
        assert_eq!(app.completion.as_ref().unwrap().selected, 1);
        let replacement = app.completion.as_ref().unwrap().suggestions.items[1]
            .insert
            .clone();
        press(&mut app, egui::Key::F3, egui::Modifiers::NONE);
        assert!(app.completion.is_none());
        assert!(app.tabs[0].doc.text.contains(&replacement));
        app.tabs[0].doc.text = "\\s".into();
        app.tabs[0].cursor = 2;
        press(&mut app, egui::Key::F1, egui::Modifiers::NONE);
        press(&mut app, egui::Key::F4, egui::Modifiers::NONE);
        assert!(app.completion.is_none());
        drop(app);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn configured_shortcuts_drive_the_app_and_replace_defaults() {
        let dir =
            std::env::temp_dir().join(format!("folio configured keys {}", std::process::id()));
        ManagedProject::create(&dir, "Test paper", Engine::PdfLatex).unwrap();
        let path = dir.join("main.tex");
        fs::write(&path, SAMPLE).unwrap();
        let ctx = egui::Context::default();
        let settings =
            Settings::parse("[keybindings]\nsave=['F6']\nfind=['F8']\n[build]\nengine='xelatex'")
                .unwrap();
        let mut app = App::with_settings(ctx.clone(), Some(dir.clone()), Arc::new(settings));
        app.tabs[0].doc.text.push_str("\n% custom save shortcut\n");
        let press = |app: &mut App, key, modifiers| {
            let _ = ctx.run(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers,
                    }],
                    ..Default::default()
                },
                |ctx| app.handle_shortcuts(ctx),
            );
        };
        press(&mut app, egui::Key::S, egui::Modifiers::COMMAND);
        assert!(
            app.tabs[0].doc.is_dirty(),
            "replaced default must no longer save"
        );
        press(&mut app, egui::Key::F6, egui::Modifiers::NONE);
        assert!(!app.tabs[0].doc.is_dirty());
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("custom save shortcut")
        );
        press(&mut app, egui::Key::F8, egui::Modifiers::NONE);
        assert!(app.show_find && app.focus_find);
        assert_eq!(
            app.managed.as_ref().unwrap().manifest.engine,
            Engine::PdfLatex,
            "application defaults must not override the project manifest"
        );
        drop(app);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn navigation_click_keeps_the_existing_source_selection() {
        for (config, modifiers) in [
            ("", egui::Modifiers::COMMAND),
            ("[mouse]\ngo='Alt+DoubleClick'", egui::Modifiers::ALT),
        ] {
            let dir =
                std::env::temp_dir().join(format!("folio source gesture {}", std::process::id()));
            ManagedProject::create(&dir, "Test paper", Engine::PdfLatex).unwrap();
            let path = dir.join("main.tex");
            let text = "\\documentclass{article}\n\\begin{document}\nAlpha chosen text\nOther paragraph\n\\end{document}\n";
            fs::write(&path, text).unwrap();
            let ctx = egui::Context::default();
            let mut app = App::with_settings(
                ctx.clone(),
                Some(dir.clone()),
                Arc::new(Settings::parse(config).unwrap()),
            );
            let selection = document::line_char_range(text, 3);
            app.tabs[0].jump = Some(selection.clone());
            app.layers.built = Some(app.layers.revision);
            let frame = |app: &mut App, events: Vec<egui::Event>| {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(800., 700.),
                        )),
                        modifiers,
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        app.layer_ui(ctx);
                    },
                );
            };
            frame(&mut app, vec![]);
            let pos = egui::pos2(200., 90.);
            frame(&mut app, vec![egui::Event::PointerMoved(pos)]);
            for pressed in [true, false] {
                frame(
                    &mut app,
                    vec![egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers,
                    }],
                );
            }
            assert_eq!(
                app.tabs[0].selection, selection,
                "the first Cmd-click must preserve the selected source range"
            );
            assert_eq!(
                app.layers.layer,
                Layer::Source,
                "one modified click only arms the gesture"
            );
            for pressed in [true, false] {
                frame(
                    &mut app,
                    vec![egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers,
                    }],
                );
            }
            assert_eq!(app.layers.layer, Layer::Preview);
            assert_eq!(
                app.pending_go.as_ref().unwrap().2,
                3,
                "navigate from the original selection"
            );
            assert_eq!(
                app.pending_go.as_ref().unwrap().3,
                0,
                "use SyncTeX's unknown-column sentinel for line navigation, as the CLI does"
            );
            assert!(!app.building, "reuse the cached preview");
            drop(app);
            fs::remove_dir_all(dir).unwrap();
        }
    }
    #[test]
    fn inverse_navigation_selects_the_exact_word_in_the_open_buffer() {
        let dir =
            std::env::temp_dir().join(format!("folio word navigation {}", std::process::id()));
        ManagedProject::create(&dir, "Test paper", Engine::PdfLatex).unwrap();
        let path = dir.join("main.tex");
        let source = "Élodie puts the first word here and the second \\emph{word} there.\n";
        fs::write(&path, source).unwrap();
        let ctx = egui::Context::default();
        let mut app = App::with_context(ctx.clone(), Some(dir.clone()));
        // Match the current buffer even when a prefix was edited since building.
        app.tabs[0].doc.text.insert_str(0, "Updated: ");
        let rendered = "Élodie puts the first word here and the second word there.";
        let offset = rendered[..rendered.rfind("word").unwrap()].chars().count();
        app.jump_source_word(
            SourceLocation {
                path: path.clone(),
                line: 1,
                column: 0,
            },
            folio_core::navigation::WordAnchor::at(rendered, offset),
        );
        let tab = &app.tabs[app.active];
        let range = tab.jump.clone().unwrap();
        assert_eq!(
            tab.doc
                .text
                .chars()
                .skip(range.start)
                .take(range.len())
                .collect::<String>(),
            "word"
        );
        assert_eq!(
            range.start,
            tab.doc.text[..tab.doc.text.rfind("word").unwrap()]
                .chars()
                .count()
        );
        app.jump_source_word(
            SourceLocation {
                path,
                line: 1,
                column: 0,
            },
            folio_core::navigation::WordAnchor::at("generated", 0),
        );
        assert_eq!(
            app.tabs[app.active].jump,
            Some(0..0),
            "unmatched output uses a caret, not a sentence selection"
        );
        settle(&mut app, &ctx, |a| !a.scanning);
        drop(app);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn background_project_scan_publishes_changes_and_rejects_obsolete_results() {
        let dir = std::env::temp_dir().join(format!("folio scan {}", std::process::id()));
        ManagedProject::create(&dir, "Test paper", Engine::PdfLatex).unwrap();
        let main = dir.join("main.tex");
        fs::write(
            &main,
            "\\documentclass{article}\n\\begin{document}Before\\end{document}",
        )
        .unwrap();
        let ctx = egui::Context::default();
        let mut app = App::with_context(ctx.clone(), Some(dir.clone()));
        let main = main.canonicalize().unwrap();
        settle(&mut app, &ctx, |a| !a.scanning);
        let original_stamp = app.project_stamp;
        fs::write(
            &main,
            "\\documentclass{article}\n\\begin{document}Changed externally\\end{document}",
        )
        .unwrap();
        app.request_project_scan(&ctx);
        assert_eq!(
            app.project_stamp, original_stamp,
            "the UI only queues disk work"
        );
        settle(&mut app, &ctx, |a| !a.scanning);
        assert_ne!(app.project_stamp, original_stamp);
        assert!(app.disk_sources[&main].contains("Changed externally"));

        let obsolete = crate::project::scan(&main, &[]);
        app.request_project_scan(&ctx);
        let old_generation = app.scan_generation;
        fs::write(
            &main,
            "\\documentclass{article}\n\\begin{document}Newest disk contents\\end{document}",
        )
        .unwrap();
        app.refresh_project(); // a save/open supersedes an outstanding background scan
        app.tx
            .send(Event::Scanned(
                old_generation,
                app.layers.revision,
                obsolete,
            ))
            .unwrap();
        settle(&mut app, &ctx, |a| !a.scanning);
        assert!(app.disk_sources[&main].contains("Newest disk contents"));
        drop(app);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn completion_reuses_other_files_and_updates_only_changed_buffers() {
        let dir =
            std::env::temp_dir().join(format!("folio completion cache {}", std::process::id()));
        ManagedProject::create(&dir, "Test paper", Engine::PdfLatex).unwrap();
        let main = dir.join("main.tex");
        let bib = dir.join("references.bib");
        fs::write(&main, "\\cite{kn").unwrap();
        fs::write(&bib, "@book{knuth,title={Original},}").unwrap();
        let ctx = egui::Context::default();
        let mut app = App::with_context(ctx.clone(), Some(dir.clone()));
        settle(&mut app, &ctx, |a| !a.scanning);
        let bib = bib.canonicalize().unwrap();
        let cached = app.disk_indexes[&bib].clone();
        app.tabs[0].cursor = app.tabs[0].doc.text.chars().count();
        app.complete();
        assert!(
            app.completion.as_ref().unwrap().suggestions.items[0]
                .detail
                .contains("Original")
        );
        app.tabs[0].doc.text.push('u');
        app.tabs[0].edited();
        app.tabs[0].cursor += 1;
        app.complete();
        assert!(Arc::ptr_eq(&cached, &app.disk_indexes[&bib]));
        let active_index = app.tabs[0].index.clone();
        app.complete();
        assert!(Arc::ptr_eq(&active_index, &app.tabs[0].index));
        app.open_document(bib);
        app.tabs[1].doc.text = "@book{knuth,title={Unsaved},}".into();
        app.tabs[1].edited();
        app.active = 0;
        app.complete();
        let items = &app.completion.as_ref().unwrap().suggestions.items;
        assert_eq!(items.len(), 1);
        assert!(items[0].detail.contains("Unsaved"));
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn unedited_buffers_follow_external_changes_instead_of_blocking_saves() {
        let dir =
            std::env::temp_dir().join(format!("folio external reload {}", std::process::id()));
        ManagedProject::create(&dir, "Test paper", Engine::PdfLatex).unwrap();
        let ctx = egui::Context::default();
        let mut app = App::with_context(ctx.clone(), Some(dir.clone()));
        let clean = dir.join("sections/clean.tex");
        let edited = dir.join("sections/edited.tex");
        for path in [&clean, &edited] {
            fs::write(path, "saved").unwrap();
            app.open_document(path.clone());
        }
        app.tabs[app.active].doc.text = "my edit".into();
        app.tabs[app.active].cursor = 7;
        settle(&mut app, &ctx, |a| !a.scanning);

        // Save path: the clean buffer reloads at once and nothing is refused.
        fs::write(&clean, "pulled").unwrap();
        let revision = app.layers.revision;
        assert!(app.save_all(), "{}", app.status);
        let tab = |app: &App, name: &str| {
            app.tabs
                .iter()
                .position(|t| t.doc.path.ends_with(name))
                .unwrap()
        };
        let index = tab(&app, "clean.tex");
        assert_eq!(app.tabs[index].doc.text, "pulled");
        assert!(!app.tabs[index].doc.is_dirty());
        assert_ne!(app.layers.revision, revision, "the preview must rebuild");
        assert_eq!(fs::read_to_string(&edited).unwrap(), "my edit");

        // Background path: a scan delivers the newer text without any save.
        settle(&mut app, &ctx, |a| !a.scanning);
        fs::write(&clean, "pulled\nagain").unwrap();
        app.refresh_project();
        settle(&mut app, &ctx, |a| !a.scanning);
        assert_eq!(app.tabs[index].doc.text, "pulled\nagain");
        assert_eq!(app.tabs[index].line_count, 2);

        // An edited buffer is never replaced; it conflicts as before.
        let index = tab(&app, "edited.tex");
        app.tabs[index].doc.text = "second edit".into();
        fs::write(&edited, "theirs").unwrap();
        app.refresh_project();
        settle(&mut app, &ctx, |a| !a.scanning);
        assert_eq!(app.tabs[index].doc.text, "second edit");
        assert!(!app.save_all());
        assert_eq!(fs::read_to_string(&edited).unwrap(), "theirs");
        settle(&mut app, &ctx, |a| !a.scanning);
        drop(app);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn save_all_preflights_every_buffer_and_reports_all_conflicts() {
        let dir = std::env::temp_dir().join(format!("folio save conflicts {}", std::process::id()));
        ManagedProject::create(&dir, "Test paper", Engine::PdfLatex).unwrap();
        let ctx = egui::Context::default();
        let mut app = App::with_context(ctx.clone(), Some(dir.clone()));
        for name in ["a.tex", "b.tex", "c.tex"] {
            let path = dir.join("sections").join(name);
            fs::write(&path, "saved").unwrap();
            app.open_document(path);
            app.tabs[app.active].doc.text = "unsaved".into();
        }
        fs::write(dir.join("sections/b.tex"), "external").unwrap();
        fs::write(dir.join("sections/c.tex"), "external").unwrap();
        assert!(!app.save_all());
        assert_eq!(
            fs::read_to_string(dir.join("sections/a.tex")).unwrap(),
            "saved",
            "a later conflict must not cause a partial save"
        );
        assert!(app.status.contains("b.tex") && app.status.contains("c.tex"));
        settle(&mut app, &ctx, |a| !a.scanning);
        fs::remove_dir_all(dir).unwrap();
    }

    pub(super) fn settle(app: &mut App, ctx: &egui::Context, ready: impl Fn(&App) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(25);
        loop {
            app.poll(ctx);
            if ready(app) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "{}\n{}\n{:?}",
                app.status,
                app.log,
                app.preview.error
            );
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn navigate_with_mouse(app: &mut App, ctx: &egui::Context) {
        fn frame(app: &mut App, ctx: &egui::Context, events: Vec<egui::Event>) {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800., 900.),
                    )),
                    modifiers: egui::Modifiers::COMMAND,
                    events,
                    ..Default::default()
                },
                |ctx| app.layer_ui(ctx),
            );
        }
        frame(app, ctx, vec![]);
        let pos = egui::pos2(400., 300.);
        frame(app, ctx, vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false, true, false] {
            frame(
                app,
                ctx,
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::COMMAND,
                }],
            );
        }
    }

    #[test]
    #[ignore = "requires TeX, BibTeX, SyncTeX and PDFium"]
    fn real_pdf_word_navigation_selects_exact_occurrences_in_included_source() {
        for engine in [Engine::PdfLatex, Engine::XeLatex] {
            let dir = std::env::temp_dir().join(format!(
                "folio exact words {engine:?} {}",
                std::process::id()
            ));
            ManagedProject::create(&dir, "Test paper", engine).unwrap();
            let main = dir.join("main.tex");
            let chapter = dir.join("sections/chapter.tex");
            fs::write(&main, "\\documentclass{article}\n\\usepackage[T1]{fontenc}\n\\begin{document}\n\\input{sections/chapter}\n\\end{document}\n").unwrap();
            let source = "\\section{Exact navigation}\nFirst marker stays here; the \\textbf{second marker} moves there.\n\nUnicode café and efficient typography live in this included file.\n";
            fs::write(&chapter, source).unwrap();
            let ctx = egui::Context::default();
            let mut app = App::with_context(ctx.clone(), Some(dir.clone()));
            settle(&mut app, &ctx, |a| !a.scanning);
            app.enter_preview(&ctx, false);
            settle(&mut app, &ctx, |a| {
                !a.building && !a.preview.is_loading() && a.preview.indexed > 0
            });
            let pdfium = crate::pdf_worker::bind().unwrap();
            let pdf = pdfium
                .load_pdf_from_file(app.preview.path.as_ref().unwrap(), None)
                .unwrap();
            let page = pdf.pages().get(0).unwrap();
            let text = crate::pdf_worker::index_page(&page).unwrap();
            for (word, occurrence) in [("marker", 0), ("marker", 1), ("café", 0), ("efficient", 0)]
            {
                app.enter_preview(&ctx, false);
                let matches = text.find(word);
                let range = matches
                    .get(occurrence)
                    .unwrap_or_else(|| panic!("{engine:?}: missing {word} in {}", text.text()));
                let rect = text.glyphs[range.start + 1].rect;
                app.preview
                    .select_point(0, rect.x + rect.width / 2., rect.y + rect.height / 2.);
                assert!(app.preview.go_word().is_some());
                app.go(&ctx);
                settle(&mut app, &ctx, |a| a.layers.layer == Layer::Source);
                let tab = &app.tabs[app.active];
                assert_eq!(tab.doc.path, chapter.canonicalize().unwrap());
                let byte = source.match_indices(word).nth(occurrence).unwrap().0;
                let expected = source[..byte].chars().count();
                assert_eq!(
                    tab.jump.as_ref().unwrap(),
                    &(expected..expected + word.chars().count()),
                    "{engine:?} should select occurrence {occurrence} of {word}, not the sentence"
                );
            }
            settle(&mut app, &ctx, |a| !a.scanning);
            drop(app);
            fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    #[ignore = "requires TeX, BibTeX, SyncTeX and PDFium"]
    fn real_layer_workflow_caches_rebuilds_and_goes_both_directions() {
        let dir = std::env::temp_dir().join(format!("folio layers {}", std::process::id()));
        ManagedProject::create(&dir, "Test paper", Engine::PdfLatex).unwrap();
        let main = dir.join("main.tex");
        let chapter = dir.join("sections/chapter.tex");
        // navigate_with_mouse clicks a fixed screen point that must stay on blank
        // paper; pin the page size so the TeX installation's default (A4 or
        // letter) cannot move the bibliography under it.
        fs::write(&main, "\\documentclass{article}\n\\pdfpagewidth=8.5in\n\\pdfpageheight=11in\n\\begin{document}\n\\input{sections/chapter}\n\\bibliographystyle{plain}\n\\bibliography{references}\n\\end{document}\n").unwrap();
        fs::write(&chapter, "\\section{Navigation}\nThis sentence lives in the included chapter.\n\nA separate paragraph cites Knuth~\\cite{knuth}.\n").unwrap();
        fs::write(dir.join("references.bib"), "@book{knuth, author={Donald Knuth}, title={The TeXbook}, year={1984}, publisher={Addison-Wesley}}\n").unwrap();
        let ctx = egui::Context::default();
        let mut app = App::with_context(ctx.clone(), Some(dir.clone()));
        settle(&mut app, &ctx, |a| !a.scanning);
        assert!(
            app.disk_indexes
                .keys()
                .any(|p| p.ends_with("references.bib"))
        );
        app.enter_preview(&ctx, false);
        settle(&mut app, &ctx, |a| !a.building && !a.preview.is_loading());
        assert!(!app.layers.stale(), "{}\n{:?}", app.log, app.preview.error);
        assert_eq!(app.layers.layer, Layer::Preview);
        let first = app
            .preview
            .path
            .clone()
            .unwrap_or_else(|| panic!("{}\n{:?}", app.log, app.preview.error));
        assert!(
            fs::read_to_string(dir.join(".folio/main.bbl"))
                .unwrap()
                .contains("TeXbook")
        );
        app.source();
        app.enter_preview(&ctx, false);
        assert!(!app.building, "unchanged preview should be instantaneous");
        assert_eq!(app.preview.path.as_ref(), Some(&first));
        app.open_document(chapter.clone());
        let tab = &mut app.tabs[app.active];
        tab.doc.text.push_str("\nMore text after editing.\n");
        tab.selection = document::line_char_range(&tab.doc.text, 2);
        tab.jump = Some(tab.selection.clone());
        app.layers.edited();
        navigate_with_mouse(&mut app, &ctx);
        assert!(app.building || app.preview_pending);
        settle(&mut app, &ctx, |a| {
            a.preview.navigation_highlight().is_some()
        });
        assert_ne!(app.preview.path.as_ref(), Some(&first));
        assert!(!app.layers.stale());
        let h = app.preview.navigation_highlight().unwrap().clone();
        app.preview.select_point(h.page - 1, h.x, h.y);
        assert_eq!(
            app.layers.layer,
            Layer::Preview,
            "selection alone stays in PDF"
        );
        navigate_with_mouse(&mut app, &ctx);
        settle(&mut app, &ctx, |a| a.layers.layer == Layer::Source);
        let tab = &app.tabs[app.active];
        assert_eq!(tab.doc.path, chapter.canonicalize().unwrap());
        let range = tab.jump.as_ref().unwrap().clone();
        let (line, _) = document::char_position(&tab.doc.text, range.start);
        assert!((1..=2).contains(&line), "inverse line {line}");
        let old_token = app.nav_token;
        app.source();
        app.tx
            .send(Event::Source(
                old_token,
                Ok(SourceLocation {
                    path: chapter,
                    line: 4,
                    column: 0,
                }),
                None,
            ))
            .unwrap();
        app.poll(&ctx);
        assert_eq!(app.tabs[app.active].jump.as_ref().unwrap(), &range);
        drop(app);
        fs::remove_dir_all(dir).unwrap();
    }
}

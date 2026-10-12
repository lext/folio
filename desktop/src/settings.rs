use eframe::egui::{self, Key, Modifiers, PointerButton};
use folio_core::build::Engine;
use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::Arc,
};
use toml_edit::{DocumentMut, Item};

pub const FILE_NAME: &str = "folio.toml";
pub const DEFAULT_TOML: &str = include_str!("../../folio.toml");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Action {
    Save,
    Recompile,
    Find,
    Go,
    Complete,
    CompletionNext,
    CompletionPrevious,
    CompletionAccept,
    CompletionDismiss,
    FindNext,
    FindPrevious,
    UiZoomIn,
    UiZoomOut,
    UiZoomReset,
    Switcher,
    Commands,
    Source,
    Preview,
    NextSurface,
    PreviousSurface,
    LauncherNext,
    LauncherPrevious,
    LauncherAccept,
    LauncherDismiss,
    Dismiss,
    ShowLog,
    Git,
    Outline,
    CancelBuild,
    Reload,
    Help,
    PdfZoomIn,
    PdfZoomOut,
    PdfFit,
    PdfNext,
    PdfPrevious,
    NewProject,
    OpenProject,
    ProjectSettings,
    ValidateProject,
    ExportProject,
    AddProjectFile,
    ImportFigure,
    ProjectFiles,
    ParagraphDown,
    ParagraphUp,
    SentenceNext,
    SentencePrevious,
    PdfFitPage,
    Tab1,
    Tab2,
    Tab3,
    Tab4,
    Tab5,
    Tab6,
    Tab7,
    Tab8,
    Tab9,
}
impl Action {
    pub const ALL: [Self; 58] = [
        Self::Save,
        Self::Recompile,
        Self::Find,
        Self::Go,
        Self::Complete,
        Self::CompletionNext,
        Self::CompletionPrevious,
        Self::CompletionAccept,
        Self::CompletionDismiss,
        Self::FindNext,
        Self::FindPrevious,
        Self::UiZoomIn,
        Self::UiZoomOut,
        Self::UiZoomReset,
        Self::Switcher,
        Self::Commands,
        Self::Source,
        Self::Preview,
        Self::NextSurface,
        Self::PreviousSurface,
        Self::LauncherNext,
        Self::LauncherPrevious,
        Self::LauncherAccept,
        Self::LauncherDismiss,
        Self::Dismiss,
        Self::ShowLog,
        Self::Git,
        Self::Outline,
        Self::CancelBuild,
        Self::Reload,
        Self::Help,
        Self::PdfZoomIn,
        Self::PdfZoomOut,
        Self::PdfFit,
        Self::PdfNext,
        Self::PdfPrevious,
        Self::NewProject,
        Self::OpenProject,
        Self::ProjectSettings,
        Self::ValidateProject,
        Self::ExportProject,
        Self::AddProjectFile,
        Self::ImportFigure,
        Self::ProjectFiles,
        Self::ParagraphDown,
        Self::ParagraphUp,
        Self::SentenceNext,
        Self::SentencePrevious,
        Self::PdfFitPage,
        Self::Tab1,
        Self::Tab2,
        Self::Tab3,
        Self::Tab4,
        Self::Tab5,
        Self::Tab6,
        Self::Tab7,
        Self::Tab8,
        Self::Tab9,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Save => "save",
            Self::Recompile => "recompile",
            Self::Find => "find",
            Self::Go => "go",
            Self::Complete => "complete",
            Self::CompletionNext => "completion_next",
            Self::CompletionPrevious => "completion_previous",
            Self::CompletionAccept => "completion_accept",
            Self::CompletionDismiss => "completion_dismiss",
            Self::FindNext => "find_next",
            Self::FindPrevious => "find_previous",
            Self::UiZoomIn => "ui_zoom_in",
            Self::UiZoomOut => "ui_zoom_out",
            Self::UiZoomReset => "ui_zoom_reset",
            Self::Switcher => "switcher",
            Self::Commands => "commands",
            Self::Source => "source",
            Self::Preview => "preview",
            Self::NextSurface => "next_surface",
            Self::PreviousSurface => "previous_surface",
            Self::LauncherNext => "launcher_next",
            Self::LauncherPrevious => "launcher_previous",
            Self::LauncherAccept => "launcher_accept",
            Self::LauncherDismiss => "launcher_dismiss",
            Self::Dismiss => "dismiss",
            Self::ShowLog => "show_log",
            Self::Git => "git",
            Self::Outline => "outline",
            Self::CancelBuild => "cancel_build",
            Self::Reload => "reload",
            Self::Help => "help",
            Self::PdfZoomIn => "pdf_zoom_in",
            Self::PdfZoomOut => "pdf_zoom_out",
            Self::PdfFit => "pdf_fit",
            Self::PdfNext => "pdf_next_page",
            Self::PdfPrevious => "pdf_previous_page",
            Self::NewProject => "new_project",
            Self::OpenProject => "open_project",
            Self::ProjectSettings => "project_settings",
            Self::ValidateProject => "validate_project",
            Self::ExportProject => "export_project",
            Self::AddProjectFile => "add_project_file",
            Self::ImportFigure => "import_figure",
            Self::ProjectFiles => "project_files",
            Self::ParagraphDown => "paragraph_down",
            Self::ParagraphUp => "paragraph_up",
            Self::SentenceNext => "sentence_next",
            Self::SentencePrevious => "sentence_previous",
            Self::PdfFitPage => "pdf_fit_page",
            Self::Tab1 => "tab_1",
            Self::Tab2 => "tab_2",
            Self::Tab3 => "tab_3",
            Self::Tab4 => "tab_4",
            Self::Tab5 => "tab_5",
            Self::Tab6 => "tab_6",
            Self::Tab7 => "tab_7",
            Self::Tab8 => "tab_8",
            Self::Tab9 => "tab_9",
        }
    }
    pub fn motion(self) -> Option<crate::language::Motion> {
        use crate::language::Motion;
        match self {
            Self::ParagraphDown => Some(Motion::ParagraphEnd),
            Self::ParagraphUp => Some(Motion::ParagraphStart),
            Self::SentenceNext => Some(Motion::SentenceEnd),
            Self::SentencePrevious => Some(Motion::SentenceStart),
            _ => None,
        }
    }
    /// Zero-based position in the tab strip that this action jumps to.
    pub fn tab(self) -> Option<usize> {
        (self as usize).checked_sub(Self::Tab1 as usize)
    }
    pub fn is_pdf_fit(self) -> bool {
        matches!(self, Self::PdfFit | Self::PdfFitPage)
    }
    pub fn is_command(self) -> bool {
        !matches!(
            self,
            Self::CompletionNext
                | Self::CompletionPrevious
                | Self::CompletionAccept
                | Self::CompletionDismiss
                | Self::FindNext
                | Self::FindPrevious
                | Self::LauncherNext
                | Self::LauncherPrevious
                | Self::LauncherAccept
                | Self::LauncherDismiss
                | Self::Dismiss
        )
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Save => "Save",
            Self::Recompile => "Recompile",
            Self::Find => "Find in current layer",
            Self::Go => "Go to selection in other layer",
            Self::Complete => "Suggestions",
            Self::CompletionNext => "Next suggestion",
            Self::CompletionPrevious => "Previous suggestion",
            Self::CompletionAccept => "Accept suggestion",
            Self::CompletionDismiss => "Dismiss suggestions",
            Self::FindNext => "Next search match (in search field)",
            Self::FindPrevious => "Previous search match (in search field)",
            Self::UiZoomIn => "Increase interface scale",
            Self::UiZoomOut => "Decrease interface scale",
            Self::UiZoomReset => "Reset interface scale",
            Self::Switcher => "Switch surface",
            Self::Commands => "Commands",
            Self::Source => "Focus source",
            Self::Preview => "Focus PDF preview",
            Self::NextSurface => "Next tab",
            Self::PreviousSurface => "Previous tab",
            Self::LauncherNext => "Next launcher result",
            Self::LauncherPrevious => "Previous launcher result",
            Self::LauncherAccept => "Open launcher result",
            Self::LauncherDismiss => "Close launcher",
            Self::Dismiss => "Dismiss panel or search",
            Self::ShowLog => "Compiler output",
            Self::Git => "Git comparison",
            Self::Outline => "Go to section",
            Self::CancelBuild => "Cancel typesetting or export",
            Self::Reload => "Reload active file…",
            Self::Help => "Keyboard shortcuts",
            Self::PdfZoomIn => "PDF: zoom in",
            Self::PdfZoomOut => "PDF: zoom out",
            Self::PdfFit => "PDF: fit width",
            Self::PdfNext => "PDF: next page",
            Self::PdfPrevious => "PDF: previous page",
            Self::NewProject => "New project…",
            Self::OpenProject => "Open project…",
            Self::ProjectSettings => "Project settings",
            Self::ValidateProject => "Validate project",
            Self::ExportProject => "Export flat project…",
            Self::AddProjectFile => "Add project file…",
            Self::ImportFigure => "Import figure…",
            Self::ProjectFiles => "Project files",
            Self::ParagraphDown => "Move to paragraph end",
            Self::ParagraphUp => "Move to paragraph start",
            Self::SentenceNext => "Move to sentence end",
            Self::SentencePrevious => "Move to sentence start",
            Self::PdfFitPage => "PDF: fit page",
            Self::Tab1 => "Go to tab 1",
            Self::Tab2 => "Go to tab 2",
            Self::Tab3 => "Go to tab 3",
            Self::Tab4 => "Go to tab 4",
            Self::Tab5 => "Go to tab 5",
            Self::Tab6 => "Go to tab 6",
            Self::Tab7 => "Go to tab 7",
            Self::Tab8 => "Go to tab 8",
            Self::Tab9 => "Go to tab 9",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Chord {
    pub modifiers: Modifiers,
    pub key: Key,
    pub label: String,
    pub presses: u8,
}
#[derive(Clone, Debug)]
pub struct Keybindings {
    bindings: [Vec<Chord>; Action::ALL.len()],
}
impl Keybindings {
    pub fn chords(&self, action: Action) -> &[Chord] {
        &self.bindings[action as usize]
    }
    pub fn consume(&self, ctx: &egui::Context, action: Action) -> bool {
        let bindings = &self.bindings[action as usize];
        ctx.input_mut(|input| {
            let mut consumed = false;
            let mut option = false;
            input.events.retain(|event| {
                let matches = matches!(event, egui::Event::Key { key, modifiers, pressed: true, .. }
                    if bindings.iter().any(|b| b.presses == 1 && b.key == *key && modifiers_match(*modifiers, b.modifiers)));
                consumed |= matches;
                option |= matches
                    && matches!(event, egui::Event::Key { modifiers, .. } if modifiers.alt && !modifiers.command && !modifiers.ctrl);
                !matches
            });
            // macOS also types a character for an Option chord (Option+1 is
            // "¡"); a shortcut that fired must not insert it into the text.
            if option {
                input
                    .events
                    .retain(|event| !matches!(event, egui::Event::Text(_)));
            }
            consumed
        })
    }
    /// Whether any action is bound to Cmd plus this key alone.
    #[cfg(target_os = "macos")]
    pub fn binds_command(&self, key: Key) -> bool {
        self.bindings.iter().flatten().any(|b| {
            b.key == key
                && (b.modifiers.command || b.modifiers.mac_cmd)
                && !b.modifiers.alt
                && !b.modifiers.shift
                && !b.modifiers.ctrl
        })
    }
    pub fn hint(&self, action: Action) -> String {
        self.bindings[action as usize]
            .iter()
            .map(|b| b.label.as_str())
            .collect::<Vec<_>>()
            .join(" / ")
    }
    pub fn button(&self, action: Action) -> egui::Button<'static> {
        egui::Button::new(action.label()).shortcut_text(self.hint(action))
    }
}
#[derive(Clone, Debug)]
pub struct MouseGesture {
    pub modifiers: Modifiers,
    pub button: PointerButton,
    pub clicks: u8,
    pub label: String,
}
#[derive(Clone, Debug)]
pub struct MouseBindings {
    pub go: Option<MouseGesture>,
    pub zoom: Vec<Modifiers>,
    pub horizontal_scroll: Vec<Modifiers>,
}
#[derive(Clone, Debug)]
pub struct EditorSettings {
    pub line_numbers: bool,
    pub font_size: f32,
    pub auto_complete: bool,
}
#[derive(Clone, Debug)]
pub struct PreviewSettings {
    pub fit_on_switch: Option<PdfFit>,
    pub width_fraction: f32,
    pub fit_width: bool,
    pub zoom: f32,
    pub cache_mib: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PdfFit {
    Page,
    Width,
}
#[derive(Clone, Debug)]
pub struct Settings {
    pub keybindings: Keybindings,
    pub mouse: MouseBindings,
    pub editor: EditorSettings,
    pub preview: PreviewSettings,
    pub theme: crate::theme::Mode,
    pub engine: Engine,
    pub build_timeout: std::time::Duration,
    pub window_size: [f32; 2],
    pub fullscreen: bool,
}
#[derive(Debug)]
pub struct Loaded {
    pub settings: Arc<Settings>,
    pub path: Option<PathBuf>,
}
impl Default for Settings {
    fn default() -> Self {
        Self::parse("").expect("bundled folio.toml is validated by tests")
    }
}
impl Settings {
    pub fn parse(source: &str) -> Result<Self, String> {
        let mut doc = DEFAULT_TOML
            .parse::<DocumentMut>()
            .map_err(|e| e.to_string())?;
        let overrides = source.parse::<DocumentMut>().map_err(|e| e.to_string())?;
        // Bindings the file sets itself; the rest come from bundled defaults.
        let mut chosen = std::collections::HashSet::new();
        for (section, item) in overrides.iter() {
            let target = doc
                .get_mut(section)
                .and_then(Item::as_table_mut)
                .ok_or_else(|| format!("Unknown settings section {section}"))?;
            let table = item
                .as_table_like()
                .ok_or_else(|| format!("{section}: expected a table"))?;
            for (key, value) in table.iter() {
                let key = if section == "keybindings" {
                    match key {
                        "set_main" | "save_copy" | "engine_pdflatex" | "engine_xelatex"
                        | "engine_lualatex" => {
                            // Retired loose-file actions in pre-0.8 settings.
                            strings(value, &format!("keybindings.{key}"))?;
                            continue;
                        }
                        "open" | "new" => {
                            let action = if key == "open" {
                                "open_project"
                            } else {
                                "new_project"
                            };
                            if table
                                .get(action)
                                .and_then(Item::as_array)
                                .is_some_and(|a| !a.is_empty())
                            {
                                continue;
                            }
                            action
                        }
                        "open_project" | "new_project" => {
                            let legacy = if key == "open_project" { "open" } else { "new" };
                            if value.as_array().is_some_and(|a| a.is_empty())
                                && table.contains_key(legacy)
                            {
                                continue;
                            }
                            key
                        }
                        _ => key,
                    }
                } else {
                    key
                };
                if !target.contains_key(key) {
                    return Err(format!("Unknown setting {section}.{key}"));
                }
                if section == "keybindings" {
                    chosen.insert(key.to_owned());
                }
                target[key] = value.clone();
            }
        }
        let mut bindings: [Vec<Chord>; Action::ALL.len()] = std::array::from_fn(|_| Vec::new());
        for action in Action::ALL {
            let field = format!("keybindings.{}", action.name());
            bindings[action as usize] = strings(&doc["keybindings"][action.name()], &field)?
                .into_iter()
                .map(|text| parse_chord(text).map_err(|e| format!("{field}: {e}")))
                .collect::<Result<_, _>>()?;
            if !action.is_pdf_fit() && bindings[action as usize].iter().any(|b| b.presses == 2) {
                return Err(format!(
                    "{field}: double presses are supported for PDF fitting commands"
                ));
            }
        }
        let chosen = Action::ALL.map(|a| chosen.contains(a.name()));
        validate_conflicts(&mut bindings, &chosen, cfg!(target_os = "macos"))?;
        let gesture = string(&doc["mouse"]["go"], "mouse.go")?;
        let go = if gesture.is_empty() {
            None
        } else {
            Some(parse_gesture(gesture).map_err(|e| format!("mouse.go: {e}"))?)
        };
        let wheel = |field| -> Result<Vec<Modifiers>, String> {
            let name = format!("mouse.{field}");
            strings(&doc["mouse"][field], &name)?
                .into_iter()
                .map(|s| {
                    parse_modifiers(s)
                        .and_then(|m| {
                            if m.is_none() {
                                Err("expected at least one modifier".into())
                            } else {
                                Ok(m)
                            }
                        })
                        .map_err(|e| format!("{name}: {e}"))
                })
                .collect()
        };
        let engine = match string(&doc["build"]["engine"], "build.engine")? {
            "pdflatex" => Engine::PdfLatex,
            "xelatex" => Engine::XeLatex,
            "lualatex" => Engine::LuaLatex,
            _ => return Err("build.engine: expected pdflatex, xelatex or lualatex".into()),
        };
        let number = |section, key, min, max| {
            number(&doc[section][key], &format!("{section}.{key}"), min, max)
        };
        let boolean = |section, key| {
            doc[section][key]
                .as_bool()
                .ok_or_else(|| format!("{section}.{key}: expected true or false"))
        };
        let cache_mib = doc["preview"]["cache_mib"]
            .as_integer()
            .filter(|n| (32..=4096).contains(n))
            .ok_or("preview.cache_mib: expected an integer between 32 and 4096")?
            as usize;
        Ok(Self {
            build_timeout: std::time::Duration::from_secs(doc["build"]["timeout_seconds"].as_integer()
                .filter(|n| (0..=86400).contains(n))
                .ok_or("build.timeout_seconds: expected an integer between 0 and 86400 (0 disables timeout)")? as u64),
            keybindings: Keybindings { bindings },
            mouse: MouseBindings {
                go,
                zoom: wheel("zoom")?,
                horizontal_scroll: wheel("horizontal_scroll")?,
            },
            editor: EditorSettings {
                line_numbers: boolean("editor", "line_numbers")?,
                font_size: number("editor", "font_size", 8., 48.)?,
                auto_complete: boolean("editor", "auto_complete")?,
            },
            preview: PreviewSettings {
                fit_on_switch: match string(&doc["preview"]["fit_on_switch"], "preview.fit_on_switch")? {
                    "page" => Some(PdfFit::Page),
                    "width" => Some(PdfFit::Width),
                    "keep" => None,
                    _ => return Err("preview.fit_on_switch: expected page, width or keep".into()),
                },
                fit_width: boolean("preview", "fit_width")?,
                width_fraction: number("preview", "width_fraction", 0.1, 1.)?,
                zoom: number("preview", "zoom", 0.25, 4.)?,
                cache_mib,
            },
            theme: match string(&doc["theme"]["mode"], "theme.mode")? {
                "dark" => crate::theme::Mode::Dark,
                "light" => crate::theme::Mode::Light,
                "system" => crate::theme::Mode::System,
                _ => return Err("theme.mode: expected dark, light or system".into()),
            },
            engine,
            fullscreen: doc["window"]["fullscreen"]
                .as_bool()
                .ok_or("window.fullscreen: expected true or false")?,
            window_size: [
                number("window", "width", 1000., 8192.)?,
                number("window", "height", 600., 8192.)?,
            ],
        })
    }
    pub fn load_at(executable: &Path, home: Option<&Path>) -> Result<Loaded, String> {
        let local = executable.parent().map(|p| p.join(FILE_NAME));
        let global = home.map(|p| p.join(".config/folio").join(FILE_NAME));
        for path in [local, global].into_iter().flatten() {
            match fs::read_to_string(&path) {
                Ok(text) => {
                    let settings =
                        Self::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
                    return Ok(Loaded {
                        settings: Arc::new(settings),
                        path: Some(path),
                    });
                }
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => return Err(format!("{}: {error}", path.display())),
            }
        }
        Ok(Loaded {
            settings: Arc::new(Self::default()),
            path: None,
        })
    }
    pub fn load() -> Result<Loaded, String> {
        let executable =
            std::env::current_exe().map_err(|e| format!("Cannot locate executable: {e}"))?;
        let home = std::env::var_os("HOME").map(PathBuf::from);
        Self::load_at(&executable, home.as_deref())
    }
    pub fn go_hint(&self) -> String {
        self.mouse
            .go
            .as_ref()
            .map(|g| g.label.clone())
            .unwrap_or_else(|| self.keybindings.hint(Action::Go))
    }
}

fn string<'a>(item: &'a Item, field: &str) -> Result<&'a str, String> {
    item.as_str()
        .ok_or_else(|| format!("{field}: expected a string"))
}
fn strings<'a>(item: &'a Item, field: &str) -> Result<Vec<&'a str>, String> {
    item.as_array()
        .ok_or_else(|| format!("{field}: expected an array of strings"))?
        .iter()
        .map(|v| {
            v.as_str()
                .ok_or_else(|| format!("{field}: expected an array of strings"))
        })
        .collect()
}
fn number(item: &Item, field: &str, min: f32, max: f32) -> Result<f32, String> {
    item.as_float()
        .or_else(|| item.as_integer().map(|i| i as f64))
        .map(|v| v as f32)
        .filter(|v| v.is_finite() && (min..=max).contains(v))
        .ok_or_else(|| format!("{field}: expected a number between {min} and {max}"))
}
fn parse_modifiers(text: &str) -> Result<Modifiers, String> {
    if text.is_empty() {
        return Ok(Modifiers::NONE);
    }
    let mut modifiers = Modifiers::NONE;
    for part in text.split('+').map(str::trim) {
        modifiers |= match part.to_ascii_lowercase().as_str() {
            "primary" => Modifiers::COMMAND,
            "cmd" | "command" | "super" => Modifiers::MAC_CMD,
            "ctrl" | "control" => Modifiers::CTRL,
            "alt" | "option" => Modifiers::ALT,
            "shift" => Modifiers::SHIFT,
            _ => return Err(format!("unknown modifier '{part}'")),
        };
    }
    Ok(modifiers)
}
fn split_chord(text: &str) -> Result<(Modifiers, &str), String> {
    let (modifiers, key) = text.rsplit_once('+').unwrap_or(("", text));
    Ok((parse_modifiers(modifiers)?, key.trim()))
}
fn display_chord(text: &str) -> String {
    text.split('+')
        .map(|s| match s.trim().to_ascii_lowercase().as_str() {
            "primary" => {
                if cfg!(target_os = "macos") {
                    "⌘".into()
                } else {
                    "Ctrl".into()
                }
            }
            "cmd" | "command" | "super" => "⌘".into(),
            "doubleclick" => "double-click".into(),
            "rightdoubleclick" => "right double-click".into(),
            "middledoubleclick" => "middle double-click".into(),
            "click" => "click".into(),
            "rightclick" => "right-click".into(),
            "middleclick" => "middle-click".into(),
            _ => s.trim().to_owned(),
        })
        .collect::<Vec<String>>()
        .join(" + ")
}
fn parse_chord(text: &str) -> Result<Chord, String> {
    // Repeated fitting chords use a compact sequence, e.g. Space Space or F7 F7.
    // Preserve the existing allowance for whitespace around modifiers and '+'.
    let compact = text.split('+').map(str::trim).collect::<Vec<_>>().join("+");
    let parts: Vec<_> = compact.split_whitespace().collect();
    let (text, presses) = match parts.as_slice() {
        [single] => (*single, 1),
        [first, second] if first.eq_ignore_ascii_case(second) => (*first, 2),
        _ => return Err("expected one chord or the same chord twice (e.g. Space Space)".into()),
    };
    let (modifiers, name) = split_chord(text)?;
    let key = Key::from_name(name)
        .or_else(|| {
            Key::ALL
                .iter()
                .copied()
                .find(|k| k.name().eq_ignore_ascii_case(name))
        })
        .ok_or_else(|| format!("unknown key '{name}' (use Plus for the + key)"))?;
    Ok(Chord {
        modifiers,
        key,
        label: if presses == 2 {
            format!("{} ×2", display_chord(text))
        } else {
            display_chord(text)
        },
        presses,
    })
}
fn parse_gesture(text: &str) -> Result<MouseGesture, String> {
    let (modifiers, name) = split_chord(text)?;
    if modifiers.is_none() {
        return Err("go needs a modifier to preserve normal text selection".into());
    }
    let (button, clicks) = match name.to_ascii_lowercase().as_str() {
        "click" => (PointerButton::Primary, 1),
        "doubleclick" => (PointerButton::Primary, 2),
        "rightclick" => (PointerButton::Secondary, 1),
        "rightdoubleclick" => (PointerButton::Secondary, 2),
        "middleclick" => (PointerButton::Middle, 1),
        "middledoubleclick" => (PointerButton::Middle, 2),
        _ => return Err(format!("unknown mouse gesture '{name}'")),
    };
    Ok(MouseGesture {
        modifiers,
        button,
        clicks,
        label: display_chord(text),
    })
}
fn physical_modifiers(mut modifiers: Modifiers, mac: bool) -> Modifiers {
    if modifiers.command {
        if mac {
            modifiers.mac_cmd = true;
        } else {
            modifiers.ctrl = true;
        }
        modifiers.command = false;
    }
    modifiers
}
pub fn modifiers_match(actual: Modifiers, expected: Modifiers) -> bool {
    physical_modifiers(actual, cfg!(target_os = "macos"))
        == physical_modifiers(expected, cfg!(target_os = "macos"))
}
/// Rejects two actions on one chord. The exception is a default added after
/// settings files were already in use (the tab positions): it gives way to a
/// chord the file chose, so a file written for an older Folio keeps loading.
fn validate_conflicts(
    bindings: &mut [Vec<Chord>; Action::ALL.len()],
    chosen: &[bool; Action::ALL.len()],
    mac: bool,
) -> Result<(), String> {
    let scope = |action| match action {
        Action::CompletionNext
        | Action::CompletionPrevious
        | Action::CompletionAccept
        | Action::CompletionDismiss => 1,
        Action::LauncherNext
        | Action::LauncherPrevious
        | Action::LauncherAccept
        | Action::LauncherDismiss => 2,
        Action::FindNext | Action::FindPrevious => 4,
        Action::Dismiss => 4 | 8 | 16,
        Action::PdfFit | Action::PdfFitPage => 8,
        Action::PdfZoomIn | Action::PdfZoomOut | Action::PdfNext | Action::PdfPrevious => 4 | 8,
        _ => 31,
    };
    for (i, a) in Action::ALL.iter().copied().enumerate() {
        for b in Action::ALL.iter().copied().skip(i + 1) {
            if scope(a) & scope(b) == 0 {
                continue;
            }
            let same = |x: &Chord, y: &Chord| {
                x.key == y.key
                    && !(a.is_pdf_fit() && b.is_pdf_fit() && x.presses != y.presses)
                    && physical_modifiers(x.modifiers, mac) == physical_modifiers(y.modifiers, mac)
            };
            let (first, second) = (bindings[a as usize].clone(), bindings[b as usize].clone());
            let Some(x) = first.iter().find(|x| second.iter().any(|y| same(x, y))) else {
                continue;
            };
            match (chosen[a as usize], chosen[b as usize]) {
                (true, false) if b.tab().is_some() => {
                    bindings[b as usize].retain(|y| !first.iter().any(|x| same(x, y)));
                }
                (false, true) if a.tab().is_some() => {
                    bindings[a as usize].retain(|x| !second.iter().any(|y| same(x, y)));
                }
                _ => {
                    return Err(format!(
                        "keybindings.{} conflicts with keybindings.{}: {}",
                        a.name(),
                        b.name(),
                        x.label
                    ));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prose_motions_have_two_key_defaults_and_can_be_remapped_or_disabled() {
        let defaults = Settings::default();
        for (name, key) in [
            ("paragraph_down", Key::D),
            ("paragraph_up", Key::U),
            ("sentence_next", Key::J),
            ("sentence_previous", Key::K),
        ] {
            let action = Action::ALL
                .into_iter()
                .find(|a| a.name() == name)
                .expect("registered source motion");
            let chords = &defaults.keybindings.bindings[action as usize];
            assert_eq!(chords.len(), 1);
            assert_eq!(chords[0].key, key);
            assert_eq!(chords[0].modifiers, Modifiers::COMMAND);
            let disabled = Settings::parse(&format!("[keybindings]\n{name}=[]")).unwrap();
            assert!(disabled.keybindings.bindings[action as usize].is_empty());
            let custom = Settings::parse(&format!("[keybindings]\n{name}=['Alt+F8']")).unwrap();
            assert_eq!(custom.keybindings.bindings[action as usize][0].key, Key::F8);
            assert!(
                Settings::parse(&format!("[keybindings]\n{name}=['Primary+S']"))
                    .unwrap_err()
                    .contains("conflicts")
            );
        }
        for mac in [true, false] {
            let mut bindings = defaults.keybindings.bindings.clone();
            validate_conflicts(&mut bindings, &[false; Action::ALL.len()], mac).unwrap();
        }
    }
    #[test]
    fn project_shortcuts_replace_file_entry_points_and_migrate_existing_settings() {
        let defaults = Settings::default();
        assert_eq!(
            defaults.keybindings.hint(Action::NewProject),
            parse_chord("Primary+N").unwrap().label
        );
        assert_eq!(
            defaults.keybindings.hint(Action::OpenProject),
            parse_chord("Primary+O").unwrap().label
        );
        assert_eq!(
            defaults.keybindings.hint(Action::AddProjectFile),
            parse_chord("Primary+Shift+N").unwrap().label
        );
        let old = Settings::parse("[keybindings]\nopen=['F7']\nnew=['F8']\nnew_project=[]\nopen_project=[]\nset_main=[]\nsave_copy=[]\nengine_pdflatex=[]\nengine_xelatex=[]\nengine_lualatex=[]").unwrap();
        assert_eq!(old.keybindings.hint(Action::OpenProject), "F7");
        assert_eq!(old.keybindings.hint(Action::NewProject), "F8");
        let explicit =
            Settings::parse("[keybindings]\nopen=['F7']\nopen_project=['F9']\nnew_project=[]")
                .unwrap();
        assert_eq!(explicit.keybindings.hint(Action::OpenProject), "F9");
        assert!(explicit.keybindings.hint(Action::NewProject).is_empty());
    }
    #[test]
    fn build_timeout_can_be_extended_or_disabled_but_not_negative() {
        assert_eq!(
            Settings::parse("[build]\ntimeout_seconds=900")
                .unwrap()
                .build_timeout,
            std::time::Duration::from_secs(900)
        );
        assert_eq!(
            Settings::parse("[build]\ntimeout_seconds=0")
                .unwrap()
                .build_timeout,
            std::time::Duration::ZERO
        );
        assert!(Settings::parse("[build]\ntimeout_seconds=-1").is_err());
        assert!(Settings::parse("[build]\ntimeout_seconds=0.5").is_err());
    }
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
    };
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static SEQ: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "folio settings {} {}",
                std::process::id(),
                SEQ.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(path.join("bin")).unwrap();
            fs::create_dir_all(path.join("home/.config/folio")).unwrap();
            Self(path)
        }
        fn load(&self) -> Result<Loaded, String> {
            Settings::load_at(&self.0.join("bin/folio"), Some(&self.0.join("home")))
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn discovers_executable_then_home_then_defaults() {
        let fixture = Fixture::new();
        let defaults = fixture.load().unwrap();
        assert!(defaults.path.is_none());
        assert!(defaults.settings.editor.line_numbers);
        let home = fixture.0.join("home/.config/folio/folio.toml");
        fs::write(&home, "[build]\nengine = 'xelatex'").unwrap();
        let loaded = fixture.load().unwrap();
        assert_eq!(loaded.path, Some(home));
        assert_eq!(loaded.settings.engine, Engine::XeLatex);
        let local = fixture.0.join("bin/folio.toml");
        fs::write(&local, "[editor]\nfont_size = 18").unwrap();
        let loaded = fixture.load().unwrap();
        assert_eq!(loaded.path, Some(local));
        assert_eq!(loaded.settings.editor.font_size, 18.);
        assert_eq!(
            loaded.settings.engine,
            Engine::PdfLatex,
            "files do not merge with each other"
        );
        assert!(
            Settings::load_at(Path::new("/nonexistent/folio"), None)
                .unwrap()
                .path
                .is_none()
        );
    }
    #[test]
    fn invalid_or_unreadable_local_file_does_not_fall_through() {
        let fixture = Fixture::new();
        fs::write(
            fixture.0.join("home/.config/folio/folio.toml"),
            "[build]\nengine='xelatex'",
        )
        .unwrap();
        let local = fixture.0.join("bin/folio.toml");
        fs::write(&local, "[keybindings]\nsvae=['Primary+S']").unwrap();
        let error = fixture.load().unwrap_err();
        assert!(
            error.contains(&local.display().to_string()) && error.contains("svae"),
            "{error}"
        );
        fs::remove_file(&local).unwrap();
        fs::create_dir(&local).unwrap();
        assert!(
            fixture
                .load()
                .unwrap_err()
                .contains(&local.display().to_string())
        );
    }
    #[test]
    fn partial_settings_replace_bindings_and_validate_values() {
        let settings = Settings::parse("[keybindings]\nsave=['Alt+Shift+F6', 'Primary+Shift+S']\ngo=[]\n[editor]\nfont_size=18.5\n[mouse]\ngo='Alt+RightClick'\nzoom=[]").unwrap();
        assert_eq!(
            settings.keybindings.bindings[Action::Save as usize].len(),
            2
        );
        assert!(settings.keybindings.bindings[Action::Go as usize].is_empty());
        assert_eq!(settings.editor.font_size, 18.5);
        assert!(settings.editor.auto_complete);
        assert!(settings.editor.line_numbers);
        assert!(
            !Settings::parse("[editor]\nline_numbers=false")
                .unwrap()
                .editor
                .line_numbers
        );
        let mouse = settings.mouse.go.unwrap();
        assert_eq!(mouse.button, PointerButton::Secondary);
        assert_eq!(mouse.clicks, 1);
        assert!(settings.mouse.zoom.is_empty());
        assert!(
            Settings::parse("[mouse]\ngo='' ")
                .unwrap()
                .mouse
                .go
                .is_none()
        );
        Settings::parse(DEFAULT_TOML).unwrap();
    }
    #[test]
    fn reports_typos_bad_chords_conflicts_and_invalid_ranges() {
        for (source, field) in [
            ("[unknown]\nx=1", "unknown"),
            ("[keybindings]\nsave='F6'", "keybindings.save"),
            ("[keybindings]\nsave=['Hyper+S']", "keybindings.save"),
            (
                "[keybindings]\nsave=['Primary+NoSuchKey']",
                "keybindings.save",
            ),
            ("[keybindings]\nsave=['Primary+O']", "open"),
            ("[keybindings]\ncompletion_accept=['Primary+S']", "save"),
            ("[mouse]\ngo='DoubleClick'", "mouse.go"),
            ("[mouse]\nzoom=['']", "mouse.zoom"),
            ("[editor]\nfont_size=nan", "editor.font_size"),
            ("[preview]\ncache_mib=0", "preview.cache_mib"),
            ("[preview]\nzoom=20", "preview.zoom"),
            (
                "[preview]\nfit_on_switch='unknown'",
                "preview.fit_on_switch",
            ),
            ("[build]\nengine='tectonic'", "build.engine"),
            ("[theme]\nmode='sepia'", "theme.mode"),
            ("[window]\nwidth=0", "window.width"),
        ] {
            let error = Settings::parse(source).unwrap_err();
            assert!(error.contains(field), "{source}: {error}");
        }
        assert!(Settings::parse("[editor]\nfont_size=17\nfont_size=18").is_err());
    }

    #[test]
    fn theme_mode_defaults_to_dark_and_accepts_light_or_system() {
        use crate::theme::Mode;
        assert_eq!(Settings::default().theme, Mode::Dark);
        for (name, mode) in [
            ("dark", Mode::Dark),
            ("light", Mode::Light),
            ("system", Mode::System),
        ] {
            let settings = Settings::parse(&format!("[theme]\nmode='{name}'")).unwrap();
            assert_eq!(settings.theme, mode);
        }
        assert!(Settings::parse("[theme]\naccent='#fff'").is_err());
    }
    #[test]
    fn preview_accepts_explicit_switch_fitting_policies() {
        assert_eq!(
            Settings::default().preview.fit_on_switch,
            Some(PdfFit::Width)
        );
        for (mode, expected) in [
            ("page", Some(PdfFit::Page)),
            ("width", Some(PdfFit::Width)),
            ("keep", None),
        ] {
            let parsed = Settings::parse(&format!("[preview]\nfit_on_switch='{mode}'")).unwrap();
            assert_eq!(parsed.preview.fit_on_switch, expected);
        }
    }
    #[test]
    fn pdf_fitting_settings_support_width_fraction_and_double_press_bindings() {
        let settings = Settings::parse("[keybindings]\npdf_fit=['Space']\npdf_fit_page=['Space Space']\n[preview]\nwidth_fraction=0.8").unwrap();
        let page = Action::ALL
            .into_iter()
            .find(|a| a.name() == "pdf_fit_page")
            .unwrap();
        assert_eq!(settings.keybindings.hint(Action::PdfFit), "Space");
        assert_eq!(settings.keybindings.hint(page), "Space ×2");
        for config in [
            "[preview]\nwidth_fraction=0",
            "[preview]\nwidth_fraction=1.1",
            "[preview]\nwidth_fraction=nan",
            "[keybindings]\npdf_fit_page=['Space Enter']",
            "[keybindings]\npdf_fit_page=['Space Space Space']",
            "[keybindings]\nsave=['Space Space']",
            "[keybindings]\npdf_fit=[]\nsave=['Space']",
            "[keybindings]\npdf_fit_page=['Space']",
        ] {
            assert!(Settings::parse(config).is_err(), "{config}");
        }
        assert!(Settings::parse("[keybindings]\npdf_fit=[]\npdf_fit_page=[]").is_ok());
        assert!(
            Settings::parse("[keybindings]\npdf_fit=['F6']\npdf_fit_page=['F7 F7','Alt+P']")
                .is_ok()
        );
    }
    #[test]
    fn custom_chord_dispatch_is_exact_and_consumes_only_its_event() {
        let settings = Settings::parse("[keybindings]\nsave=['Alt+Shift+F6']\ngo=[]").unwrap();
        let ctx = egui::Context::default();
        let key = |key, modifiers| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        let _ = ctx.run(
            egui::RawInput {
                events: vec![
                    key(Key::S, Modifiers::COMMAND),
                    key(Key::F6, Modifiers::ALT),
                    key(Key::F6, Modifiers::ALT | Modifiers::SHIFT),
                    key(Key::G, Modifiers::COMMAND),
                ],
                ..Default::default()
            },
            |ctx| {
                assert!(settings.keybindings.consume(ctx, Action::Save));
                assert!(!settings.keybindings.consume(ctx, Action::Save));
                assert!(!settings.keybindings.consume(ctx, Action::Go));
                assert_eq!(ctx.input(|i| i.events.len()), 3);
            },
        );
    }
    #[test]
    fn a_settings_file_from_an_older_version_keeps_its_bindings_over_new_defaults() {
        // 0.9.5 shipped these; Primary+1..9 later became the tab defaults.
        let old =
            Settings::parse("[keybindings]\nsource=['Primary+1']\npreview=['Primary+2']").unwrap();
        assert_eq!(old.keybindings.chords(Action::Source)[0].key, Key::Num1);
        assert!(old.keybindings.chords(Action::Tab1).is_empty());
        assert!(old.keybindings.chords(Action::Tab2).is_empty());
        assert_eq!(old.keybindings.chords(Action::Tab3)[0].key, Key::Num3);
        // Two bindings the file itself gives one chord are still an error.
        let error = Settings::parse("[keybindings]\nsource=['Primary+1']\ntab_1=['Primary+1']")
            .unwrap_err();
        assert!(error.contains("conflicts"), "{error}");
        // Taking an established default's chord is still reported, not hidden.
        assert!(Settings::parse("[keybindings]\ntab_1=['Primary+S']").is_err());
    }
    #[test]
    fn folio_starts_in_full_screen_unless_the_settings_say_otherwise() {
        assert!(Settings::default().fullscreen);
        assert!(
            !Settings::parse("[window]\nfullscreen=false")
                .unwrap()
                .fullscreen
        );
        assert!(Settings::parse("[window]\nfullscreen='yes'").is_err());
    }
    #[test]
    fn the_action_table_is_indexed_consistently_and_names_are_unique() {
        // Bindings are stored by discriminant and filled by walking ALL.
        for (index, action) in Action::ALL.into_iter().enumerate() {
            assert_eq!(action as usize, index, "{}", action.name());
        }
        let mut names: Vec<&str> = Action::ALL.iter().map(|a| a.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), Action::ALL.len());
        // Only the nine tab actions report a position, in order.
        let tabs: Vec<(String, usize)> = Action::ALL
            .into_iter()
            .filter_map(|a| a.tab().map(|i| (a.name().to_owned(), i)))
            .collect();
        let expected: Vec<(String, usize)> =
            (0..9).map(|i| (format!("tab_{}", i + 1), i)).collect();
        assert_eq!(tabs, expected);
        let defaults = Settings::default();
        for (action, index) in Action::ALL
            .into_iter()
            .filter_map(|a| a.tab().map(|i| (a, i)))
        {
            let chords = defaults.keybindings.chords(action);
            assert_eq!(chords.len(), 1);
            assert_eq!(chords[0].key.name(), (index + 1).to_string());
            assert_eq!(chords[0].modifiers, Modifiers::COMMAND);
        }
        assert_eq!(
            defaults.keybindings.chords(Action::Source)[0].modifiers,
            Modifiers::ALT
        );
        assert_eq!(
            defaults.keybindings.chords(Action::Preview)[0].modifiers,
            Modifiers::ALT
        );
        assert_eq!(defaults.keybindings.chords(Action::Help)[0].key, Key::H);
    }
    #[test]
    fn the_settings_file_shipped_with_0_9_5_still_loads_and_keeps_its_choices() {
        // That file was a full copy of the defaults of its day: no tab
        // shortcuts, no help key, no [theme], no fullscreen, Cmd+1/2 for views.
        let old: String = DEFAULT_TOML
            .lines()
            .filter(|line| !line.starts_with("tab_") && !line.starts_with("fullscreen"))
            .map(|line| match line {
                l if l.starts_with("source = ") => "source = [\"Primary+1\"]".into(),
                l if l.starts_with("preview = ") => "preview = [\"Primary+2\"]".into(),
                l if l.starts_with("help = ") => "help = []".into(),
                l => l.to_owned(),
            })
            .collect::<Vec<_>>()
            .join("\n");
        let old = old.split("[theme]").next().unwrap().to_owned();
        assert!(!old.contains("tab_1") && !old.contains("[theme]"));
        let settings = Settings::parse(&old).expect("an old full settings file must load");
        let source = &settings.keybindings.chords(Action::Source)[0];
        assert_eq!(
            (source.key, source.modifiers),
            (Key::Num1, Modifiers::COMMAND)
        );
        assert!(settings.keybindings.chords(Action::Tab1).is_empty());
        assert!(settings.keybindings.chords(Action::Tab2).is_empty());
        assert_eq!(settings.keybindings.chords(Action::Tab9).len(), 1);
        assert!(settings.keybindings.chords(Action::Help).is_empty());
        assert!(
            settings.fullscreen,
            "a key the old file lacks takes the new default"
        );
    }
    #[test]
    fn option_swallowing_is_limited_to_the_chord_that_fired() {
        let settings =
            Settings::parse("[keybindings]\nparagraph_down=['Alt+Shift+D']\ngit=['Ctrl+Alt+G']")
                .unwrap();
        let key = |key, modifiers| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        let typed = |ctx: &egui::Context| {
            ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Text(_))))
        };
        let ctx = egui::Context::default();
        // Option held for a character that is not a shortcut: the text stays.
        let _ = ctx.run(
            egui::RawInput {
                events: vec![key(Key::E, Modifiers::ALT), egui::Event::Text("é".into())],
                ..Default::default()
            },
            |ctx| {
                assert!(!settings.keybindings.consume(ctx, Action::Source));
                assert!(!settings.keybindings.consume(ctx, Action::ParagraphDown));
                assert!(typed(ctx), "accented input must still be typed");
            },
        );
        // Option+Shift chord fires and takes its character with it.
        let _ = ctx.run(
            egui::RawInput {
                events: vec![
                    key(Key::D, Modifiers::ALT | Modifiers::SHIFT),
                    egui::Event::Text("Î".into()),
                ],
                ..Default::default()
            },
            |ctx| {
                assert!(settings.keybindings.consume(ctx, Action::ParagraphDown));
                assert!(!typed(ctx));
            },
        );
        // A released key is not a press.
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: Key::Num1,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: Modifiers::ALT,
                }],
                ..Default::default()
            },
            |ctx| assert!(!settings.keybindings.consume(ctx, Action::Source)),
        );
    }
    #[test]
    fn an_option_shortcut_swallows_the_character_macos_types_for_it() {
        let settings = Settings::default();
        let key = |key, modifiers| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        let text = |ctx: &egui::Context| {
            ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Text(_))))
        };
        let ctx = egui::Context::default();
        let _ = ctx.run(
            egui::RawInput {
                events: vec![
                    key(Key::Num2, Modifiers::ALT),
                    egui::Event::Text("™".into()),
                ],
                ..Default::default()
            },
            |ctx| {
                assert!(settings.keybindings.consume(ctx, Action::Preview));
                assert!(!text(ctx), "Option+2 must not also type a character");
            },
        );
        // Ordinary typing next to a non-Option shortcut is left alone.
        let _ = ctx.run(
            egui::RawInput {
                events: vec![
                    key(Key::S, Modifiers::COMMAND),
                    egui::Event::Text("a".into()),
                ],
                ..Default::default()
            },
            |ctx| {
                assert!(settings.keybindings.consume(ctx, Action::Save));
                assert!(text(ctx));
            },
        );
    }
    #[test]
    fn primary_alias_conflicts_are_checked_for_macos_and_linux() {
        let settings = Settings::default();
        for (mac, alias) in [(true, "Cmd+O"), (false, "Ctrl+O")] {
            let mut bindings = settings.keybindings.bindings.clone();
            bindings[Action::Save as usize] = vec![parse_chord(alias).unwrap()];
            assert!(
                validate_conflicts(&mut bindings, &[false; Action::ALL.len()], mac)
                    .unwrap_err()
                    .contains("save")
            );
        }
        let primary = parse_chord("Primary+S").unwrap().modifiers;
        assert!(!modifiers_match(primary | Modifiers::SHIFT, primary));
        assert!(!modifiers_match(primary | Modifiers::ALT, primary));
        #[cfg(target_os = "macos")]
        assert!(!modifiers_match(primary | Modifiers::CTRL, primary));
    }
    #[test]
    fn primary_shortcut_matches_native_platform_modifiers() {
        let settings = Settings::parse("").unwrap();
        let ctx = egui::Context::default();
        let modifiers = if cfg!(target_os = "macos") {
            Modifiers::MAC_CMD | Modifiers::COMMAND
        } else {
            Modifiers::CTRL | Modifiers::COMMAND
        };
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: Key::S,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                }],
                ..Default::default()
            },
            |ctx| assert!(settings.keybindings.consume(ctx, Action::Save)),
        );
    }
}

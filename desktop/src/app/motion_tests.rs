use super::*;

struct Editor {
    app: App,
    ctx: egui::Context,
    dir: PathBuf,
    time: f64,
}
impl Editor {
    fn new(name: &str, text: &str, settings: Settings) -> Self {
        let dir = std::env::temp_dir().join(format!("folio motions {name} {}", std::process::id()));
        ManagedProject::create(&dir, "Motion test", Engine::PdfLatex).unwrap();
        fs::write(dir.join("main.tex"), text).unwrap();
        let ctx = egui::Context::default();
        let app = App::with_settings(ctx.clone(), Some(dir.clone()), Arc::new(settings));
        let mut editor = Self {
            app,
            ctx,
            dir,
            time: 0.,
        };
        editor.place(0..0);
        editor
    }
    fn frame(&mut self, key: Option<(egui::Key, egui::Modifiers)>) {
        self.time += 0.1;
        let _ = self.ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800., 500.),
                )),
                time: Some(self.time),
                modifiers: key.map_or(egui::Modifiers::NONE, |(_, m)| m),
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
                self.app.handle_shortcuts(ctx);
                self.app.workspace_ui(ctx);
                self.app.completion_ui(ctx);
            },
        );
    }
    fn settle(&mut self) {
        for _ in 0..4 {
            self.frame(None);
        }
    }
    fn place(&mut self, range: std::ops::Range<usize>) {
        self.app.tabs[0].jump = Some(range);
        self.settle();
    }
    fn press(&mut self, key: egui::Key) {
        self.frame(Some((key, egui::Modifiers::COMMAND)));
    }
    fn source_id(&self) -> egui::Id {
        egui::Id::new(("source", &self.app.tabs[0].doc.path))
    }
    fn close(self) {
        drop(self.app);
        fs::remove_dir_all(self.dir).unwrap();
    }
}
fn before(text: &str, needle: &str) -> usize {
    text[..text.find(needle).unwrap()].chars().count()
}
fn after(text: &str, needle: &str) -> usize {
    before(text, needle) + needle.chars().count()
}

#[test]
fn prose_shortcuts_move_the_live_caret_without_changing_text() {
    let text = "α one. A second sentence.\nSoft line continues!\n\nβ paragraph.\n\nLast fragment";
    let mut editor = Editor::new("caret", text, Settings::default());
    for (key, expected) in [
        (egui::Key::J, after(text, "α one.")),
        (egui::Key::J, after(text, "sentence.")),
        (egui::Key::K, before(text, "A second")),
        (egui::Key::U, 0),
        (egui::Key::D, after(text, "continues!")),
        (egui::Key::D, after(text, "β paragraph.")),
        (egui::Key::U, before(text, "β")),
        (egui::Key::U, 0),
    ] {
        editor.press(key);
        let tab = &editor.app.tabs[0];
        assert_eq!(tab.cursor, expected, "Primary+{key:?}");
        assert_eq!(tab.selection, expected..expected);
        assert_eq!(tab.doc.text, text);
        assert_eq!(tab.revision, 0);
        assert!(!tab.doc.is_dirty());
        assert!(editor.ctx.memory(|m| m.has_focus(editor.source_id())));
        let state = egui::TextEdit::load_state(&editor.ctx, editor.source_id()).unwrap();
        assert_eq!(state.cursor.char_range().unwrap().primary.index, expected);
    }
    // A mouse selection is collapsed, and normal editing remains available.
    editor.place(1..4);
    editor.press(egui::Key::J);
    assert_eq!(editor.app.tabs[0].selection, 6..6);
    editor.frame(Some((egui::Key::ArrowLeft, egui::Modifiers::NONE)));
    assert_eq!(editor.app.tabs[0].cursor, 5);
    editor.app.tabs[0].doc.text = "\\s".into();
    editor.app.tabs[0].edited();
    editor.place(2..2);
    editor.app.complete();
    assert!(editor.app.completion.is_some());
    editor.press(egui::Key::U);
    assert!(editor.app.completion.is_none());
    assert_eq!(editor.app.tabs[0].cursor, 0);
    assert_eq!(editor.app.tabs[0].doc.text, "\\s");
    editor.close();
}

#[test]
fn prose_shortcuts_obey_settings_and_leave_other_surfaces_alone() {
    let settings =
        Settings::parse("[keybindings]\nparagraph_down=['Alt+D']\nsentence_next=[]").unwrap();
    let text = "First. Second.\n\nNext paragraph.";
    let mut editor = Editor::new("routing", text, settings);
    editor.press(egui::Key::D);
    editor.press(egui::Key::J);
    assert_eq!(
        editor.app.tabs[0].cursor, 0,
        "replaced and disabled defaults must stay disabled"
    );
    editor.frame(Some((egui::Key::D, egui::Modifiers::ALT)));
    assert_eq!(editor.app.tabs[0].cursor, after(text, "Second."));
    editor.app.perform_action(&editor.ctx, Action::Find);
    editor.settle();
    let find_focus = editor.ctx.memory(|m| m.focused());
    assert_ne!(find_focus, Some(editor.source_id()));
    editor.press(egui::Key::U);
    editor.frame(Some((egui::Key::D, egui::Modifiers::ALT)));
    assert_eq!(editor.app.tabs[0].cursor, after(text, "Second."));
    assert_eq!(editor.ctx.memory(|m| m.focused()), find_focus);
    editor.app.show_find = false;
    editor.place(0..0);
    editor.app.open_launcher(crate::launcher::Mode::Commands);
    editor.settle();
    editor.frame(Some((egui::Key::D, egui::Modifiers::ALT)));
    assert_eq!(editor.app.tabs[0].cursor, 0);
    // Palette invocation is deliberate and works despite its own search focus.
    editor.app.launcher.as_mut().unwrap().query = "Move to paragraph end".into();
    editor.frame(None);
    editor.frame(Some((egui::Key::Enter, egui::Modifiers::NONE)));
    editor.settle();
    assert!(editor.app.launcher.is_none());
    assert_eq!(editor.app.tabs[0].cursor, after(text, "Second."));
    editor.place(0..0);
    for dialog in [true, false] {
        editor.app.reload_dialog = dialog;
        editor.app.close_dialog = !dialog;
        editor.frame(Some((egui::Key::D, egui::Modifiers::ALT)));
        assert_eq!(
            editor.app.tabs[0].cursor, 0,
            "confirmation dialogs must block source motions"
        );
    }
    editor.app.reload_dialog = false;
    editor.app.close_dialog = false;
    editor.app.layers.layer = Layer::Preview;
    editor.frame(Some((egui::Key::D, egui::Modifiers::ALT)));
    assert_eq!(editor.app.layers.layer, Layer::Preview);
    assert_eq!(editor.app.tabs[0].cursor, 0);
    editor.app.tabs.clear();
    editor.app.active = 0;
    editor.app.layers.layer = Layer::Source;
    editor.frame(Some((egui::Key::D, egui::Modifiers::ALT)));
    editor.close();
}

#[test]
fn prose_motion_scrolls_only_when_destination_is_outside_the_view() {
    let text = format!(
        "{}\nMiddle one.\nMiddle two.\n\n{}",
        "Top line.\n".repeat(60),
        "Bottom line.\n".repeat(60)
    );
    let mut editor = Editor::new("scroll", &text, Settings::default());
    let middle = before(&text, "Middle one.");
    editor.place(middle..middle);
    let previous_y = editor.app.caret.y;
    editor.press(egui::Key::D);
    editor.settle();
    assert_eq!(editor.app.tabs[0].cursor, after(&text, "Middle two."));
    let line_height = theme::line_height(editor.app.settings.editor.font_size);
    assert!(
        (editor.app.caret.y - previous_y - line_height).abs() < 2.,
        "a visible destination must not recenter the viewport"
    );
    editor.place(0..0);
    editor.press(egui::Key::D);
    editor.settle();
    assert!(editor.app.tabs[0].cursor > 500);
    assert!(
        (70. ..480.).contains(&editor.app.caret.y),
        "offscreen caret must be revealed: {:?}",
        editor.app.caret
    );
    editor.press(egui::Key::U);
    editor.settle();
    assert_eq!(editor.app.tabs[0].cursor, 0);
    assert!((70. ..480.).contains(&editor.app.caret.y));
    editor.close();
}

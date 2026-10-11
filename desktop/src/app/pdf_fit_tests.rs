use super::*;

fn tap(key: egui::Key, modifiers: egui::Modifiers) -> Vec<egui::Event> {
    let mut events: Vec<_> = [true, false]
        .into_iter()
        .map(|pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        })
        .collect();
    if key == egui::Key::Space && modifiers.is_none() {
        events.push(egui::Event::Text(" ".into()));
    }
    events
}
fn space() -> Vec<egui::Event> {
    tap(egui::Key::Space, egui::Modifiers::NONE)
}
fn frame(
    app: &mut App,
    ctx: &egui::Context,
    time: f64,
    events: Vec<egui::Event>,
) -> Option<(egui::Rect, egui::Rect)> {
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000., 700.),
            )),
            time: Some(time),
            events,
            ..Default::default()
        },
        |ctx| {
            app.handle_shortcuts(ctx);
            app.workspace_ui(ctx);
        },
    );
    output
        .shapes
        .into_iter()
        .filter_map(|shape| match shape.shape {
            egui::Shape::Rect(rect) if rect.fill == egui::Color32::WHITE => {
                Some((rect.rect, shape.clip_rect))
            }
            _ => None,
        })
        .max_by(|(a, ac), (b, bc)| a.intersect(*ac).area().total_cmp(&b.intersect(*bc).area()))
        .map(|(page, _)| (page, app.preview.canvas))
}
fn viewer(settings: Settings) -> (egui::Context, App) {
    let ctx = egui::Context::default();
    let mut app = App::with_settings(ctx.clone(), None, Arc::new(settings));
    app.root = Some(PathBuf::from("paper/main.tex"));
    app.layers.built = Some(app.layers.revision);
    app.preview.set_test_pages(vec![(600., 1100.); 4]);
    app.enter_preview(&ctx, false);
    app.preview.command(&ctx, Action::PdfNext);
    frame(&mut app, &ctx, 1., vec![]);
    (ctx, app)
}
fn width((page, clip): (egui::Rect, egui::Rect), fraction: f32) {
    assert!(
        (page.width() / clip.width() - fraction).abs() < 0.001,
        "{page:?}, {clip:?}"
    );
    assert!((page.center().x - clip.center().x).abs() < 1.);
}
fn whole((page, clip): (egui::Rect, egui::Rect)) {
    assert!(
        clip.contains_rect(page),
        "page {page:?} must fit inside {clip:?}"
    );
    assert!(
        page.width() < clip.width() * 0.8,
        "portrait page must zoom out to fit its height"
    );
}

#[test]
fn width_fitting_opens_tall_pages_at_the_top_including_after_delayed_loading() {
    let ctx = egui::Context::default();
    let mut app = App::with_context(ctx.clone(), None);
    app.root = Some(PathBuf::from("paper/main.tex"));
    app.layers.built = Some(app.layers.revision);
    app.enter_preview(&ctx, false);
    assert!(frame(&mut app, &ctx, 1., vec![]).is_none());
    app.preview.set_test_pages(vec![(600., 1100.); 3]);
    let (page, canvas) = frame(&mut app, &ctx, 2., vec![]).unwrap();
    width((page, canvas), 0.9);
    assert!(
        page.top() >= canvas.top() && page.top() < canvas.top() + 20.,
        "new PDF must start at the top: {page:?}, {canvas:?}"
    );
    assert!(
        page.bottom() > canvas.bottom(),
        "width fit must not be height-limited"
    );
    app.preview.command(&ctx, Action::PdfNext);
    let (page, canvas) = frame(&mut app, &ctx, 3., vec![]).unwrap();
    assert_eq!(app.preview.page, 2);
    assert!(page.top() >= canvas.top() && page.top() < canvas.top() + 20.);
}

#[test]
fn space_fits_width_and_double_space_fits_page_without_rebuilding_or_changing_page() {
    let (ctx, mut app) = viewer(Settings::default());
    app.preview.command(&ctx, Action::PdfZoomIn);
    frame(&mut app, &ctx, 1.5, vec![]);
    width(frame(&mut app, &ctx, 2., space()).unwrap(), 0.9);
    whole(frame(&mut app, &ctx, 2.2, space()).unwrap());
    assert_eq!(app.preview.page, 2);
    // A third tap starts a new pair, and a late second tap is another single.
    let reading = frame(&mut app, &ctx, 2.3, space()).unwrap();
    width(reading, 0.9);
    assert!(
        reading.0.top() >= reading.1.top(),
        "leaving the overview must reveal the page's beginning, not its blank middle"
    );
    let mut held = space();
    held.retain(|e| !matches!(e, egui::Event::Key { pressed: false, .. }));
    width(frame(&mut app, &ctx, 2.8, held).unwrap(), 0.9);
    // Key repeat from holding the key cannot stand in for a second tap.
    let repeated = vec![egui::Event::Key {
        key: egui::Key::Space,
        physical_key: None,
        pressed: true,
        repeat: true,
        modifiers: egui::Modifiers::NONE,
    }];
    width(frame(&mut app, &ctx, 2.9, repeated).unwrap(), 0.9);
    frame(
        &mut app,
        &ctx,
        3.4,
        vec![egui::Event::Key {
            key: egui::Key::Space,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    let mut pair = space();
    pair.extend(space());
    whole(frame(&mut app, &ctx, 4., pair).unwrap());
    assert_eq!(app.preview.page, 2);
    assert!(!app.building);
}

#[test]
fn fit_taps_do_not_steal_search_or_launcher_spaces_or_cross_context_changes() {
    let (ctx, mut app) = viewer(Settings::default());
    frame(&mut app, &ctx, 2., space());
    app.perform_action(&ctx, Action::Find);
    frame(&mut app, &ctx, 2.05, vec![]);
    frame(&mut app, &ctx, 2.1, space());
    width(frame(&mut app, &ctx, 2.15, space()).unwrap(), 0.9);
    assert_eq!(app.preview.query, "  ");
    frame(
        &mut app,
        &ctx,
        2.2,
        tap(egui::Key::Escape, egui::Modifiers::NONE),
    );
    width(frame(&mut app, &ctx, 2.25, space()).unwrap(), 0.9);
    app.open_launcher(crate::launcher::Mode::Commands);
    frame(&mut app, &ctx, 2.3, vec![]);
    frame(&mut app, &ctx, 2.35, space());
    frame(&mut app, &ctx, 2.4, space());
    assert_eq!(app.launcher.as_ref().unwrap().query, "  ");
    frame(
        &mut app,
        &ctx,
        2.45,
        tap(egui::Key::Escape, egui::Modifiers::NONE),
    );
    width(frame(&mut app, &ctx, 2.5, space()).unwrap(), 0.9);
    frame(
        &mut app,
        &ctx,
        2.55,
        tap(egui::Key::A, egui::Modifiers::NONE),
    );
    width(frame(&mut app, &ctx, 2.6, space()).unwrap(), 0.9);
    app.close_dialog = true;
    frame(&mut app, &ctx, 2.65, space());
    app.close_dialog = false;
    width(frame(&mut app, &ctx, 2.7, space()).unwrap(), 0.9);
    frame(
        &mut app,
        &ctx,
        2.75,
        vec![egui::Event::WindowFocused(false)],
    );
    frame(&mut app, &ctx, 2.8, vec![egui::Event::WindowFocused(true)]);
    width(frame(&mut app, &ctx, 2.85, space()).unwrap(), 0.9);
    frame(
        &mut app,
        &ctx,
        2.9,
        tap(egui::Key::PageDown, egui::Modifiers::NONE),
    );
    width(frame(&mut app, &ctx, 2.95, space()).unwrap(), 0.9);
    assert_eq!(
        app.preview.page, 3,
        "another command must break a double-tap sequence"
    );
}

#[test]
fn pdf_fit_bindings_and_width_fraction_can_be_changed_or_disabled() {
    let settings = Settings::parse("[keybindings]\npdf_fit=['F6']\npdf_fit_page=['F7 F7','Alt+P']\n[preview]\nwidth_fraction=0.8").unwrap();
    let (ctx, mut app) = viewer(settings);
    app.preview.command(&ctx, Action::PdfZoomIn);
    let before = frame(&mut app, &ctx, 1.5, vec![]).unwrap().0;
    assert_eq!(frame(&mut app, &ctx, 2., space()).unwrap().0, before);
    width(
        frame(
            &mut app,
            &ctx,
            2.5,
            tap(egui::Key::F6, egui::Modifiers::NONE),
        )
        .unwrap(),
        0.8,
    );
    width(
        frame(
            &mut app,
            &ctx,
            3.,
            tap(egui::Key::F7, egui::Modifiers::NONE),
        )
        .unwrap(),
        0.8,
    );
    whole(
        frame(
            &mut app,
            &ctx,
            3.2,
            tap(egui::Key::F7, egui::Modifiers::NONE),
        )
        .unwrap(),
    );
    frame(
        &mut app,
        &ctx,
        4.,
        tap(egui::Key::F6, egui::Modifiers::NONE),
    );
    whole(frame(&mut app, &ctx, 5., tap(egui::Key::P, egui::Modifiers::ALT)).unwrap());
    let settings = Settings::parse("[keybindings]\npdf_fit=[]\npdf_fit_page=[]").unwrap();
    let (ctx, mut app) = viewer(settings);
    app.preview.command(&ctx, Action::PdfZoomIn);
    let before = frame(&mut app, &ctx, 1.5, vec![]).unwrap().0;
    frame(&mut app, &ctx, 2., space());
    assert_eq!(frame(&mut app, &ctx, 2.2, space()).unwrap().0, before);
}

#[test]
fn pdf_fit_space_binding_preserves_normal_source_typing() {
    let dir = std::env::temp_dir().join(format!("folio PDF space {}", std::process::id()));
    ManagedProject::create(&dir, "Space test", Engine::PdfLatex).unwrap();
    fs::write(dir.join("main.tex"), "Alpha").unwrap();
    let ctx = egui::Context::default();
    let mut app = App::with_context(ctx.clone(), Some(dir.clone()));
    app.tabs[0].jump = Some(0..0);
    frame(&mut app, &ctx, 1., vec![]);
    frame(&mut app, &ctx, 1.5, vec![]);
    frame(&mut app, &ctx, 2., space());
    frame(&mut app, &ctx, 2.2, space());
    assert_eq!(app.tabs[0].doc.text, "  Alpha");
    assert_eq!(app.layers.layer, Layer::Source);
    drop(app);
    fs::remove_dir_all(dir).unwrap();
}

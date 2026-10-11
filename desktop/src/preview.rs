use crate::page_layout::page_scale;
use crate::pdf_worker::{self, Event, Operation, Request};
use crate::render_queue::Task;
use crate::settings::{Action, PdfFit, Settings, modifiers_match};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, TextureHandle, Vec2};
use folio_core::{
    navigation::WordAnchor,
    pdf::{CacheBudget, PageText, PageTransform},
    synctex::PdfLocation,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    path::PathBuf,
};
mod fit_keys;

pub struct Preview {
    settings: Arc<Settings>,
    worker: pdf_worker::Worker,
    layout: crate::page_layout::Layout,
    requested: u64,
    generation: u64,
    pub path: Option<PathBuf>,
    next_path: Option<PathBuf>,
    sizes: Vec<(f32, f32)>,
    texts: Vec<Option<PageText>>,
    textures: HashMap<u64, TextureHandle>,
    fresh: HashSet<u64>,
    failed: HashSet<u64>,
    plan: Vec<Task>,
    desired: HashSet<usize>,
    view: Option<(Rect, u32)>,
    input_frame: Option<u64>,
    motion: Instant,
    budget: CacheBudget,
    pub error: Option<String>,
    pub page: usize,
    zoom: f32,
    fit: Option<PdfFit>,
    fit_keys: fit_keys::FitKeys,
    jump: Option<usize>,
    pub query: String,
    results: Vec<(usize, Range<usize>)>,
    match_index: usize,
    highlight: Option<PdfLocation>,
    selection: Option<(usize, usize, usize)>,
    clicked_point: Option<(usize, f32, f32)>,
    go_gesture: crate::navigation::GoGesture,
    find_visible: bool,
    focus_find: bool,
    pub indexed: usize,
    #[cfg(test)]
    page_rects: Vec<Rect>,
    #[cfg(test)]
    pub canvas: Rect,
}
fn key(page: usize, width: i32) -> u64 {
    ((page as u64) << 32) | width as u64
}
// Account for every resident page and the device's texture dimensions. Round
// heights upwards so PDFium's integer raster dimensions fit the byte allowance.
fn bounded_width((pw, ph): (f32, f32), requested: i32, bytes: usize, side: usize) -> i32 {
    let ratio = f64::from(ph.max(1.)) / f64::from(pw.max(1.));
    let (mut low, mut high) = (1, requested.max(1).min(side.max(1) as i32));
    while low < high {
        let mid = (low + high + 1) / 2;
        let height = (f64::from(mid) * ratio).ceil();
        if height <= side as f64 && f64::from(mid) * height * 4. <= bytes as f64 {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    low
}
fn dominant_page(pages: &[(usize, Rect)], clip: Rect) -> Option<usize> {
    pages
        .iter()
        .filter_map(|(page, rect)| {
            let visible = rect.intersect(clip);
            (visible.is_positive()).then_some((*page, visible.area()))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(page, _)| page)
}
impl Preview {
    #[cfg(test)]
    pub fn new(ctx: egui::Context) -> Self {
        Self::with_settings(ctx, Arc::new(Settings::default()))
    }
    pub fn with_settings(ctx: egui::Context, settings: Arc<Settings>) -> Self {
        Self {
            settings: settings.clone(),
            worker: pdf_worker::Worker::start(ctx),
            layout: Default::default(),
            requested: 0,
            generation: 0,
            path: None,
            next_path: None,
            sizes: vec![],
            texts: vec![],
            textures: HashMap::new(),
            fresh: HashSet::new(),
            failed: HashSet::new(),
            plan: vec![],
            desired: HashSet::new(),
            view: None,
            input_frame: None,
            motion: Instant::now(),
            budget: CacheBudget::new(settings.preview.cache_mib * 1024 * 1024),
            error: None,
            page: 1,
            zoom: settings.preview.zoom,
            fit: settings.preview.fit_width.then_some(PdfFit::Width),
            fit_keys: Default::default(),
            jump: None,
            query: String::new(),
            results: vec![],
            match_index: 0,
            highlight: None,
            selection: None,
            clicked_point: None,
            go_gesture: crate::navigation::GoGesture::new(settings.mouse.go.clone()),
            find_visible: false,
            focus_find: false,
            indexed: 0,
            #[cfg(test)]
            page_rects: vec![],
            #[cfg(test)]
            canvas: Rect::NOTHING,
        }
    }
    pub fn is_loading(&self) -> bool {
        self.next_path.is_some()
    }
    pub fn activate(&mut self) {
        self.fit_keys.reset();
        if let Some(fit) = self.settings.preview.fit_on_switch {
            self.fit = Some(fit);
            // Layout is deferred until all panels have claimed their space.
            // Retain this jump while the first PDF is still compiling/loading.
            self.jump = Some(self.page.saturating_sub(1));
        }
    }
    pub fn load_failed(&self) -> bool {
        self.requested != self.generation && !self.is_loading()
    }
    pub fn show_find(&mut self) {
        self.find_visible = true;
        self.focus_find = true;
    }
    pub fn hide_find(&mut self) {
        self.find_visible = false;
        self.focus_find = false;
        self.query.clear();
        self.search();
    }
    pub fn position_label(&self) -> String {
        if self.sizes.is_empty() {
            String::new()
        } else {
            format!("{} / {}", self.page, self.sizes.len())
        }
    }
    pub fn command(&mut self, ctx: &egui::Context, action: Action) {
        if !action.is_pdf_fit() {
            self.fit_keys.reset();
        }
        let page = self.page.saturating_sub(1);
        let shown_zoom = self
            .layout
            .pages
            .get(page)
            .zip(self.sizes.get(page))
            .map_or(self.zoom, |(rect, (width, _))| rect.width() / width.max(1.));
        match action {
            Action::PdfZoomIn => {
                self.fit = None;
                self.zoom = (shown_zoom * 1.2).min(4.);
            }
            Action::PdfZoomOut => {
                self.fit = None;
                self.zoom = (shown_zoom / 1.2).max(0.25);
            }
            Action::PdfFit => {
                if self.fit == Some(PdfFit::Page) {
                    // Return from the overview to readable content at the top
                    // of this page (or its explicit search/SyncTeX target).
                    self.jump = Some(page);
                }
                self.fit = Some(PdfFit::Width);
                self.zoom = 1.;
            }
            Action::PdfFitPage => {
                self.fit = Some(PdfFit::Page);
                self.zoom = 1.;
                self.jump = Some(page);
            }
            Action::PdfNext | Action::PdfPrevious if !self.sizes.is_empty() => {
                self.page = if action == Action::PdfNext {
                    (self.page + 1).min(self.sizes.len())
                } else {
                    self.page.saturating_sub(1).max(1)
                };
                self.jump = Some(self.page - 1);
            }
            _ => {}
        }
        ctx.request_repaint();
    }
    pub fn handle_fit_shortcuts(&mut self, ctx: &egui::Context, enabled: bool) {
        let actions = self.fit_keys.consume(
            ctx,
            &self.settings.keybindings,
            enabled && !self.focus_find && !ctx.wants_keyboard_input(),
        );
        for action in actions {
            self.command(ctx, action);
        }
    }
    pub fn clear(&mut self) {
        self.fit_keys.reset();
        self.requested += 1;
        self.generation = self.requested;
        self.path = None;
        self.next_path = None;
        self.sizes.clear();
        self.layout.clear();
        self.texts.clear();
        self.textures.clear();
        self.plan.clear();
        self.fresh.clear();
        self.failed.clear();
        self.results.clear();
        self.selection = None;
        self.clicked_point = None;
        self.highlight = None;
        self.jump = None;
        self.budget = CacheBudget::new(self.settings.preview.cache_mib * 1024 * 1024);
        self.indexed = 0;
        self.page = 1;
        self.error = None;
    }
    pub fn go_target(&self) -> Option<(usize, f32, f32)> {
        if let Some(point) = self.clicked_point {
            return Some(point);
        }
        if let Some((page, a, b)) = self.selection
            && let Some(text) = self.texts.get(page).and_then(Option::as_ref)
            && let Some(glyph) = text.glyphs.get(a.min(b))
        {
            return Some((
                page + 1,
                glyph.rect.x + glyph.rect.width / 2.,
                glyph.rect.y + glyph.rect.height / 2.,
            ));
        }
        self.clicked_point
    }
    pub fn go_word(&self) -> Option<WordAnchor> {
        let (page, a, b) = self.selection?;
        let text = self.texts.get(page)?.as_ref()?;
        WordAnchor::at(&text.text(), a.min(b))
    }
    fn point_word(&self, page: usize, x: f32, y: f32) -> Option<WordAnchor> {
        let text = self.texts.get(page)?.as_ref()?;
        WordAnchor::at(&text.text(), text.hit_test(x, y)?)
    }
    fn page_ready(&self, page: usize) -> bool {
        self.fresh.iter().any(|k| (*k >> 32) == page as u64)
            || !self.textures.keys().any(|k| (*k >> 32) == page as u64)
    }
    pub fn select_point(&mut self, page: usize, x: f32, y: f32) {
        if !self.page_ready(page) {
            return;
        }
        self.selection = self
            .point_word(page, x, y)
            .map(|word| (page, word.range.start, word.range.end - 1));
        self.clicked_point = Some((page + 1, x, y));
        self.highlight = None;
    }
    #[cfg(test)]
    pub fn navigation_highlight(&self) -> Option<&PdfLocation> {
        self.highlight.as_ref()
    }
    #[cfg(test)]
    pub fn set_test_pages(&mut self, sizes: Vec<(f32, f32)>) {
        // Geometry tests use the same layout/painting path without native PDF IO.
        let (tx, _) = std::sync::mpsc::channel();
        let (_, rx) = std::sync::mpsc::channel();
        self.worker = pdf_worker::Worker { tx, rx };
        self.texts = vec![None; sizes.len()];
        self.sizes = sizes;
        self.layout.clear();
        self.page = 1;
    }
    pub fn load(&mut self, path: PathBuf) {
        self.fit_keys.reset();
        self.requested += 1;
        self.next_path = Some(path.clone());
        self.error = None;
        if self
            .worker
            .tx
            .send(Request::Open {
                path,
                generation: self.requested,
            })
            .is_err()
        {
            self.error = Some("PDF worker unavailable. Install PDFium, then restart Folio.".into());
            self.next_path = None;
        }
    }
    pub fn jump(&mut self, location: PdfLocation) {
        self.page = location.page;
        self.jump = Some(location.page.saturating_sub(1));
        self.highlight = Some(location);
    }
    fn search(&mut self) {
        self.results = self
            .texts
            .iter()
            .enumerate()
            .flat_map(|(page, text)| {
                text.as_ref()
                    .map_or(vec![], |t| t.find(&self.query))
                    .into_iter()
                    .map(move |r| (page, r))
            })
            .collect();
        self.match_index = self.match_index.min(self.results.len().saturating_sub(1));
    }
    fn goto_match(&mut self) {
        if let Some((page, range)) = self.results.get(self.match_index) {
            self.page = page + 1;
            self.jump = Some(*page);
            if let Some(g) = self.texts[*page]
                .as_ref()
                .and_then(|t| t.glyphs.get(range.start))
            {
                self.highlight = Some(PdfLocation {
                    page: page + 1,
                    x: g.rect.x,
                    y: g.rect.y,
                    h: None,
                    v: None,
                    width: Some(g.rect.width),
                    height: Some(g.rect.height),
                });
            }
        }
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.worker.rx.try_recv() {
            match event {
                Event::Opened { generation, sizes } if generation == self.requested => {
                    self.generation = generation;
                    self.path = self.next_path.take();
                    self.texts = vec![None; sizes.len()];
                    self.sizes = sizes;
                    self.layout.clear();
                    // Retain the old pixels as a bounded fallback until fresh
                    // draft/full-resolution pages replace them after recompilation.
                    self.plan.clear();
                    self.fresh.clear();
                    self.failed.clear();
                    self.error = None;
                    self.indexed = 0;
                    self.results.clear();
                    self.selection = None;
                    self.clicked_point = None;
                    self.highlight = None;
                    self.page = self.page.clamp(1, self.sizes.len().max(1));
                    if self.fit == Some(PdfFit::Page) {
                        self.jump = Some(self.page - 1);
                    }
                }
                Event::Indexed {
                    generation,
                    page,
                    text,
                } if generation == self.generation => {
                    if page < self.texts.len() {
                        self.results.retain(|(p, _)| *p != page);
                        self.results
                            .extend(text.find(&self.query).into_iter().map(|r| (page, r)));
                        self.results.sort_by_key(|(p, r)| (*p, r.start));
                        if self.texts[page].is_none() {
                            self.indexed += 1;
                        }
                        self.texts[page] = Some(text);
                    }
                }
                Event::Rendered {
                    generation,
                    page,
                    width,
                    image,
                } if generation == self.generation => {
                    let k = key(page, width);
                    if self.fresh.contains(&k)
                        || (!self.desired.is_empty() && !self.desired.contains(&page))
                    {
                        continue;
                    }
                    let bytes = image.pixels.len() * 4;
                    // Retain one resolution per page. Otherwise the draft and
                    // refined texture compete for the same cache allocation.
                    let old: Vec<_> = self
                        .textures
                        .keys()
                        .copied()
                        .filter(|k| (*k >> 32) == page as u64)
                        .collect();
                    for k in old {
                        self.textures.remove(&k);
                        self.fresh.remove(&k);
                        self.budget.remove(k);
                    }
                    let handle = ctx.load_texture(
                        format!("pdf-{generation}-{k}"),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    self.textures.insert(k, handle);
                    self.fresh.insert(k);
                    for evicted in self.budget.insert(k, bytes) {
                        self.textures.remove(&evicted);
                        self.fresh.remove(&evicted);
                    }
                    // One upload per frame; conversion was already done on the worker.
                    ctx.request_repaint();
                    break;
                }
                Event::Error {
                    generation,
                    operation,
                    message,
                } if matches!(operation, Operation::Backend)
                    || (matches!(operation, Operation::Open) && generation == self.requested)
                    || (matches!(operation, Operation::Render(_) | Operation::Index(_))
                        && generation == self.generation) =>
                {
                    self.error = Some(match operation {
                        Operation::Render(task) => {
                            self.failed.insert(key(task.page, task.width));
                            format!("Page {} could not render: {message}", task.page + 1)
                        }
                        Operation::Index(page) => {
                            format!("Page {} text could not be indexed: {message}", page + 1)
                        }
                        _ => message,
                    });
                    self.plan.clear();
                    if matches!(operation, Operation::Backend | Operation::Open) {
                        self.next_path = None;
                    }
                }
                _ => {}
            }
        }
    }
    pub fn ui(&mut self, ui: &mut egui::Ui) -> bool {
        #[cfg(test)]
        self.page_rects.clear();
        if !ui.ctx().wants_keyboard_input()
            && ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Copy)))
            && let Some((page, a, b)) = self.selection
            && let Some(text) = &self.texts[page]
        {
            ui.ctx()
                .copy_text(text.selected_text(a.min(b)..a.max(b) + 1));
        }
        if self.find_visible {
            crate::theme::popup(ui.visuals())
                .inner_margin(10.)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let find_id = ui.make_persistent_id("pdf-find");
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
                        let search = ui.add(
                            egui::TextEdit::singleline(&mut self.query)
                                .id(find_id)
                                .return_key(None)
                                .frame(false)
                                .font(egui::FontId::proportional(14.))
                                .hint_text("Find in PDF…")
                                .desired_width(200.),
                        );
                        if search.changed() {
                            self.match_index = 0;
                            self.search();
                            self.goto_match();
                        }
                        ui.label(crate::theme::caption(
                            ui,
                            format!("{} matches", self.results.len()),
                        ));
                        crate::theme::search_hint(ui, &self.settings.keybindings);
                        if previous && !self.results.is_empty() {
                            self.match_index =
                                (self.match_index + self.results.len() - 1) % self.results.len();
                            self.goto_match();
                        }
                        if next && !self.results.is_empty() {
                            self.match_index = (self.match_index + 1) % self.results.len();
                            self.goto_match();
                        }
                    });
                });
            ui.add_space(12.);
        }
        if let Some(error) = &self.error {
            ui.colored_label(crate::theme::of(ui).error, error);
        }
        if self.sizes.is_empty() {
            ui.add_space(100.);
            ui.vertical_centered(|ui| {
                ui.weak("Your PDF will appear here after typesetting.");
            });
            return false;
        }
        let area = ui.available_rect_before_wrap().intersect(ui.clip_rect());
        #[cfg(test)]
        {
            self.canvas = area;
        }
        let mut shown_zoom =
            self.sizes
                .get(self.page.saturating_sub(1))
                .map_or(self.zoom, |&size| {
                    page_scale(
                        size,
                        area.size(),
                        self.fit,
                        self.zoom,
                        self.settings.preview.width_fraction,
                    )
                });
        let id = ui.make_persistent_id(egui::Id::new("pdf-scroll"));
        let mut offset = egui::scroll_area::State::load(ui.ctx(), id)
            .unwrap_or_default()
            .offset;
        let hovered = ui.rect_contains_pointer(area);
        let frame = ui.ctx().cumulative_frame_nr();
        let (scroll, magnify) = if self.input_frame != Some(frame) && hovered {
            // The OS already supplies smooth pixel deltas and momentum. Egui's
            // wheel filter adds a ~100 ms tail to larger trackpad movements, and
            // ScrollArea applies it after painting. Consume gestures here instead.
            let options = ui.ctx().options(|o| o.input_options);
            ui.input(|i| {
                let mut scroll = Vec2::ZERO;
                let mut magnify = 1.;
                for event in &i.events {
                    match event {
                        egui::Event::Zoom(factor) => magnify *= factor,
                        egui::Event::MouseWheel {
                            unit,
                            delta,
                            modifiers,
                        } => {
                            let mut delta = *delta
                                * match unit {
                                    egui::MouseWheelUnit::Point => 1.,
                                    egui::MouseWheelUnit::Line => options.line_scroll_speed,
                                    egui::MouseWheelUnit::Page => area.height(),
                                };
                            if self
                                .settings
                                .mouse
                                .zoom
                                .iter()
                                .any(|m| modifiers_match(*modifiers, *m))
                            {
                                magnify *= ((delta.x + delta.y) * options.scroll_zoom_speed).exp();
                            } else {
                                if self
                                    .settings
                                    .mouse
                                    .horizontal_scroll
                                    .iter()
                                    .any(|m| modifiers_match(*modifiers, *m))
                                {
                                    delta = Vec2::new(delta.x + delta.y, 0.);
                                }
                                scroll += delta;
                            }
                        }
                        _ => {}
                    }
                }
                (scroll, magnify)
            })
        } else {
            (Vec2::ZERO, 1.)
        };
        self.input_frame = Some(frame);
        if magnify != 1. {
            self.fit = None;
            self.zoom = (shown_zoom * magnify).clamp(0.25, 4.);
            shown_zoom = self.zoom;
        }
        let anchor = if magnify != 1. {
            ui.input(|i| i.pointer.hover_pos()).unwrap_or(area.center())
        } else {
            area.center()
        } - area.min;
        let document_point = offset + anchor;
        let anchor_page = self
            .layout
            .pages
            .partition_point(|r| r.bottom() < document_point.y)
            .min(self.layout.pages.len().saturating_sub(1));
        let anchored = self.layout.pages.get(anchor_page).map(|rect| {
            let page = anchor_page;
            (page, (document_point - rect.min.to_vec2()) / rect.size())
        });
        let changed = self.layout.update(
            &self.sizes,
            area.size(),
            self.fit,
            self.zoom,
            self.settings.preview.width_fraction,
        );
        if let Some((page, point)) = anchored
            && let Some(rect) = self.layout.pages.get(page)
        {
            offset = rect.min.to_vec2() + point * rect.size() - anchor;
        }
        offset -= scroll;
        if changed && self.fit == Some(PdfFit::Page) && self.jump.is_none() {
            // Keep a complete current page on screen when the browser, search
            // row, compiler panel or window changes the available canvas.
            self.jump = Some(self.page.saturating_sub(1));
        }
        if let Some(page) = self.jump.take()
            && let Some(rect) = self.layout.pages.get(page)
        {
            let point = self
                .highlight
                .as_ref()
                .filter(|h| h.page == page + 1 && self.fit != Some(PdfFit::Page))
                .map(|h| {
                    rect.min + Vec2::new(h.x, h.y) * (rect.width() / self.sizes[page].0.max(1.))
                })
                .unwrap_or_else(|| {
                    if self.fit == Some(PdfFit::Width) {
                        // Start a tall page at its top when entering/navigating
                        // the PDF; explicit text/SyncTeX targets still win.
                        Pos2::new(
                            rect.center().x,
                            rect.top()
                                + (rect.height() / 2.).min((area.height() / 2. - 12.).max(0.)),
                        )
                    } else {
                        rect.center()
                    }
                });
            offset = point.to_vec2() - area.size() / 2.;
        }
        offset = offset.clamp(Vec2::ZERO, (self.layout.size - area.size()).max(Vec2::ZERO));
        let mut go_requested = false;
        let navigation_pointer = self.go_gesture.modified_pointer(ui.ctx());
        egui::ScrollArea::both()
            .id_salt("pdf-scroll")
            .scroll_source(egui::scroll_area::ScrollSource::SCROLL_BAR)
            .scroll_offset(offset)
            .animated(false)
            .auto_shrink([false, false])
            .show_viewport(ui, |ui, viewport| {
                let origin = ui.min_rect().min.to_vec2();
                ui.set_min_size(self.layout.size);
                let view = (viewport, shown_zoom.to_bits());
                if self.view != Some(view) {
                    self.view = Some(view);
                    self.motion = Instant::now();
                }
                let settled = self.motion.elapsed() >= Duration::from_millis(120);
                if !settled {
                    ui.ctx().request_repaint_after(Duration::from_millis(120));
                }
                let visible = self.layout.visible(viewport, 32.);
                let nearby =
                    visible.start.saturating_sub(1)..(visible.end + 1).min(self.sizes.len());
                self.desired = nearby.clone().collect();
                let has_fresh = |page: usize| self.fresh.iter().any(|k| (*k >> 32) == page as u64);
                let allowance = (self.settings.preview.cache_mib * 1024 * 1024
                    / nearby.len().max(1))
                .min(32 * 1024 * 1024);
                let limit = ui.input(|i| i.max_texture_side).min(8192);
                let render_width = |page: usize, width: i32| {
                    bounded_width(self.sizes[page], width, allowance, limit)
                };
                let mut plan = Vec::new();
                // Drafts arrive quickly during motion. Sharp pixels follow only
                // once scrolling/zooming settles, with neighboring drafts last.
                for page in visible.clone() {
                    if !has_fresh(page) {
                        plan.push(Task {
                            page,
                            width: render_width(page, 768),
                        });
                    }
                }
                if settled {
                    for page in visible.clone() {
                        let width = (((self.layout.pages[page].width()
                            * ui.ctx().pixels_per_point())
                            / 64.)
                            .ceil() as i32
                            * 64)
                            .clamp(128, 4096);
                        let width = render_width(page, width);
                        if !self.fresh.contains(&key(page, width)) {
                            plan.push(Task { page, width });
                        }
                    }
                }
                for page in nearby {
                    if !visible.contains(&page) && !has_fresh(page) {
                        plan.push(Task {
                            page,
                            width: render_width(page, 768),
                        });
                    }
                }
                plan.retain(|task| !self.failed.contains(&key(task.page, task.width)));
                if plan != self.plan {
                    let _ = self.worker.tx.send(Request::Viewport {
                        generation: self.generation,
                        tasks: plan.clone(),
                    });
                    self.plan = plan;
                }
                let mut visible_pages = Vec::new();
                for page in self.layout.visible(viewport, 32.) {
                    let (pw, ph) = self.sizes[page];
                    let rect = self.layout.pages[page].translate(origin);
                    let display_width = rect.width();
                    let response = ui.interact(
                        rect,
                        ui.id().with(("pdf-page", page)),
                        Sense::click_and_drag(),
                    );
                    let transform = PageTransform {
                        origin_x: rect.left(),
                        origin_y: rect.top(),
                        display_width,
                        page_width: pw,
                        page_height: ph,
                    };
                    #[cfg(test)]
                    self.page_rects.push(rect);
                    visible_pages.push((page + 1, rect));
                    ui.painter().rect_filled(
                        rect.translate(Vec2::new(2., 3.)),
                        2.,
                        Color32::from_black_alpha(60),
                    );
                    ui.painter().rect_filled(rect, 1., Color32::WHITE);
                    if let Some((&k, texture)) = self
                        .textures
                        .iter()
                        .filter(|(k, _)| (**k >> 32) == page as u64)
                        .max_by_key(|(k, _)| (self.fresh.contains(k), **k as u32))
                    {
                        self.budget.touch(k);
                        ui.painter().image(
                            texture.id(),
                            rect,
                            Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                            Color32::WHITE,
                        );
                    }
                    let paint_box = |r: folio_core::pdf::Rect, color: Color32| {
                        let Some((x, y)) = transform.to_screen(r.x, r.y) else {
                            return;
                        };
                        let Some(scale) = transform.scale() else {
                            return;
                        };
                        if !r.is_valid() {
                            return;
                        }
                        ui.painter().rect_filled(
                            Rect::from_min_size(
                                Pos2::new(x, y),
                                Vec2::new(r.width * scale, r.height * scale),
                            ),
                            1.,
                            color,
                        );
                    };
                    if let Some(text) = &self.texts[page]
                        && self.page_ready(page)
                    {
                        for (_, range) in self.results.iter().filter(|(p, _)| *p == page) {
                            for glyph in &text.glyphs[range.clone()] {
                                paint_box(
                                    glyph.rect,
                                    Color32::from_rgba_unmultiplied(255, 197, 66, 85),
                                );
                            }
                        }
                        if let Some((p, a, b)) = self.selection
                            && p == page
                        {
                            for g in text.glyphs.iter().skip(a.min(b)).take(a.abs_diff(b) + 1) {
                                paint_box(
                                    g.rect,
                                    Color32::from_rgba_unmultiplied(70, 140, 240, 80),
                                );
                            }
                        }
                        if let Some(pos) = response.interact_pointer_pos()
                            && !navigation_pointer
                            && let Some((x, y)) = transform.to_page(pos.x, pos.y)
                        {
                            if response.drag_started()
                                && let Some(origin) = ui.input(|i| i.pointer.press_origin())
                                && let Some((sx, sy)) = transform.to_page(origin.x, origin.y)
                                && let Some(i) = text.nearest(sx, sy)
                            {
                                self.selection = Some((page, i, i));
                                self.clicked_point = None;
                            }
                            if response.dragged()
                                && let Some((p, a, _)) = self.selection
                                && p == page
                                && let Some(b) = text.nearest(x, y)
                            {
                                self.selection = Some((p, a, b));
                            }
                        }
                    }
                    if let Some(h) = &self.highlight
                        && h.page == page + 1
                    {
                        paint_box(h.rect(), Color32::from_rgba_unmultiplied(83, 191, 159, 65));
                    }
                    if self.go_gesture.clicked(&response) && self.page_ready(page) {
                        if let Some(pos) = response.interact_pointer_pos()
                            && let Some((x, y)) = transform.to_page(pos.x, pos.y)
                            && (self.point_word(page, x, y).is_some() || self.go_target().is_none())
                        {
                            self.select_point(page, x, y);
                        }
                        go_requested = self.go_target().is_some();
                    }
                    if response.clicked_by(egui::PointerButton::Primary)
                        && !navigation_pointer
                        && let Some(pos) = response.interact_pointer_pos()
                        && let Some((x, y)) = transform.to_page(pos.x, pos.y)
                    {
                        self.select_point(page, x, y);
                    }
                }
                if let Some(page) = dominant_page(&visible_pages, ui.clip_rect()) {
                    self.page = page;
                }
            });
        go_requested
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pdf_search_previous_wraps_and_uses_the_configured_binding() {
        let (ctx, mut preview) = input_fixture_with_settings(
            Settings::parse("[keybindings]\nfind_previous=['F10']").unwrap(),
        );
        preview.results = vec![(0, 0..1), (2, 0..1)];
        preview.show_find();
        input_frame(&ctx, &mut preview, vec![]);
        let key = |key, modifiers| {
            vec![egui::Event::Key {
                key,
                modifiers,
                physical_key: None,
                pressed: true,
                repeat: false,
            }]
        };
        input_frame(
            &ctx,
            &mut preview,
            key(egui::Key::Enter, egui::Modifiers::SHIFT),
        );
        assert_eq!(
            preview.match_index, 0,
            "the replaced default must be inactive"
        );
        input_frame(
            &ctx,
            &mut preview,
            key(egui::Key::F10, egui::Modifiers::NONE),
        );
        assert_eq!(preview.match_index, 1);
        assert_eq!(preview.page, 3);
        input_frame(
            &ctx,
            &mut preview,
            key(egui::Key::F10, egui::Modifiers::NONE),
        );
        assert_eq!(preview.match_index, 0);
        assert_eq!(preview.page, 1);
    }

    #[test]
    fn toolbar_free_pdf_keeps_page_zoom_and_copy_commands() {
        let (ctx, mut preview) = input_fixture();
        assert_eq!(Preview::new(ctx.clone()).position_label(), "");
        let before = preview.page_rects[0].width();
        preview.command(&ctx, Action::PdfZoomIn);
        input_frame(&ctx, &mut preview, vec![]);
        assert!((preview.page_rects[0].width() / before - 1.2).abs() < 0.001);
        preview.command(&ctx, Action::PdfFit);
        input_frame(&ctx, &mut preview, vec![]);
        assert!((preview.page_rects[0].width() - before).abs() < 0.001);
        preview.command(&ctx, Action::PdfNext);
        input_frame(&ctx, &mut preview, vec![]);
        assert_eq!(preview.position_label(), "2 / 3");
        preview.command(&ctx, Action::PdfPrevious);
        input_frame(&ctx, &mut preview, vec![]);
        assert_eq!(preview.position_label(), "1 / 3");
        preview.texts[0] = Some(PageText {
            width: 600.,
            height: 800.,
            glyphs: "ABC"
                .chars()
                .enumerate()
                .map(|(i, ch)| folio_core::pdf::Glyph {
                    ch,
                    rect: folio_core::pdf::Rect {
                        x: i as f32 * 12.,
                        y: 20.,
                        width: 12.,
                        height: 16.,
                    },
                })
                .collect(),
        });
        preview.selection = Some((0, 0, 1));
        let output = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Copy],
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    preview.ui(ui);
                });
            },
        );
        assert!(
            output
                .platform_output
                .commands
                .iter()
                .any(|c| matches!(c, egui::OutputCommand::CopyText(text) if text == "AB"))
        );
        for shape in output.shapes {
            if let egui::epaint::Shape::Text(text) = shape.shape {
                assert!(
                    !["Fit width", "Copy", "Find", "‹", "›"]
                        .contains(&text.galley.job.text.as_str()),
                    "PDF toolbar reappeared"
                );
            }
        }
    }
    fn input_frame(ctx: &egui::Context, preview: &mut Preview, events: Vec<egui::Event>) -> bool {
        let mut go = false;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 900.))),
                max_texture_side: Some(8192),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| go |= preview.ui(ui));
            },
        );
        go
    }
    fn input_fixture() -> (egui::Context, Preview) {
        input_fixture_with_settings(Settings::default())
    }
    #[test]
    fn switch_fitting_can_fill_width_or_preserve_manual_zoom() {
        for policy in ["keep", "width"] {
            let settings = Settings::parse(&format!(
                "[preview]\nfit_on_switch='{policy}'\nfit_width=false\nzoom=1.5"
            ))
            .unwrap();
            let (ctx, mut preview) = input_fixture_with_settings(settings);
            let before = preview.page_rects[0];
            preview.activate();
            input_frame(&ctx, &mut preview, vec![]);
            let after = preview.page_rects[0];
            if policy == "keep" {
                assert_eq!(
                    after, before,
                    "opt-out must preserve zoom and reading position"
                );
            } else {
                assert!(after.left() >= 8. && after.right() <= 792.);
                assert!(after.width() > 700., "width fit should fill the canvas");
                assert!(
                    after.height() > 900.,
                    "width fit need not fit the page height"
                );
            }
        }
    }

    #[test]
    fn page_fit_survives_delayed_loading_mixed_sizes_and_synctex_jumps() {
        let ctx = egui::Context::default();
        let mut preview = Preview::with_settings(
            ctx.clone(),
            Arc::new(Settings::parse("[preview]\nfit_on_switch='page'").unwrap()),
        );
        let (requests, _rx) = std::sync::mpsc::channel();
        let (tx, events) = std::sync::mpsc::channel();
        preview.worker = pdf_worker::Worker {
            tx: requests,
            rx: events,
        };
        preview.requested = 1;
        preview.next_path = Some("paper.pdf".into());
        preview.activate();
        input_frame(&ctx, &mut preview, vec![]);
        assert!(preview.page_rects.is_empty());
        tx.send(Event::Opened {
            generation: 1,
            sizes: vec![(600., 800.), (1000., 600.), (420., 1200.)],
        })
        .unwrap();
        preview.poll(&ctx);
        let canvas = Rect::from_min_max(Pos2::new(8., 8.), Pos2::new(792., 892.));
        for page in 1..=3 {
            preview.jump(PdfLocation {
                page,
                x: 100.,
                y: 500.,
                h: None,
                v: None,
                width: Some(30.),
                height: Some(15.),
            });
            input_frame(&ctx, &mut preview, vec![]);
            assert_eq!(preview.page, page);
            assert!(
                preview.page_rects.iter().any(|r| canvas.contains_rect(*r)),
                "the destination's entire page must be visible: {:?}",
                preview.page_rects
            );
            assert_eq!(preview.navigation_highlight().unwrap().page, page);
        }
        preview.requested = 2;
        preview.next_path = Some("rebuilt.pdf".into());
        tx.send(Event::Opened {
            generation: 2,
            sizes: vec![(1200., 500.)],
        })
        .unwrap();
        preview.poll(&ctx);
        input_frame(&ctx, &mut preview, vec![]);
        assert_eq!(preview.page, 1);
        assert!(canvas.contains_rect(preview.page_rects[0]));
    }
    #[test]
    fn a_preview_error_does_not_block_other_pages_or_the_retained_document() {
        let (ctx, mut preview) = input_fixture();
        let (requests, rx) = std::sync::mpsc::channel();
        let (tx, events) = std::sync::mpsc::channel();
        preview.worker = pdf_worker::Worker {
            tx: requests,
            rx: events,
        };
        preview.path = Some("good.pdf".into());
        preview.requested = 1;
        preview.next_path = Some("broken.pdf".into());
        tx.send(Event::Error {
            generation: 1,
            operation: Operation::Open,
            message: "Cannot open PDF".into(),
        })
        .unwrap();
        preview.poll(&ctx);
        preview.jump = Some(2);
        input_frame(&ctx, &mut preview, vec![]);
        assert_eq!(
            preview.path.as_deref(),
            Some(std::path::Path::new("good.pdf"))
        );
        assert!(!preview.is_loading());
        assert!(rx.try_iter().any(|r| matches!(r, Request::Viewport { generation: 0, tasks } if tasks.iter().any(|t| t.page == 2))), "the last good PDF must still render after an open failure");
        tx.send(Event::Error {
            generation: 0,
            operation: Operation::Render(Task {
                page: 2,
                width: 768,
            }),
            message: "Bad page".into(),
        })
        .unwrap();
        tx.send(Event::Error {
            generation: 0,
            operation: Operation::Index(2),
            message: "Bad text".into(),
        })
        .unwrap();
        preview.poll(&ctx);
        preview.jump = Some(0);
        input_frame(&ctx, &mut preview, vec![]);
        let plans: Vec<_> = rx
            .try_iter()
            .filter_map(|r| match r {
                Request::Viewport { tasks, .. } => Some(tasks),
                _ => None,
            })
            .flatten()
            .collect();
        assert!(plans.iter().any(|t| t.page == 0));
        assert!(
            !plans.contains(&Task {
                page: 2,
                width: 768
            }),
            "failed renders must not spin in a retry loop"
        );
    }
    #[test]
    fn stale_page_errors_cannot_cancel_a_newer_pdf_load() {
        let (ctx, mut preview) = input_fixture();
        let (requests, _rx) = std::sync::mpsc::channel();
        let (tx, events) = std::sync::mpsc::channel();
        preview.worker = pdf_worker::Worker {
            tx: requests,
            rx: events,
        };
        preview.generation = 1;
        preview.requested = 2;
        preview.next_path = Some("latest.pdf".into());
        for (generation, operation) in [
            (
                0,
                Operation::Render(Task {
                    page: 0,
                    width: 768,
                }),
            ),
            (1, Operation::Open),
        ] {
            tx.send(Event::Error {
                generation,
                operation,
                message: "Stale error".into(),
            })
            .unwrap();
        }
        preview.poll(&ctx);
        assert!(preview.is_loading());
        assert!(preview.error.is_none());
    }
    #[test]
    fn a_small_texture_budget_converges_instead_of_rendering_forever() {
        let (ctx, mut preview) =
            input_fixture_with_settings(Settings::parse("[preview]\ncache_mib=32").unwrap());
        ctx.set_pixels_per_point(4.);
        let (requests, rx) = std::sync::mpsc::channel();
        let (tx, events) = std::sync::mpsc::channel();
        preview.worker = pdf_worker::Worker {
            tx: requests,
            rx: events,
        };
        preview.plan.clear();
        input_frame(&ctx, &mut preview, vec![]);
        let mut renders = 0;
        for _ in 0..12 {
            preview.motion = Instant::now() - Duration::from_secs(1);
            input_frame(&ctx, &mut preview, vec![]);
            let plan = rx
                .try_iter()
                .filter_map(|r| match r {
                    Request::Viewport { tasks, .. } => Some(tasks),
                    _ => None,
                })
                .last()
                .unwrap_or_default();
            if plan.is_empty() {
                break;
            }
            for task in plan {
                renders += 1;
                let width = task.width as usize;
                let height = (width as f32 * 800. / 600.).ceil() as usize;
                tx.send(Event::Rendered {
                    generation: 0,
                    page: task.page,
                    width: task.width,
                    image: egui::ColorImage::filled([width, height], Color32::WHITE),
                })
                .unwrap();
                preview.poll(&ctx);
            }
        }
        assert!(
            renders < 12,
            "cache churn rendered {renders} pages for an unmoving viewport"
        );
        assert!(!preview.fresh.is_empty());
        assert!(preview.budget.used() <= 32 * 1024 * 1024);
    }
    #[test]
    fn rebuild_with_fewer_pages_while_scrolled_down_keeps_a_valid_view() {
        let (ctx, mut preview) = input_fixture();
        let (requests, _rx) = std::sync::mpsc::channel();
        let (tx, events) = std::sync::mpsc::channel();
        preview.worker = pdf_worker::Worker {
            tx: requests,
            rx: events,
        };
        preview.sizes = vec![(600., 800.); 10];
        preview.texts = vec![None; 10];
        preview.layout.clear();
        preview.jump = Some(7);
        input_frame(&ctx, &mut preview, vec![]);
        assert_eq!(preview.page, 8);
        preview.requested = 1;
        preview.next_path = Some("shorter.pdf".into());
        tx.send(Event::Opened {
            generation: 1,
            sizes: vec![(600., 800.); 5],
        })
        .unwrap();
        preview.poll(&ctx);
        input_frame(&ctx, &mut preview, vec![]);
        assert!(preview.page <= 5);
        assert!(!preview.page_rects.is_empty());
        tx.send(Event::Opened {
            generation: 1,
            sizes: vec![],
        })
        .unwrap();
        preview.poll(&ctx);
        input_frame(&ctx, &mut preview, vec![]);
        assert!(preview.layout.pages.is_empty());
    }
    fn input_fixture_with_settings(settings: Settings) -> (egui::Context, Preview) {
        let ctx = egui::Context::default();
        let mut preview = Preview::with_settings(ctx.clone(), Arc::new(settings));
        preview.sizes = vec![(600., 800.); 3];
        preview.texts = vec![None; 3];
        preview.indexed = 3;
        input_frame(&ctx, &mut preview, vec![]);
        input_frame(&ctx, &mut preview, vec![]);
        (ctx, preview)
    }
    #[test]
    fn navigation_click_targets_the_word_under_the_pointer() {
        for (config, modifiers) in [
            ("", egui::Modifiers::COMMAND),
            ("[mouse]\ngo='Alt+DoubleClick'", egui::Modifiers::ALT),
        ] {
            let (ctx, mut preview) = input_fixture_with_settings(Settings::parse(config).unwrap());
            preview.texts[0] = Some(PageText {
                width: 600.,
                height: 800.,
                glyphs: "one two"
                    .chars()
                    .enumerate()
                    .map(|(i, ch)| folio_core::pdf::Glyph {
                        ch,
                        rect: folio_core::pdf::Rect {
                            x: 50. + i as f32 * 12.,
                            y: 50.,
                            width: 12.,
                            height: 16.,
                        },
                    })
                    .collect(),
            });
            preview.selection = Some((0, 0, 2));
            let page = preview.page_rects[0];
            let pos = page.min + Vec2::new(116., 58.) * (page.width() / 600.);
            input_frame(&ctx, &mut preview, vec![egui::Event::PointerMoved(pos)]);
            for pressed in [true, false] {
                assert!(!input_frame(
                    &ctx,
                    &mut preview,
                    vec![egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers,
                    }]
                ));
            }
            assert_eq!(
                preview.selection,
                Some((0, 0, 2)),
                "the first Cmd-click must preserve the selected range"
            );
            for pressed in [true, false] {
                assert_eq!(
                    input_frame(
                        &ctx,
                        &mut preview,
                        vec![egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers,
                        }]
                    ),
                    !pressed
                );
            }
            let (page, x, y) = preview.go_target().unwrap();
            assert_eq!(page, 1);
            assert!(
                (x - 116.).abs() < 0.001 && (y - 58.).abs() < 0.001,
                "the completed gesture targets the clicked word, not an old selection"
            );
            assert_eq!(preview.selection, Some((0, 4, 6)));
        }
    }
    #[test]
    fn plain_pdf_click_selects_a_word_and_preserves_its_clicked_coordinate() {
        let (ctx, mut preview) = input_fixture();
        preview.texts[0] = Some(PageText {
            width: 600.,
            height: 800.,
            glyphs: "one two."
                .chars()
                .enumerate()
                .map(|(i, ch)| folio_core::pdf::Glyph {
                    ch,
                    rect: folio_core::pdf::Rect {
                        x: 50. + i as f32 * 12.,
                        y: 50.,
                        width: 12.,
                        height: 16.,
                    },
                })
                .collect(),
        });
        let rect = preview.page_rects[0];
        let pos = rect.min + Vec2::new(116., 58.) * (rect.width() / 600.);
        input_frame(&ctx, &mut preview, vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            assert!(
                !input_frame(
                    &ctx,
                    &mut preview,
                    vec![egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    }]
                ),
                "plain clicks do not switch layers"
            );
        }
        assert_eq!(preview.selection, Some((0, 4, 6)));
        let (page, x, y) = preview.go_target().unwrap();
        assert_eq!(page, 1);
        assert!((x - 116.).abs() < 0.001 && (y - 58.).abs() < 0.001);
    }
    #[test]
    fn trackpad_scroll_is_drawn_in_the_input_frame_without_an_added_tail() {
        let (ctx, mut preview) = input_fixture();
        let initial = preview.page_rects[0].top();
        input_frame(
            &ctx,
            &mut preview,
            vec![
                egui::Event::PointerMoved(Pos2::new(400., 300.)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: Vec2::new(0., -120.),
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let moved = initial - preview.page_rects[0].top();
        assert!(
            (moved - 120.).abs() < 0.1,
            "first frame moved {moved}, expected 120 points"
        );
        for _ in 0..12 {
            input_frame(&ctx, &mut preview, vec![]);
            assert!(
                (initial - preview.page_rects[0].top() - 120.).abs() < 0.1,
                "the OS already supplies trackpad momentum; do not add another tail: {}",
                initial - preview.page_rects[0].top()
            );
        }
    }
    #[test]
    fn pinch_zoom_is_immediate_and_keeps_the_pdf_point_under_the_pointer() {
        let (ctx, mut preview) = input_fixture();
        let pointer = Pos2::new(400., 300.);
        let before = preview.page_rects[0];
        let point = (pointer - before.min) / before.size();
        input_frame(
            &ctx,
            &mut preview,
            vec![egui::Event::PointerMoved(pointer), egui::Event::Zoom(1.2)],
        );
        let after = preview.page_rects[0];
        assert!(
            (after.width() / before.width() - 1.2).abs() < 0.001,
            "pinch must scale the PDF in the input frame"
        );
        // ScrollArea rounds its origin to physical pixels.
        assert!(
            (after.min + point * after.size() - pointer).length() <= 1. / ctx.pixels_per_point(),
            "zoom must preserve the reading position: before {before:?}, after {after:?}, error {:?}",
            after.min + point * after.size() - pointer
        );
        assert_eq!(
            ctx.zoom_factor(),
            1.,
            "zoom the PDF, not the application chrome"
        );
    }
    #[test]
    fn configured_zoom_and_cache_settings_reach_the_viewer() {
        let settings = Settings::parse("[preview]\nfit_width=false\nzoom=1.5\ncache_mib=32\n[mouse]\nzoom=['Alt']\nhorizontal_scroll=['Alt+Shift']").unwrap();
        let (ctx, mut preview) = input_fixture_with_settings(settings);
        assert_eq!(preview.fit, None);
        assert_eq!(preview.zoom, 1.5);
        let wheel = |modifiers| egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: Vec2::new(0., -40.),
            modifiers,
        };
        input_frame(
            &ctx,
            &mut preview,
            vec![
                egui::Event::PointerMoved(Pos2::new(400., 300.)),
                wheel(egui::Modifiers::CTRL),
            ],
        );
        assert_eq!(preview.zoom, 1.5, "replaced Ctrl wheel must not zoom");
        let before = preview.page_rects[0];
        input_frame(
            &ctx,
            &mut preview,
            vec![wheel(egui::Modifiers::ALT | egui::Modifiers::SHIFT)],
        );
        let after = preview.page_rects[0];
        assert!((after.left() - before.left() + 40.).abs() <= 1.);
        assert_eq!(preview.zoom, 1.5);
        input_frame(&ctx, &mut preview, vec![wheel(egui::Modifiers::ALT)]);
        assert!((preview.zoom - 1.5 * (-0.2_f32).exp()).abs() < 0.001);
        for _ in 0..2 {
            assert!(preview.budget.insert(42, 20 * 1024 * 1024).is_empty());
            assert_eq!(preview.budget.insert(43, 20 * 1024 * 1024), [42]);
            preview.clear();
        }
    }
    #[test]
    fn control_wheel_zooms_the_pdf_and_input_outside_the_canvas_is_ignored() {
        let (ctx, mut preview) = input_fixture();
        let before = preview.page_rects[0];
        let wheel = egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: Vec2::new(0., 40.),
            modifiers: egui::Modifiers::CTRL,
        };
        input_frame(
            &ctx,
            &mut preview,
            vec![
                egui::Event::PointerMoved(Pos2::new(400., 300.)),
                wheel.clone(),
            ],
        );
        let after = preview.page_rects[0];
        assert!((after.width() / before.width() - 0.2_f32.exp()).abs() < 0.001);
        input_frame(
            &ctx,
            &mut preview,
            vec![egui::Event::PointerMoved(Pos2::new(400., -15.)), wheel],
        );
        assert_eq!(preview.page_rects[0].size(), after.size());
    }
    #[test]
    fn synctex_jump_draws_the_destination_in_the_same_frame() {
        let (ctx, mut preview) = input_fixture();
        preview.jump(PdfLocation {
            page: 3,
            x: 100.,
            y: 300.,
            h: None,
            v: None,
            width: Some(80.),
            height: Some(20.),
        });
        input_frame(&ctx, &mut preview, vec![]);
        assert_eq!(preview.page, 3);
        let page = *preview.page_rects.last().unwrap();
        let point = page.min + Vec2::new(100., 300.) * (page.width() / 600.);
        assert!(
            (50.0..850.0).contains(&point.y),
            "jump target must already be on screen"
        );
    }
    #[test]
    fn drafts_refine_after_motion_and_cached_pages_are_not_rendered_again() {
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(2.);
        let mut preview = Preview::new(ctx.clone());
        let (requests, rx) = std::sync::mpsc::channel();
        let (tx, events) = std::sync::mpsc::channel();
        preview.worker = pdf_worker::Worker {
            tx: requests,
            rx: events,
        };
        preview.sizes = vec![(600., 800.); 2];
        preview.texts = vec![None; 2];
        fn frame(ctx: &egui::Context, preview: &mut Preview) {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 900.))),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| preview.ui(ui));
                },
            );
        }
        frame(&ctx, &mut preview);
        let Request::Viewport { tasks, .. } = rx.recv_timeout(Duration::from_secs(1)).unwrap()
        else {
            panic!("expected viewport")
        };
        assert!(tasks.iter().all(|t| t.width == 768));
        assert_eq!(tasks[0].page, 0);
        let bitmap = || egui::ColorImage::new([2, 2], vec![Color32::WHITE; 4]);
        for page in 0..2 {
            tx.send(Event::Rendered {
                generation: 0,
                page,
                width: 768,
                image: bitmap(),
            })
            .unwrap();
        }
        preview.poll(&ctx);
        assert_eq!(preview.fresh.len(), 1, "at most one GPU upload per frame");
        preview.poll(&ctx);
        assert_eq!(preview.fresh.len(), 2);
        frame(&ctx, &mut preview);
        for _ in rx.try_iter() {}
        preview.motion = Instant::now() - Duration::from_secs(1);
        frame(&ctx, &mut preview);
        let Request::Viewport { tasks, .. } = rx.recv_timeout(Duration::from_secs(1)).unwrap()
        else {
            panic!("expected refinement")
        };
        assert_eq!(tasks.len(), 1);
        let full = tasks[0];
        assert_eq!(full.page, 0);
        tx.send(Event::Rendered {
            generation: 0,
            page: 0,
            width: full.width,
            image: bitmap(),
        })
        .unwrap();
        preview.poll(&ctx);
        frame(&ctx, &mut preview);
        let Request::Viewport { tasks, .. } = rx.recv_timeout(Duration::from_secs(1)).unwrap()
        else {
            panic!("expected empty plan")
        };
        assert!(tasks.is_empty());
        frame(&ctx, &mut preview);
        assert!(
            rx.try_recv().is_err(),
            "cached view must not send repeated render requests"
        );
        preview.requested = 1;
        tx.send(Event::Opened {
            generation: 1,
            sizes: vec![(600., 800.); 2],
        })
        .unwrap();
        preview.poll(&ctx);
        assert!(
            !preview.textures.is_empty(),
            "keep pixels visible across recompilation"
        );
        assert!(
            preview.fresh.is_empty(),
            "old pixels must not count as current renders"
        );
        assert!(
            !preview.page_ready(0),
            "old pixels must not navigate using the new SyncTeX map"
        );
    }
    #[test]
    fn long_documents_only_lay_out_pages_near_the_viewport() {
        let ctx = egui::Context::default();
        let mut preview = Preview::new(ctx.clone());
        preview.sizes = vec![(600., 800.); 10_000];
        preview.texts = vec![None; 10_000];
        let start = std::time::Instant::now();
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 900.))),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| preview.ui(ui));
            },
        );
        eprintln!(
            "PDF LAYOUT 10000 pages: {:?}, {} page widgets",
            start.elapsed(),
            preview.page_rects.len()
        );
        assert!(
            preview.page_rects.len() <= 4,
            "offscreen pages must not allocate widgets every frame"
        );
        let start = Instant::now();
        for _ in 0..100 {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 900.))),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| preview.ui(ui));
                },
            );
        }
        eprintln!("PDF LAYOUT steady average: {:?}", start.elapsed() / 100);
    }
    #[test]
    #[ignore = "requires PDFium"]
    fn invalid_pdf_finishes_loading_with_an_error() {
        let ctx = egui::Context::default();
        let mut preview = Preview::new(ctx.clone());
        preview.load(std::env::temp_dir().join("folio-nonexistent-input.pdf"));
        let start = std::time::Instant::now();
        while preview.error.is_none() && start.elapsed().as_secs() < 5 {
            preview.poll(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(preview.error.is_some());
        assert!(!preview.is_loading());
    }
    #[test]
    fn pdf_drag_selection_includes_the_pressed_character() {
        let ctx = egui::Context::default();
        let mut preview = Preview::new(ctx.clone());
        preview.sizes = vec![(600., 800.)];
        preview.texts = vec![Some(PageText {
            width: 600.,
            height: 800.,
            glyphs: "ABC"
                .chars()
                .enumerate()
                .map(|(i, ch)| folio_core::pdf::Glyph {
                    ch,
                    rect: folio_core::pdf::Rect {
                        x: 50. + i as f32 * 12.,
                        y: 50.,
                        width: 12.,
                        height: 16.,
                    },
                })
                .collect(),
        })];
        fn frame(ctx: &egui::Context, preview: &mut Preview, events: Vec<egui::Event>) {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 900.))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        preview.ui(ui);
                    });
                },
            );
        }
        frame(&ctx, &mut preview, vec![]);
        let rect = preview.page_rects[0];
        let scale = rect.width() / 600.;
        let start = rect.min + Vec2::new(56., 58.) * scale;
        let end = rect.min + Vec2::new(80., 58.) * scale;
        frame(
            &ctx,
            &mut preview,
            vec![
                egui::Event::PointerMoved(start),
                egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        frame(&ctx, &mut preview, vec![egui::Event::PointerMoved(end)]);
        frame(
            &ctx,
            &mut preview,
            vec![egui::Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(preview.selection, Some((0, 0, 2)));
        assert_eq!(preview.go_target(), Some((1, 56., 58.)));
        preview.selection = Some((0, 2, 0));
        assert_eq!(preview.go_target(), Some((1, 56., 58.)));
    }
    #[test]
    fn fit_zoom_and_visible_page_match_the_displayed_content() {
        assert_eq!(
            page_scale(
                (600., 800.),
                Vec2::new(932., 900.),
                Some(PdfFit::Width),
                1.,
                0.9
            ),
            932. * 0.9 / 600.
        );
        assert_eq!(
            page_scale((600., 800.), Vec2::new(932., 900.), None, 2., 0.9),
            2.
        );
        let clip = Rect::from_min_max(Pos2::ZERO, Pos2::new(900., 600.));
        let pages = [
            (
                1,
                Rect::from_min_max(Pos2::new(0., -900.), Pos2::new(900., 70.)),
            ),
            (
                2,
                Rect::from_min_max(Pos2::new(0., 100.), Pos2::new(900., 1100.)),
            ),
        ];
        assert_eq!(dominant_page(&pages, clip), Some(2));
    }
}

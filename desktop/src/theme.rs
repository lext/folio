//! Folio's native visual vocabulary. Loaded once; no image assets or animation loop.
use eframe::egui::{self, Color32, FontId, RichText, Stroke};
use std::sync::OnceLock;

pub const CANVAS: Color32 = Color32::from_rgb(18, 20, 22);
pub const SIDEBAR: Color32 = Color32::from_rgb(16, 18, 20);
pub const SURFACE: Color32 = Color32::from_rgb(25, 28, 31);
pub const HOVER: Color32 = Color32::from_rgb(32, 36, 39);
pub const SELECTED: Color32 = Color32::from_rgb(35, 45, 43);
pub const BORDER: Color32 = Color32::from_rgb(46, 51, 54);
pub const TEXT: Color32 = Color32::from_rgb(224, 227, 229);
pub const SECONDARY: Color32 = Color32::from_rgb(155, 163, 169);
pub const MUTED: Color32 = Color32::from_rgb(110, 120, 129);
pub const ACCENT: Color32 = Color32::from_rgb(172, 207, 192);
pub const ROW_HEIGHT: f32 = 28.;

pub fn line_height(font_size: f32) -> f32 {
    (font_size * 1.55).round()
}

fn fonts() -> egui::FontDefinitions {
    static FONTS: OnceLock<egui::FontDefinitions> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut fonts = egui::FontDefinitions::default();
            // Use the platform's own fonts without redistributing them. Linux has
            // several common installation layouts; the bundled fonts always remain
            // at the end of each family for missing fonts and Unicode fallback.
            let sans: &[&str] = if cfg!(target_os = "macos") {
                &["/System/Library/Fonts/SFNS.ttf"]
            } else {
                &[
                    "/usr/share/fonts/truetype/inter/Inter-Regular.otf",
                    "/usr/share/fonts/opentype/inter/Inter-Regular.otf",
                    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
                    "/usr/share/fonts/noto/NotoSans-Regular.ttf",
                    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                ]
            };
            let mono: &[&str] = if cfg!(target_os = "macos") {
                &["/System/Library/Fonts/SFNSMono.ttf"]
            } else {
                &[
                    "/usr/share/fonts/truetype/jetbrains-mono/JetBrainsMono-Regular.ttf",
                    "/usr/share/fonts/TTF/JetBrainsMono-Regular.ttf",
                    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
                ]
            };
            for (family, name, candidates) in [
                (egui::FontFamily::Proportional, "folio-sans", sans),
                (egui::FontFamily::Monospace, "folio-mono", mono),
            ] {
                if let Some(bytes) = candidates.iter().find_map(|p| std::fs::read(p).ok()) {
                    fonts
                        .font_data
                        .insert(name.into(), egui::FontData::from_owned(bytes).into());
                    fonts
                        .families
                        .entry(family)
                        .or_default()
                        .insert(0, name.into());
                }
            }
            fonts
        })
        .clone()
}

pub fn apply(ctx: &egui::Context, editor_size: f32) {
    ctx.set_fonts(fonts());
    ctx.set_visuals(egui::Visuals::dark());
    let mut style = (*ctx.style()).clone();
    style.text_styles = [
        (egui::TextStyle::Small, FontId::proportional(11.)),
        (egui::TextStyle::Body, FontId::proportional(13.)),
        (egui::TextStyle::Button, FontId::proportional(13.)),
        (egui::TextStyle::Heading, FontId::proportional(19.)),
        (egui::TextStyle::Monospace, FontId::monospace(editor_size)),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(8., 6.);
    style.spacing.button_padding = egui::vec2(10., 6.);
    style.spacing.window_margin = egui::Margin::same(18);
    style.spacing.menu_margin = egui::Margin::same(8);
    style.spacing.interact_size.y = 28.;
    style.spacing.scroll.bar_width = 5.;
    style.spacing.scroll.floating_width = 2.;
    style.spacing.scroll.dormant_handle_opacity = 0.;
    style.animation_time = 0.;
    style.scroll_animation = egui::style::ScrollAnimation::duration(0.);
    let v = &mut style.visuals;
    v.panel_fill = CANVAS;
    v.window_fill = SURFACE;
    v.extreme_bg_color = CANVAS;
    v.text_edit_bg_color = Some(CANVAS);
    v.code_bg_color = CANVAS;
    v.faint_bg_color = HOVER;
    v.weak_text_color = Some(SECONDARY);
    v.selection.bg_fill = SELECTED;
    v.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    v.hyperlink_color = ACCENT;
    v.window_corner_radius = 10.into();
    v.menu_corner_radius = 8.into();
    v.window_stroke = Stroke::new(1.0_f32, BORDER);
    v.window_shadow = egui::epaint::Shadow::NONE;
    v.popup_shadow = egui::epaint::Shadow::NONE;
    v.window_highlight_topmost = false;
    v.button_frame = false;
    v.text_cursor.stroke = Stroke::new(1.5_f32, ACCENT);
    v.text_cursor.blink = false;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    for (widget, fill, foreground) in [
        (&mut v.widgets.inactive, SURFACE, SECONDARY),
        (&mut v.widgets.hovered, HOVER, TEXT),
        (&mut v.widgets.active, SELECTED, ACCENT),
        (&mut v.widgets.open, HOVER, TEXT),
    ] {
        widget.bg_fill = fill;
        widget.weak_bg_fill = if fill == SURFACE {
            Color32::TRANSPARENT
        } else {
            fill
        };
        widget.bg_stroke = Stroke::new(1.0_f32, BORDER);
        widget.fg_stroke = Stroke::new(1.0_f32, foreground);
        widget.corner_radius = 6.into();
        widget.expansion = 0.;
    }
    ctx.set_style(style);
}

pub fn popup() -> egui::Frame {
    egui::Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(10)
        .inner_margin(14.)
}

pub fn caption(text: impl Into<String>) -> RichText {
    RichText::new(text).size(11.).color(MUTED)
}

/// A single hit target, including its metadata and the empty space to the right.
/// Clip long labels before the shortcut; never let them grow the overlay.
pub fn row(
    ui: &mut egui::Ui,
    title: &str,
    detail: &str,
    shortcut: &str,
    selected: bool,
) -> egui::Response {
    let height = 44.;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::click(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            title,
        )
    });
    if ui.is_rect_visible(rect) {
        if selected || response.hovered() || response.has_focus() {
            ui.painter()
                .rect_filled(rect, 6., if selected { SELECTED } else { HOVER });
        }
        let mut right = rect.right() - 12.;
        let shortcut = key_label(shortcut);
        if !shortcut.is_empty() {
            let key = ui
                .painter()
                .layout_no_wrap(shortcut, FontId::proportional(11.), SECONDARY);
            let size = key.size() + egui::vec2(12., 6.);
            let badge = egui::Rect::from_min_size(
                egui::pos2(right - size.x, rect.center().y - size.y / 2.),
                size,
            );
            ui.painter().rect_stroke(
                badge,
                4.,
                Stroke::new(1.0_f32, BORDER),
                egui::StrokeKind::Inside,
            );
            ui.painter()
                .galley(badge.min + egui::vec2(6., 3.), key, SECONDARY);
            right = badge.left() - 16.;
        }
        let clip = egui::Rect::from_min_max(
            rect.min + egui::vec2(12., 0.),
            egui::pos2(right.max(rect.left() + 12.), rect.bottom()),
        );
        let painter = ui.painter().with_clip_rect(clip.intersect(ui.clip_rect()));
        if detail.is_empty() {
            painter.text(
                egui::pos2(rect.left() + 12., rect.center().y),
                egui::Align2::LEFT_CENTER,
                title,
                FontId::proportional(13.),
                TEXT,
            );
        } else {
            painter.text(
                rect.min + egui::vec2(12., 5.),
                egui::Align2::LEFT_TOP,
                title,
                FontId::proportional(13.),
                TEXT,
            );
            painter.text(
                rect.min + egui::vec2(12., 24.),
                egui::Align2::LEFT_TOP,
                detail,
                FontId::proportional(11.),
                SECONDARY,
            );
        }
    }
    response
}

/// Search controls stay in the keyboard layer, with configured bindings visible.
pub fn search_hint(ui: &mut egui::Ui, keys: &crate::settings::Keybindings) {
    use crate::settings::Action;
    ui.add(
        egui::Label::new(caption(format!(
            "{} next  ·  {} previous  ·  {} close",
            key_label(&keys.hint(Action::FindNext)),
            key_label(&keys.hint(Action::FindPrevious)),
            key_label(&keys.hint(Action::Dismiss))
        )))
        .truncate(),
    );
}

/// Compact presentation only; parsing and routing retain the original bindings.
pub fn key_label(label: &str) -> String {
    let label = label
        .replace("ArrowDown", "↓")
        .replace("ArrowUp", "↑")
        .replace("ArrowLeft", "←")
        .replace("ArrowRight", "→")
        .replace("Enter", "enter")
        .replace("Plus", "+")
        .replace("Minus", "-")
        .replace("Equals", "=")
        .replace("Escape", "esc");
    if cfg!(target_os = "macos") {
        label
            .replace("Shift", "⇧")
            .replace("Alt", "⌥")
            .replace("Ctrl", "⌃")
            .replace(" + ", "")
    } else {
        label.replace(" + ", "+")
    }
}

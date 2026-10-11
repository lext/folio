//! Folio's native visual vocabulary. Loaded once; no image assets or animation loop.
use eframe::egui::{self, Color32, FontId, RichText, Stroke};
use std::sync::OnceLock;

/// Every color Folio paints. Widgets read the palette of the active egui theme,
/// so `[theme] mode` and a system appearance change need no other code.
pub struct Palette {
    pub canvas: Color32,
    pub sidebar: Color32,
    pub surface: Color32,
    pub hover: Color32,
    pub selected: Color32,
    pub border: Color32,
    pub text: Color32,
    pub secondary: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub error: Color32,
    pub added: Color32,
    pub modified: Color32,
    pub deleted: Color32,
    pub command: Color32,
    pub comment: Color32,
    pub math: Color32,
    pub delimiter: Color32,
    pub string: Color32,
    pub keyword: Color32,
}

/// Slate surfaces with saturated syntax hues, so commands, math, strings and
/// comments are told apart at a glance without a glaring background.
pub const DARK: Palette = Palette {
    canvas: Color32::from_rgb(30, 33, 39),
    sidebar: Color32::from_rgb(26, 29, 34),
    surface: Color32::from_rgb(37, 41, 48),
    hover: Color32::from_rgb(44, 49, 58),
    selected: Color32::from_rgb(41, 53, 55),
    border: Color32::from_rgb(58, 63, 73),
    text: Color32::from_rgb(205, 210, 218),
    secondary: Color32::from_rgb(160, 167, 178),
    muted: Color32::from_rgb(124, 131, 143),
    accent: Color32::from_rgb(126, 200, 170),
    error: Color32::from_rgb(224, 108, 117),
    added: Color32::from_rgb(152, 195, 121),
    modified: Color32::from_rgb(97, 175, 239),
    deleted: Color32::from_rgb(224, 108, 117),
    command: Color32::from_rgb(97, 175, 239),
    comment: Color32::from_rgb(134, 140, 151),
    math: Color32::from_rgb(209, 154, 102),
    delimiter: Color32::from_rgb(86, 182, 194),
    string: Color32::from_rgb(152, 195, 121),
    keyword: Color32::from_rgb(198, 120, 221),
};

/// Soft grey-blue paper rather than white; syntax hues are darkened to stay
/// legible on it.
pub const LIGHT: Palette = Palette {
    canvas: Color32::from_rgb(239, 241, 245),
    sidebar: Color32::from_rgb(230, 233, 239),
    surface: Color32::from_rgb(247, 248, 250),
    hover: Color32::from_rgb(220, 224, 232),
    selected: Color32::from_rgb(203, 222, 215),
    border: Color32::from_rgb(204, 208, 218),
    text: Color32::from_rgb(52, 56, 72),
    secondary: Color32::from_rgb(82, 87, 105),
    muted: Color32::from_rgb(108, 113, 131),
    accent: Color32::from_rgb(24, 112, 90),
    error: Color32::from_rgb(196, 24, 58),
    added: Color32::from_rgb(46, 125, 50),
    modified: Color32::from_rgb(30, 102, 245),
    deleted: Color32::from_rgb(196, 24, 58),
    command: Color32::from_rgb(30, 102, 245),
    comment: Color32::from_rgb(112, 116, 134),
    math: Color32::from_rgb(176, 78, 0),
    delimiter: Color32::from_rgb(14, 116, 124),
    string: Color32::from_rgb(40, 116, 44),
    keyword: Color32::from_rgb(136, 57, 239),
};

/// `[theme] mode`. System follows the desktop appearance while Folio runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Dark,
    Light,
    System,
}

pub fn palette(visuals: &egui::Visuals) -> &'static Palette {
    if visuals.dark_mode { &DARK } else { &LIGHT }
}

/// The palette of the theme `ui` is painted with.
pub fn of(ui: &egui::Ui) -> &'static Palette {
    palette(ui.visuals())
}

/// A translucent wash of a palette color, for selections drawn over text.
pub fn tint(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

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

pub fn apply(ctx: &egui::Context, editor_size: f32, mode: Mode) {
    ctx.set_fonts(fonts());
    for (theme, visuals, p) in [
        (egui::Theme::Dark, egui::Visuals::dark(), &DARK),
        (egui::Theme::Light, egui::Visuals::light(), &LIGHT),
    ] {
        ctx.set_style_of(theme, style(visuals, p, editor_size));
    }
    ctx.set_theme(match mode {
        Mode::Dark => egui::ThemePreference::Dark,
        Mode::Light => egui::ThemePreference::Light,
        Mode::System => egui::ThemePreference::System,
    });
}

fn style(visuals: egui::Visuals, p: &Palette, editor_size: f32) -> egui::Style {
    let mut style = egui::Style {
        visuals,
        ..Default::default()
    };
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
    v.panel_fill = p.canvas;
    v.window_fill = p.surface;
    v.extreme_bg_color = p.canvas;
    v.text_edit_bg_color = Some(p.canvas);
    v.code_bg_color = p.canvas;
    v.faint_bg_color = p.hover;
    v.weak_text_color = Some(p.secondary);
    v.selection.bg_fill = p.selected;
    v.selection.stroke = Stroke::new(1.0_f32, p.accent);
    v.hyperlink_color = p.accent;
    v.error_fg_color = p.error;
    v.window_corner_radius = 10.into();
    v.menu_corner_radius = 8.into();
    v.window_stroke = Stroke::new(1.0_f32, p.border);
    v.window_shadow = egui::epaint::Shadow::NONE;
    v.popup_shadow = egui::epaint::Shadow::NONE;
    v.window_highlight_topmost = false;
    v.button_frame = false;
    v.text_cursor.stroke = Stroke::new(1.5_f32, p.accent);
    v.text_cursor.blink = false;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, p.text);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.border);
    for (widget, fill, foreground) in [
        (&mut v.widgets.inactive, p.surface, p.secondary),
        (&mut v.widgets.hovered, p.hover, p.text),
        (&mut v.widgets.active, p.selected, p.accent),
        (&mut v.widgets.open, p.hover, p.text),
    ] {
        widget.bg_fill = fill;
        widget.weak_bg_fill = if fill == p.surface {
            Color32::TRANSPARENT
        } else {
            fill
        };
        widget.bg_stroke = Stroke::new(1.0_f32, p.border);
        widget.fg_stroke = Stroke::new(1.0_f32, foreground);
        widget.corner_radius = 6.into();
        widget.expansion = 0.;
    }
    style
}

pub fn popup(visuals: &egui::Visuals) -> egui::Frame {
    let p = palette(visuals);
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0_f32, p.border))
        .corner_radius(10)
        .inner_margin(14.)
}

/// Context menus use the same rows as the rest of Folio: full-strength labels,
/// muted shortcuts and a borderless hover fill. Call first inside the menu.
pub fn menu(ui: &mut egui::Ui) {
    let p = of(ui);
    ui.set_min_width(220.);
    ui.spacing_mut().button_padding = egui::vec2(10., 6.);
    ui.spacing_mut().item_spacing.y = 2.;
    let v = ui.visuals_mut();
    v.weak_text_color = Some(p.muted);
    v.widgets.inactive.fg_stroke.color = p.text;
    v.widgets.active.fg_stroke.color = p.text;
    for widget in [&mut v.widgets.hovered, &mut v.widgets.active] {
        widget.bg_stroke = Stroke::NONE;
    }
}

pub fn caption(ui: &egui::Ui, text: impl Into<String>) -> RichText {
    RichText::new(text).size(11.).color(of(ui).muted)
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
    let p = of(ui);
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
                .rect_filled(rect, 6., if selected { p.selected } else { p.hover });
        }
        let mut right = rect.right() - 12.;
        let shortcut = key_label(shortcut);
        if !shortcut.is_empty() {
            let key = ui
                .painter()
                .layout_no_wrap(shortcut, FontId::proportional(11.), p.secondary);
            let size = key.size() + egui::vec2(12., 6.);
            let badge = egui::Rect::from_min_size(
                egui::pos2(right - size.x, rect.center().y - size.y / 2.),
                size,
            );
            ui.painter().rect_stroke(
                badge,
                4.,
                Stroke::new(1.0_f32, p.border),
                egui::StrokeKind::Inside,
            );
            ui.painter()
                .galley(badge.min + egui::vec2(6., 3.), key, p.secondary);
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
                p.text,
            );
        } else {
            painter.text(
                rect.min + egui::vec2(12., 5.),
                egui::Align2::LEFT_TOP,
                title,
                FontId::proportional(13.),
                p.text,
            );
            painter.text(
                rect.min + egui::vec2(12., 24.),
                egui::Align2::LEFT_TOP,
                detail,
                FontId::proportional(11.),
                p.secondary,
            );
        }
    }
    response
}

/// Search controls stay in the keyboard layer, with configured bindings visible.
pub fn search_hint(ui: &mut egui::Ui, keys: &crate::settings::Keybindings) {
    use crate::settings::Action;
    ui.add(
        egui::Label::new(caption(
            ui,
            format!(
                "{} next  ·  {} previous  ·  {} close",
                key_label(&keys.hint(Action::FindNext)),
                key_label(&keys.hint(Action::FindPrevious)),
                key_label(&keys.hint(Action::Dismiss))
            ),
        ))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_selects_a_complete_palette_for_widgets_and_helpers() {
        for (mode, expected) in [(Mode::Dark, &DARK), (Mode::Light, &LIGHT)] {
            let ctx = egui::Context::default();
            apply(&ctx, 15., mode);
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let p = of(ui);
                    assert_eq!(p.canvas, expected.canvas);
                    let v = ui.visuals();
                    assert_eq!(v.dark_mode, mode == Mode::Dark);
                    assert_eq!(v.panel_fill, p.canvas);
                    assert_eq!(v.window_fill, p.surface);
                    assert_eq!(v.text_color(), p.text);
                    assert_eq!(v.selection.bg_fill, p.selected);
                    assert_eq!(popup(v).fill, p.surface);
                });
            });
        }
    }

    #[test]
    fn system_mode_follows_the_desktop_appearance_while_running() {
        let ctx = egui::Context::default();
        apply(&ctx, 15., Mode::System);
        for (system, expected) in [
            (egui::Theme::Light, &LIGHT),
            (egui::Theme::Dark, &DARK),
            (egui::Theme::Light, &LIGHT),
        ] {
            let input = || egui::RawInput {
                system_theme: Some(system),
                ..Default::default()
            };
            let _ = ctx.run(input(), |_| {});
            let _ = ctx.run(input(), |ctx| {
                assert_eq!(palette(&ctx.style().visuals).canvas, expected.canvas);
            });
        }
    }

    #[test]
    fn both_palettes_keep_text_readable_on_every_surface() {
        fn luminance(c: Color32) -> f32 {
            let channel = |v: u8| {
                let v = v as f32 / 255.;
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
        }
        fn contrast(a: Color32, b: Color32) -> f32 {
            let (a, b) = (luminance(a), luminance(b));
            (a.max(b) + 0.05) / (a.min(b) + 0.05)
        }
        for (name, p) in [("dark", &DARK), ("light", &LIGHT)] {
            for surface in [p.canvas, p.sidebar, p.surface, p.hover, p.selected] {
                for (role, color, minimum) in [
                    ("text", p.text, 7.),
                    ("secondary", p.secondary, 4.5),
                    ("muted", p.muted, 3.),
                    ("accent", p.accent, 3.),
                    ("error", p.error, 3.),
                    ("command", p.command, 3.),
                    ("comment", p.comment, 3.),
                    ("math", p.math, 3.),
                    ("delimiter", p.delimiter, 3.),
                    ("string", p.string, 3.),
                    ("keyword", p.keyword, 3.),
                ] {
                    let ratio = contrast(color, surface);
                    assert!(
                        ratio >= minimum,
                        "{name} {role} on {surface:?}: contrast {ratio:.2} < {minimum}"
                    );
                }
            }
        }
    }
}

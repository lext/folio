use crate::settings::PdfFit;
use eframe::egui::{Pos2, Rect, Vec2};

pub fn page_scale(
    (pw, ph): (f32, f32),
    viewport: Vec2,
    fit: Option<PdfFit>,
    zoom: f32,
    width_fraction: f32,
) -> f32 {
    let width = (viewport.x * width_fraction).max(1.) / pw.max(1.);
    match fit {
        Some(PdfFit::Page) => width.min((viewport.y - 32.).max(1.) / ph.max(1.)),
        Some(PdfFit::Width) => width,
        None => zoom,
    }
}

#[derive(Default)]
pub struct Layout {
    pub pages: Vec<Rect>,
    pub size: Vec2,
    key: Option<(usize, u32, u32, Option<PdfFit>, u32, u32)>,
}
impl Layout {
    pub fn clear(&mut self) {
        self.key = None;
        self.pages.clear();
        self.size = Vec2::ZERO;
    }
    pub fn update(
        &mut self,
        sizes: &[(f32, f32)],
        viewport: Vec2,
        fit: Option<PdfFit>,
        zoom: f32,
        width_fraction: f32,
    ) -> bool {
        let key = (
            sizes.len(),
            viewport.x.to_bits(),
            viewport.y.to_bits(),
            fit,
            zoom.to_bits(),
            width_fraction.to_bits(),
        );
        if self.key == Some(key) {
            return false;
        }
        self.key = Some(key);
        self.pages.clear();
        self.size = Vec2::new(viewport.x, 0.);
        for &(pw, ph) in sizes {
            let scale = page_scale((pw, ph), viewport, fit, zoom, width_fraction);
            let size = Vec2::new(pw.max(1.) * scale, ph.max(1.) * scale);
            self.pages.push(Rect::from_min_size(
                Pos2::new(
                    ((viewport.x - size.x) / 2.).max(if fit.is_some() { 0. } else { 12. }),
                    self.size.y + 12.,
                ),
                size,
            ));
            self.size.y += size.y + 32.;
            self.size.x = self
                .size
                .x
                .max(size.x + if fit.is_some() { 0. } else { 24. });
        }
        true
    }
    pub fn visible(&self, viewport: Rect, overscan: f32) -> std::ops::Range<usize> {
        let start = self
            .pages
            .partition_point(|r| r.bottom() < viewport.top() - overscan);
        let end = self
            .pages
            .partition_point(|r| r.top() <= viewport.bottom() + overscan);
        start..end
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn variable_page_sizes_and_zoom_preserve_visible_ranges() {
        let mut layout = Layout::default();
        layout.update(
            &[(600., 800.), (600., 1200.), (300., 300.)],
            Vec2::new(932., 900.),
            Some(PdfFit::Width),
            1.,
            0.9,
        );
        assert!((layout.pages[0].width() - 932. * 0.9).abs() < 0.001);
        let second = layout.pages[1];
        assert_eq!(
            layout.visible(
                Rect::from_min_size(second.min + Vec2::new(0., 20.), Vec2::new(900., 200.)),
                0.
            ),
            1..2
        );
        layout.update(
            &[(600., 800.), (600., 1200.), (300., 300.)],
            Vec2::new(932., 900.),
            None,
            2.,
            0.9,
        );
        assert_eq!(layout.pages[0].width(), 1200.);
    }
}

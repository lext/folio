use eframe::egui::{self, Event, Pos2, Response};

pub struct GoGesture {
    binding: Option<crate::settings::MouseGesture>,
    first: Option<(egui::Id, Pos2, f64)>,
    handled_frame: Option<u64>,
}
impl Default for GoGesture {
    fn default() -> Self {
        Self::new(crate::settings::Settings::default().mouse.go)
    }
}
impl GoGesture {
    /// Keep native selection handling from consuming the configured gesture,
    /// even if the modifier was released later in this input batch. Each surface
    /// chooses its navigation target when the gesture completes.
    pub fn modified_pointer(&self, ctx: &egui::Context) -> bool {
        let Some(binding) = &self.binding else {
            return false;
        };
        ctx.input(|i| {
            (crate::settings::modifiers_match(i.modifiers, binding.modifiers)
                && i.pointer.button_down(binding.button))
                || i.events.iter().any(|event| matches!(event,
                    Event::PointerButton { button, modifiers, .. }
                    if *button == binding.button && crate::settings::modifiers_match(*modifiers, binding.modifiers)))
        })
    }
    pub fn new(binding: Option<crate::settings::MouseGesture>) -> Self {
        Self {
            binding,
            first: None,
            handled_frame: None,
        }
    }
    pub fn clicked(&mut self, response: &Response) -> bool {
        let Some(binding) = &self.binding else {
            return false;
        };
        let ctx = &response.ctx;
        let frame = ctx.cumulative_frame_nr();
        if self.handled_frame == Some(frame) || !response.clicked_by(binding.button) {
            return false;
        }
        self.handled_frame = Some(frame);
        let options = ctx.options(|o| o.input_options);
        ctx.input(|i| {
            let mut go = false;
            for event in &i.events {
                if let Event::PointerButton {
                    pos,
                    button,
                    pressed: false,
                    modifiers,
                } = event
                    && *button == binding.button
                    && response.interact_rect.contains(*pos)
                {
                    if !crate::settings::modifiers_match(*modifiers, binding.modifiers) {
                        self.first = None;
                        continue;
                    }
                    if binding.clicks == 1
                        || self.first.is_some_and(|(id, point, time)| {
                            id == response.id
                                && point.distance(*pos) <= options.max_click_dist
                                && i.time - time <= options.max_double_click_delay
                        })
                    {
                        go = true;
                        self.first = None;
                    } else {
                        self.first = Some((response.id, *pos, i.time));
                    }
                }
            }
            go
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::PointerButton;
    struct Harness {
        ctx: egui::Context,
        gesture: GoGesture,
        time: f64,
    }
    impl Harness {
        fn new() -> Self {
            let mut harness = Self {
                ctx: Default::default(),
                gesture: Default::default(),
                time: 1.,
            };
            harness.frame(vec![]);
            harness.frame(vec![]);
            harness
        }
        fn frame(&mut self, events: Vec<Event>) -> bool {
            self.time += 0.02;
            let mut go = false;
            let _ = self.ctx.run(
                egui::RawInput {
                    time: Some(self.time),
                    screen_rect: Some(egui::Rect::from_min_size(
                        Pos2::ZERO,
                        egui::vec2(500., 500.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let response = ui.interact(
                            ui.max_rect(),
                            egui::Id::new("document"),
                            egui::Sense::click_and_drag(),
                        );
                        go |= self.gesture.clicked(&response);
                        assert!(
                            !self.gesture.clicked(&response),
                            "multipass/repeated reads must not navigate twice"
                        );
                    });
                },
            );
            go
        }
        fn click(&mut self, modifiers: egui::Modifiers, button: PointerButton) -> bool {
            let pos = Pos2::new(100., 100.);
            assert!(!self.frame(vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button,
                    pressed: true,
                    modifiers
                }
            ]));
            self.frame(vec![Event::PointerButton {
                pos,
                button,
                pressed: false,
                modifiers,
            }])
        }
    }
    #[test]
    fn mouse_binding_can_change_modifier_button_click_count_or_be_disabled() {
        let mut h = Harness::new();
        h.gesture = GoGesture::new(
            crate::settings::Settings::parse("[mouse]\ngo='Alt+RightClick'")
                .unwrap()
                .mouse
                .go,
        );
        assert!(h.click(egui::Modifiers::ALT, PointerButton::Secondary));
        assert!(!h.click(egui::Modifiers::COMMAND, PointerButton::Primary));
        assert!(!h.click(egui::Modifiers::COMMAND, PointerButton::Primary));
        h.gesture = GoGesture::new(None);
        assert!(!h.click(egui::Modifiers::COMMAND, PointerButton::Primary));
        assert!(!h.click(egui::Modifiers::COMMAND, PointerButton::Primary));
    }
    #[test]
    fn go_requires_two_modified_left_clicks_and_expires() {
        let mut h = Harness::new();
        assert!(!h.click(egui::Modifiers::NONE, PointerButton::Primary));
        assert!(!h.click(egui::Modifiers::NONE, PointerButton::Primary));
        // The preceding unmodified selection click cannot count as the first click.
        assert!(!h.click(egui::Modifiers::COMMAND, PointerButton::Primary));
        assert!(h.click(egui::Modifiers::COMMAND, PointerButton::Primary));
        assert!(!h.frame(vec![]));
        assert!(!h.click(egui::Modifiers::COMMAND, PointerButton::Secondary));
        assert!(!h.click(egui::Modifiers::COMMAND, PointerButton::Secondary));
        assert!(!h.click(egui::Modifiers::COMMAND, PointerButton::Primary));
        h.time += 1.;
        assert!(!h.click(egui::Modifiers::COMMAND, PointerButton::Primary));
        assert!(h.click(egui::Modifiers::COMMAND, PointerButton::Primary));
    }
    #[test]
    fn coalesced_double_click_navigates_once_even_after_modifier_release() {
        let mut h = Harness::new();
        let pos = Pos2::new(100., 100.);
        h.frame(vec![Event::PointerMoved(pos)]);
        let events = [true, false, true, false]
            .into_iter()
            .map(|pressed| Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::COMMAND,
            })
            .collect();
        // RawInput.modifiers is NONE: both clicks retain their own modifier state.
        assert!(h.frame(events));
        assert!(!h.frame(vec![]));
    }
}

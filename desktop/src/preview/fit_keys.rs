//! PDF-local key sequences. Singles act immediately; a released second press
//! can replace that fit with the double binding. Text fields never enter here.
use crate::settings::{Action, Keybindings, modifiers_match};
use eframe::egui::{self, Event, Key, Modifiers};

#[derive(Default)]
pub struct FitKeys {
    first: Option<Press>,
    handled_frame: Option<u64>,
}
struct Press {
    key: Key,
    modifiers: Modifiers,
    time: f64,
    released: bool,
}
impl FitKeys {
    pub fn reset(&mut self) {
        self.first = None;
    }

    pub fn consume(
        &mut self,
        ctx: &egui::Context,
        bindings: &Keybindings,
        enabled: bool,
    ) -> Vec<Action> {
        if !enabled || !ctx.input(|i| i.focused) {
            self.reset();
            return vec![];
        }
        let frame = ctx.cumulative_frame_nr();
        if self.handled_frame == Some(frame) {
            return vec![];
        }
        self.handled_frame = Some(frame);
        ctx.input_mut(|input| {
            let mut actions = Vec::new();
            input.events.retain(|event| {
                match event {
                    Event::Key {
                        key,
                        pressed,
                        repeat,
                        modifiers,
                        ..
                    } => {
                        if !pressed {
                            if let Some(first) = &mut self.first
                                && first.key == *key
                            {
                                first.released = true;
                            }
                            return true;
                        }
                        let matching = |presses| {
                            [Action::PdfFit, Action::PdfFitPage].into_iter().find(|a| {
                                bindings.chords(*a).iter().any(|b| {
                                    b.presses == presses
                                        && b.key == *key
                                        && modifiers_match(*modifiers, b.modifiers)
                                })
                            })
                        };
                        let single = matching(1);
                        let double = matching(2);
                        if single.is_none() && double.is_none() {
                            self.reset();
                            return true;
                        }
                        // Egui marks OS key repeats even if a backend didn't.
                        if *repeat {
                            return false;
                        }
                        let completed = double.is_some()
                            && self.first.as_ref().is_some_and(|first| {
                                first.key == *key
                                    && modifiers_match(*modifiers, first.modifiers)
                                    && first.released
                                    && (0. ..=0.35).contains(&(input.time - first.time))
                            });
                        if completed {
                            actions.extend(double);
                            self.reset();
                        } else {
                            actions.extend(single);
                            self.first = double.map(|_| Press {
                                key: *key,
                                modifiers: *modifiers,
                                time: input.time,
                                released: false,
                            });
                        }
                        false
                    }
                    Event::PointerButton { .. }
                    | Event::MouseWheel { .. }
                    | Event::Zoom(_)
                    | Event::WindowFocused(false)
                    | Event::Paste(_)
                    | Event::Cut => {
                        self.reset();
                        true
                    }
                    _ => true,
                }
            });
            actions
        })
    }
}

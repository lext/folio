#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Layer {
    #[default]
    Source,
    Preview,
}

#[derive(Default)]
pub struct Layers {
    pub layer: Layer,
    pub revision: u64,
    pub built: Option<u64>,
    compiling: Option<u64>,
    failed: Option<u64>,
}
impl Layers {
    pub fn edited(&mut self) {
        self.revision += 1;
    }
    pub fn stale(&self) -> bool {
        self.built != Some(self.revision)
    }
    pub fn can_auto_build(&self) -> bool {
        self.stale() && self.compiling.is_none() && self.failed != Some(self.revision)
    }
    pub fn source(&mut self) {
        self.layer = Layer::Source;
    }
    pub fn enter_preview(&mut self) -> bool {
        self.layer = Layer::Preview;
        if self.stale() && self.compiling.is_none() {
            self.compiling = Some(self.revision);
            true
        } else {
            false
        }
    }
    pub fn finished(&mut self, success: bool) {
        if let Some(revision) = self.compiling.take() {
            if success {
                self.built = Some(revision);
                self.failed = None;
            } else {
                self.failed = Some(revision);
            }
        }
    }
    pub fn preview_failed(&mut self) {
        self.built = None;
        self.failed = Some(self.revision);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_revision_waits_for_an_edit_or_explicit_retry() {
        let mut state = Layers::default();
        assert!(state.can_auto_build());
        assert!(state.enter_preview());
        assert!(!state.can_auto_build());
        state.finished(false);
        assert!(!state.can_auto_build());
        state.edited();
        assert!(state.can_auto_build());
        assert!(state.enter_preview());
        state.finished(false);
        assert!(state.enter_preview());
        state.finished(true);
        assert!(!state.can_auto_build());
    }
    #[test]
    fn unreadable_pdf_stays_stale_without_an_automatic_build_loop() {
        let mut state = Layers::default();
        assert!(state.enter_preview());
        state.finished(true);
        state.preview_failed();
        assert!(state.stale());
        assert!(!state.can_auto_build());
        assert!(state.enter_preview());
        state.finished(true);
        assert!(!state.stale());
    }
    #[test]
    fn preview_is_compiled_once_per_revision_and_failed_builds_stay_stale() {
        let mut state = Layers::default();
        assert!(state.enter_preview());
        assert!(!state.enter_preview()); // one in-flight build
        state.finished(true);
        state.source();
        assert!(!state.enter_preview()); // cache hit
        state.edited();
        assert!(state.enter_preview());
        state.finished(false);
        assert!(state.stale());
        assert!(state.enter_preview()); // explicit retry
    }
    #[test]
    fn edits_during_compilation_and_switching_back_do_not_publish_a_fresh_layer() {
        let mut state = Layers::default();
        assert!(state.enter_preview());
        state.edited();
        state.source();
        state.finished(true);
        assert_eq!(state.layer, Layer::Source);
        assert!(state.stale());
        assert!(state.enter_preview());
        state.finished(true);
        assert!(!state.stale());
    }
}

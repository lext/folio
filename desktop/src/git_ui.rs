use eframe::egui::{self, RichText};
use folio_core::git::{self, ChangeKind, Commit, Comparison, Hunk};
use std::{
    path::PathBuf,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
struct Request {
    id: u64,
    path: PathBuf,
    revision: String,
    text: String,
}
struct Reply {
    id: u64,
    comparison: Result<Comparison, String>,
    history: Vec<Commit>,
}
pub struct Git {
    tx: mpsc::Sender<Request>,
    rx: mpsc::Receiver<Reply>,
    id: u64,
    path: PathBuf,
    text: String,
    pub revision: String,
    dirty: bool,
    due: Instant,
    refreshed: Instant,
    pub comparison: Option<Comparison>,
    history: Vec<Commit>,
    error: Option<String>,
    detail: Option<Hunk>,
    custom_revision: String,
}
impl Git {
    pub fn new(ctx: egui::Context) -> Self {
        let (tx, requests) = mpsc::channel::<Request>();
        let (replies, rx) = mpsc::channel();
        thread::spawn(move || {
            while let Ok(mut request) = requests.recv() {
                while let Ok(newer) = requests.try_recv() {
                    request = newer;
                }
                let comparison = git::compare(&request.path, &request.revision, &request.text);
                let history = if comparison.is_ok() {
                    git::history(&request.path).unwrap_or_default()
                } else {
                    vec![]
                };
                if replies
                    .send(Reply {
                        id: request.id,
                        comparison,
                        history,
                    })
                    .is_err()
                {
                    break;
                }
                ctx.request_repaint();
            }
        });
        Self {
            tx,
            rx,
            id: 0,
            path: PathBuf::new(),
            text: String::new(),
            revision: "HEAD".into(),
            dirty: true,
            due: Instant::now(),
            refreshed: Instant::now(),
            comparison: None,
            history: vec![],
            error: None,
            detail: None,
            custom_revision: String::new(),
        }
    }
    pub fn tick(&mut self, ctx: &egui::Context, path: &std::path::Path, text: &str) {
        if self.path != path || self.text != text {
            let switched = self.path != path;
            self.path = path.into();
            self.text = text.into();
            self.invalidate();
            if switched {
                self.due = Instant::now();
            }
        }
        while let Ok(reply) = self.rx.try_recv() {
            self.apply(reply);
        }
        if !self.dirty && self.refreshed.elapsed() >= Duration::from_secs(2) {
            self.id += 1;
            self.dirty = true;
            self.due = Instant::now();
        }
        if self.dirty {
            if Instant::now() >= self.due {
                let _ = self.tx.send(Request {
                    id: self.id,
                    path: self.path.clone(),
                    revision: self.revision.clone(),
                    text: self.text.clone(),
                });
                self.dirty = false;
                self.refreshed = Instant::now();
            } else {
                ctx.request_repaint_after(self.due.saturating_duration_since(Instant::now()));
            }
        }
    }
    fn apply(&mut self, reply: Reply) {
        if reply.id != self.id {
            return;
        }
        self.history = reply.history;
        match reply.comparison {
            Ok(comparison) => {
                self.comparison = Some(comparison);
                self.error = None;
            }
            Err(error) => {
                self.comparison = None;
                self.error = Some(error);
            }
        }
    }
    fn invalidate(&mut self) {
        self.id += 1;
        self.dirty = true;
        self.due = Instant::now() + Duration::from_millis(250);
        self.comparison = None;
        self.detail = None;
    }
    pub fn comparison_ui(&mut self, ui: &mut egui::Ui, line: usize) -> Option<usize> {
        let mut jump = None;
        ui.label("Compare working text against");
        let mut revision = self.revision.clone();
        if ui
            .selectable_label(revision == "HEAD", "HEAD · latest commit")
            .clicked()
        {
            revision = "HEAD".into();
            ui.close();
        }
        for commit in &self.history {
            if ui
                .selectable_label(
                    revision == commit.oid,
                    format!("{}  {}", &commit.oid[..7], commit.subject),
                )
                .clicked()
            {
                revision = commit.oid.clone();
                ui.close();
            }
        }
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.custom_revision)
                    .desired_width(150.)
                    .hint_text("Commit / tag / HEAD~1"),
            );
            if ui.button("Compare").clicked() && !self.custom_revision.trim().is_empty() {
                revision = self.custom_revision.trim().into();
                ui.close();
            }
        });
        if revision != self.revision {
            self.revision = revision;
            self.invalidate();
        }
        ui.separator();
        if let Some(c) = &self.comparison {
            ui.weak(format!("{} · {}", self.revision, c.root.display()));
            if c.hunks.is_empty() {
                ui.weak("No changes");
            } else {
                if ui.button("Next change").clicked() {
                    jump = c
                        .hunks
                        .iter()
                        .find(|h| h.anchor_line() > line)
                        .or(c.hunks.first())
                        .map(Hunk::anchor_line);
                    ui.close();
                }
                if ui.button("Previous change").clicked() {
                    jump = c
                        .hunks
                        .iter()
                        .rev()
                        .find(|h| h.anchor_line() < line)
                        .or(c.hunks.last())
                        .map(Hunk::anchor_line);
                    ui.close();
                }
            }
        } else if let Some(error) = &self.error {
            ui.add(egui::Label::new(error).wrap());
        } else {
            ui.weak("Reading Git…");
        }
        ui.weak("Green: added · blue: modified · red: deleted");
        jump
    }
    pub fn gutter(
        &mut self,
        ui: &mut egui::Ui,
        right: f32,
        tops: &[f32],
        first: usize,
        last: usize,
    ) {
        // `tops[line]` is the screen y of a zero-based source line; one extra
        // entry closes the last line. Wrapped lines are taller than one row.
        let lines = tops.len().saturating_sub(1);
        let Some(comparison) = &self.comparison else {
            return;
        };
        for (i, hunk) in comparison.hunks.iter().enumerate() {
            let start = hunk.anchor_line() - 1;
            let end = start + hunk.new_count.max(1);
            if end <= first || start >= last || start >= lines {
                continue;
            }
            let (from, to) = (start.max(first), end.min(last).min(lines));
            let deleted = hunk.kind() == ChangeKind::Deleted;
            let rect = egui::Rect::from_min_size(
                egui::pos2(
                    right - 3.,
                    if deleted && hunk.new_start > 0 {
                        tops[from + 1] - 3.
                    } else {
                        tops[from]
                    },
                ),
                egui::vec2(3., if deleted { 3. } else { tops[to] - tops[from] }),
            );
            let color = match hunk.kind() {
                ChangeKind::Added => crate::theme::of(ui).added,
                ChangeKind::Modified => crate::theme::of(ui).modified,
                ChangeKind::Deleted => crate::theme::of(ui).deleted,
            };
            ui.painter().rect_filled(rect, 1., color);
            if ui
                .interact(
                    rect.expand2(egui::vec2(5., 1.)),
                    ui.id().with(("git-change", i)),
                    egui::Sense::click(),
                )
                .on_hover_text("Click to see changes against the selected commit")
                .clicked()
            {
                self.detail = Some(hunk.clone());
            }
        }
    }
    pub fn dismiss_detail(&mut self) {
        self.detail = None;
    }
    pub fn detail(&mut self, ctx: &egui::Context) {
        if let Some(hunk) = &self.detail {
            let mut open = true;
            egui::Window::new(format!("Changes against {}", self.revision))
                .open(&mut open)
                .collapsible(false)
                .default_width(660.)
                .show(ctx, |ui| {
                    egui::ScrollArea::both().max_height(360.).show(ui, |ui| {
                        for (prefix, lines, color) in [
                            ("−", &hunk.before, crate::theme::of(ui).deleted),
                            ("+", &hunk.after, crate::theme::of(ui).added),
                        ] {
                            for line in lines {
                                ui.label(
                                    RichText::new(format!("{prefix} {line}"))
                                        .monospace()
                                        .color(color),
                                );
                            }
                        }
                    });
                });
            if !open {
                self.detail = None;
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edits_discard_old_commit_comparisons_and_stale_worker_replies() {
        let mut git = Git::new(egui::Context::default());
        let id = git.id;
        git.invalidate();
        git.apply(Reply {
            id,
            comparison: Ok(Comparison {
                root: PathBuf::new(),
                commit: None,
                hunks: vec![],
            }),
            history: vec![],
        });
        assert!(git.comparison.is_none());
        git.apply(Reply {
            id: git.id,
            comparison: Ok(Comparison {
                root: PathBuf::new(),
                commit: None,
                hunks: vec![],
            }),
            history: vec![],
        });
        assert!(git.comparison.is_some());
        git.invalidate();
        assert!(git.comparison.is_none());
    }
}

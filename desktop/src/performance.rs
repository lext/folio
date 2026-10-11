//! Opt-in, local native-frame recording. No logging or forced redraws normally.
use eframe::egui;
use std::{io::Write, path::PathBuf, time::Instant};

pub struct FrameTrace {
    path: PathBuf,
    adapter: String,
    start: Option<Instant>,
    previous: Option<Instant>,
    rows: Vec<[f64; 6]>,
    complete: bool,
}
impl FrameTrace {
    pub fn from_env(cc: &eframe::CreationContext<'_>) -> Option<Self> {
        let path = std::env::var_os("FOLIO_FRAME_TRACE")?.into();
        let adapter = cc
            .wgpu_render_state
            .as_ref()
            .map(|gpu| {
                let info = gpu.adapter.get_info();
                format!("{} / {:?}", info.name, info.backend)
            })
            .unwrap_or_default();
        Some(Self {
            path,
            adapter,
            start: None,
            previous: None,
            rows: Vec::with_capacity(8192),
            complete: false,
        })
    }
    pub fn record(
        &mut self,
        ctx: &egui::Context,
        preview: bool,
        update_start: Instant,
        cpu: Option<f32>,
    ) {
        if self.complete || (!preview && self.start.is_none()) {
            return;
        }
        let start = *self.start.get_or_insert(update_start);
        let seconds = update_start.duration_since(start).as_secs_f64();
        let interval = self.previous.replace(update_start).map_or(0., |last| {
            update_start.duration_since(last).as_secs_f64() * 1000.
        });
        let input_events = ctx.input(|i| {
            i.events
                .iter()
                .filter(|e| matches!(e, egui::Event::MouseWheel { .. } | egui::Event::Zoom(_)))
                .count()
        });
        self.rows.push([
            seconds,
            interval,
            update_start.elapsed().as_secs_f64() * 1000.,
            cpu.map_or(0., |s| f64::from(s) * 1000.),
            input_events as f64,
            u8::from(preview) as f64,
        ]);
        if seconds >= 30. || self.rows.len() >= 16_384 {
            self.complete = true;
            let path = self.path.clone();
            let adapter = self.adapter.clone();
            let rows = std::mem::take(&mut self.rows);
            std::thread::spawn(move || {
                if let Err(error) = write_trace(&path, &adapter, &rows) {
                    eprintln!("Frame trace: {error}");
                }
            });
        } else {
            // Continuous redraws make frame intervals meaningful during this
            // 30-second diagnostic run. Normal Folio stays event driven.
            ctx.request_repaint();
        }
    }
}
fn write_trace(path: &std::path::Path, adapter: &str, rows: &[[f64; 6]]) -> std::io::Result<()> {
    let mut out = std::io::BufWriter::new(std::fs::File::create(path)?);
    writeln!(out, "# Folio {} | {adapter}", env!("CARGO_PKG_VERSION"))?;
    writeln!(
        out,
        "# Native callback intervals; CPU includes render submission, excludes vsync. These are not GPU timestamp or input-to-photon measurements."
    )?;
    writeln!(
        out,
        "seconds,interval_ms,update_ms,previous_cpu_ms,gesture_events,preview"
    )?;
    for row in rows {
        writeln!(
            out,
            "{:.6},{:.6},{:.6},{:.6},{},{}",
            row[0], row[1], row[2], row[3], row[4], row[5]
        )?;
    }
    out.flush()
}

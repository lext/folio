mod app;
mod file_browser;
mod git_ui;
mod language;
mod launcher;
mod layers;
mod navigation;
mod page_layout;
mod pdf_worker;
mod performance;
mod preview;
mod project;
mod settings;
mod theme;
mod workspace;
use std::path::PathBuf;
fn main() -> eframe::Result<()> {
    let path = std::env::args_os().nth(1).map(PathBuf::from);
    let (loaded, error) = match settings::Settings::load() {
        Ok(loaded) => (loaded, None),
        Err(error) => {
            eprintln!("{error}");
            (
                settings::Loaded {
                    settings: Default::default(),
                    path: None,
                },
                Some(error),
            )
        }
    };
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration {
            desired_maximum_frame_latency: Some(1),
            ..Default::default()
        },
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size(loaded.settings.window_size)
            .with_min_inner_size([1000., 600.])
            .with_app_id("dev.folio.editor")
            .with_fullsize_content_view(true)
            .with_title_shown(false)
            .with_titlebar_shown(false),
        ..Default::default()
    };
    eframe::run_native(
        "Folio — LaTeX, locally",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, path, loaded, error)))),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_gpu_backend_is_compiled_in() {
        let backends = eframe::wgpu::Instance::enabled_backend_features();
        #[cfg(target_os = "macos")]
        assert!(backends.contains(eframe::wgpu::Backends::METAL));
        #[cfg(target_os = "linux")]
        assert!(backends.intersects(eframe::wgpu::Backends::VULKAN | eframe::wgpu::Backends::GL));
    }
}

mod render_queue;

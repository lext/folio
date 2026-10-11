use crate::render_queue::{Queue, Task};
use eframe::egui;
use folio_core::pdf::{Glyph, PageText, Rect};
use pdfium_render::prelude::*;
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

pub enum Request {
    Open { path: PathBuf, generation: u64 },
    Viewport { generation: u64, tasks: Vec<Task> },
}
#[derive(Clone, Copy, Debug)]
pub enum Operation {
    Backend,
    Open,
    Render(Task),
    Index(usize),
}
pub enum Event {
    Opened {
        generation: u64,
        sizes: Vec<(f32, f32)>,
    },
    Indexed {
        generation: u64,
        page: usize,
        text: PageText,
    },
    Rendered {
        generation: u64,
        page: usize,
        width: i32,
        image: egui::ColorImage,
    },
    Error {
        generation: u64,
        operation: Operation,
        message: String,
    },
}
pub struct Worker {
    pub tx: Sender<Request>,
    pub rx: Receiver<Event>,
}
pub fn bind() -> Result<Pdfium, String> {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("FOLIO_PDFIUM_LIB") {
        candidates.push(PathBuf::from(path));
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        candidates.push(Pdfium::pdfium_platform_library_name_at_path(dir));
    }
    candidates.push(Pdfium::pdfium_platform_library_name_at_path("runtime/lib"));
    candidates.push(Pdfium::pdfium_platform_library_name_at_path(
        "../runtime/lib",
    ));
    for path in candidates {
        match Pdfium::bind_to_library(path) {
            Ok(bindings) => return Ok(Pdfium::new(bindings)),
            // pdfium-render owns process-wide bindings. Default explicitly
            // reuses them once initialized, even after earlier handles drop.
            Err(PdfiumError::PdfiumLibraryBindingsAlreadyInitialized) => {
                return Ok(Pdfium::default());
            }
            Err(_) => {}
        }
    }
    match Pdfium::bind_to_system_library() {
        Ok(bindings) => Ok(Pdfium::new(bindings)),
        Err(PdfiumError::PdfiumLibraryBindingsAlreadyInitialized) => Ok(Pdfium::default()),
        Err(e) => Err(format!(
            "PDFium is unavailable: {e}. Run scripts/setup-pdfium.sh or set FOLIO_PDFIUM_LIB to the library path."
        )),
    }
}
pub fn index_page(page: &PdfPage<'_>) -> Result<PageText, PdfiumError> {
    let text = page.text()?;
    let height = page.height().value;
    let mut glyphs = vec![];
    for c in text.chars().iter() {
        if let Some(ch) = c.unicode_char() {
            let rect = c
                .loose_bounds()
                .map(|r| Rect {
                    x: r.left().value,
                    y: height - r.top().value,
                    width: r.width().value,
                    height: r.height().value,
                })
                .unwrap_or_default();
            glyphs.push(Glyph { ch, rect });
        }
    }
    Ok(PageText {
        width: page.width().value,
        height,
        glyphs,
    })
}
pub fn render_page(page: &PdfPage<'_>, width: i32) -> Result<egui::ColorImage, PdfiumError> {
    let bitmap = page.render_with_config(
        &PdfRenderConfig::new()
            .set_target_width(width)
            .set_maximum_height(8192),
    )?;
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [bitmap.width() as usize, bitmap.height() as usize],
        &bitmap.as_rgba_bytes(),
    ))
}
impl Worker {
    pub fn start(ctx: egui::Context) -> Self {
        let (tx, rx) = mpsc::channel();
        // Bound completed images: a fast renderer must not accumulate large RGBA
        // buffers while the event thread is busy or the window is minimized.
        let (events, event_rx) = mpsc::sync_channel(2);
        thread::spawn(move || {
            let pdfium = match bind() {
                Ok(p) => p,
                Err(message) => {
                    let _ = events.send(Event::Error {
                        generation: 0,
                        operation: Operation::Backend,
                        message,
                    });
                    ctx.request_repaint();
                    return;
                }
            };
            let mut document: Option<PdfDocument<'_>> = None;
            let mut current = 0;
            let mut next_index = 0;
            let mut queue = Queue::default();
            let mut incoming = None;
            loop {
                // Drain commands before doing expensive work. The newest viewport
                // replaces obsolete scroll/zoom work instead of joining a FIFO.
                loop {
                    let request = incoming.take().or_else(|| rx.try_recv().ok());
                    let Some(request) = request else {
                        break;
                    };
                    match request {
                        Request::Open { path, generation } => {
                            let opened =
                                std::fs::read(path)
                                    .map_err(|e| e.to_string())
                                    .and_then(|bytes| {
                                        pdfium
                                            .load_pdf_from_byte_vec(bytes, None)
                                            .map_err(|e| e.to_string())
                                    });
                            match opened {
                                Ok(doc) => {
                                    let sizes: Vec<_> = doc
                                        .pages()
                                        .iter()
                                        .map(|p| (p.width().value, p.height().value))
                                        .collect();
                                    if sizes.iter().any(|&(w, h)| {
                                        !w.is_finite() || !h.is_finite() || w <= 0. || h <= 0.
                                    }) {
                                        if events
                                            .send(Event::Error {
                                                generation,
                                                operation: Operation::Open,
                                                message: "PDF contains an invalid page size".into(),
                                            })
                                            .is_err()
                                        {
                                            return;
                                        }
                                        ctx.request_repaint();
                                        continue;
                                    }
                                    document = Some(doc);
                                    current = generation;
                                    next_index = 0;
                                    queue.replace(vec![]);
                                    if events.send(Event::Opened { generation, sizes }).is_err() {
                                        return;
                                    }
                                }
                                Err(message) => {
                                    if events
                                        .send(Event::Error {
                                            generation,
                                            operation: Operation::Open,
                                            message,
                                        })
                                        .is_err()
                                    {
                                        return;
                                    }
                                }
                            }
                        }
                        Request::Viewport { generation, tasks } if generation == current => {
                            queue.replace(tasks)
                        }
                        _ => {}
                    }
                    ctx.request_repaint();
                }
                if let Some(task) = queue.next()
                    && let Some(doc) = &document
                {
                    let rendered = doc
                        .pages()
                        .get(task.page as i32)
                        .and_then(|page| render_page(&page, task.width));
                    let event = match rendered {
                        Ok(image) => Event::Rendered {
                            generation: current,
                            page: task.page,
                            width: task.width,
                            image,
                        },
                        Err(e) => Event::Error {
                            generation: current,
                            operation: Operation::Render(task),
                            message: e.to_string(),
                        },
                    };
                    if events.send(event).is_err() {
                        return;
                    }
                    ctx.request_repaint();
                    continue;
                }
                // Background indexing only runs after visible rendering work.
                if let Some(doc) = &document
                    && next_index < doc.pages().len() as usize
                {
                    if let Ok(page) = doc.pages().get(next_index as i32) {
                        let event = match index_page(&page) {
                            Ok(text) => Event::Indexed {
                                generation: current,
                                page: next_index,
                                text,
                            },
                            Err(e) => Event::Error {
                                generation: current,
                                operation: Operation::Index(next_index),
                                message: e.to_string(),
                            },
                        };
                        if events.send(event).is_err() {
                            return;
                        }
                    }
                    next_index += 1;
                    ctx.request_repaint();
                    continue;
                }
                match rx.recv() {
                    Ok(request) => incoming = Some(request),
                    Err(_) => break,
                }
            }
        });
        Self { tx, rx: event_rx }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires PDFium"]
    fn repeated_binding_reuses_the_initialized_pdfium_library() {
        let first = bind().unwrap();
        let second = bind().expect("an initialized PDFium library is available, not an error");
        drop(first);
        assert!(second.create_new_pdf().is_ok());
    }
    #[test]
    #[ignore = "requires PDFium and FOLIO_TEST_PDF pointing to a built fixture"]
    fn failed_replacement_keeps_the_previous_document_renderable() {
        let worker = Worker::start(egui::Context::default());
        worker
            .tx
            .send(Request::Open {
                path: std::env::var("FOLIO_TEST_PDF").unwrap().into(),
                generation: 1,
            })
            .unwrap();
        let next = || {
            worker
                .rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap()
        };
        loop {
            match next() {
                Event::Opened { generation: 1, .. } => break,
                Event::Error { message, .. } => panic!("{message}"),
                _ => {}
            }
        }
        worker
            .tx
            .send(Request::Open {
                path: "/does-not-exist/folio.pdf".into(),
                generation: 2,
            })
            .unwrap();
        loop {
            if let Event::Error {
                generation: 2,
                operation: Operation::Open,
                ..
            } = next()
            {
                break;
            }
        }
        worker
            .tx
            .send(Request::Viewport {
                generation: 1,
                tasks: vec![Task {
                    page: 0,
                    width: 256,
                }],
            })
            .unwrap();
        loop {
            match next() {
                Event::Rendered {
                    generation: 1,
                    image,
                    ..
                } => {
                    assert!(!image.pixels.is_empty());
                    break;
                }
                Event::Error { message, .. } => panic!("{message}"),
                _ => {}
            }
        }
    }
    #[test]
    #[ignore = "requires PDFium and FOLIO_TEST_PDF pointing to a built fixture"]
    fn rendered_pdf_has_searchable_text_and_real_pixels() {
        let pdfium = bind().unwrap();
        let path = std::env::var("FOLIO_TEST_PDF").unwrap();
        let doc = pdfium.load_pdf_from_file(&path, None).unwrap();
        let page = doc.pages().get(0).unwrap();
        let index = index_page(&page).unwrap();
        assert!(!index.find("Folio").is_empty());
        let glyph = index.glyphs.iter().find(|g| g.ch == 'F').unwrap();
        assert!(
            index
                .hit_test(glyph.rect.x + 1., glyph.rect.y + 1.)
                .is_some()
        );
        let bitmap = page
            .render_with_config(&PdfRenderConfig::new().set_target_width(800))
            .unwrap();
        assert_eq!(bitmap.width(), 800);
        assert!(bitmap.as_rgba_bytes().iter().any(|b| *b < 100));
        // Exercise the same indexed glyph -> top-left PDF point -> SyncTeX path
        // used by a real click, rather than testing the two adapters separately.
        let range = index
            .find("Folio is a local workspace")
            .into_iter()
            .next()
            .unwrap();
        let glyph = &index.glyphs[range.start];
        let source = folio_core::synctex::inverse(
            std::path::Path::new(&path),
            1,
            glyph.rect.x + glyph.rect.width / 2.,
            glyph.rect.y + glyph.rect.height / 2.,
        )
        .unwrap();
        assert_eq!(source.path.file_name().unwrap(), "main.tex");
        assert!((14..=17).contains(&source.line), "{}", source.line);
        assert_eq!(doc.pages().len(), 2);
        let second = doc.pages().get(1).unwrap();
        let second_index = index_page(&second).unwrap();
        assert!(
            !second_index
                .find("Search works across every indexed page")
                .is_empty()
        );
        let bitmap = second
            .render_with_config(&PdfRenderConfig::new().set_target_width(800))
            .unwrap();
        assert!(bitmap.as_rgba_bytes().iter().any(|b| *b < 100));
    }
}

#[cfg(test)]
mod performance {
    use super::*;
    use std::time::Instant;
    #[test]
    #[ignore = "manual PDF profiling; requires PDFium and FOLIO_TEST_PDF"]
    fn profile_pdf_pipeline() {
        let start = Instant::now();
        let pdfium = bind().unwrap();
        let doc = pdfium
            .load_pdf_from_file(&std::env::var("FOLIO_TEST_PDF").unwrap(), None)
            .unwrap();
        let opened = start.elapsed();
        let start = Instant::now();
        let sizes: Vec<_> = doc
            .pages()
            .iter()
            .map(|p| (p.width().value, p.height().value))
            .collect();
        let measured = start.elapsed();
        let page = doc.pages().get(0).unwrap();
        let start = Instant::now();
        let index = index_page(&page).unwrap();
        let indexed = start.elapsed();
        let start = Instant::now();
        let bitmap = page
            .render_with_config(&PdfRenderConfig::new().set_target_width(1856))
            .unwrap();
        let rendered = start.elapsed();
        let start = Instant::now();
        let rgba = bitmap.as_rgba_bytes();
        let copied = start.elapsed();
        let start = Instant::now();
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [bitmap.width() as usize, bitmap.height() as usize],
            &rgba,
        );
        let converted = start.elapsed();
        std::hint::black_box(image);
        eprintln!(
            "PDF PROFILE pages={} glyphs={} open={opened:?} all_dimensions={measured:?} index_page={indexed:?} render_1856={rendered:?} rgba={copied:?} color_conversion_now_on_worker={converted:?}",
            sizes.len(),
            index.glyphs.len()
        );
    }
}

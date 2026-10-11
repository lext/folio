use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    ops::Range,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Debug)]
pub struct Document {
    pub path: PathBuf,
    pub text: String,
    saved: String,
}
impl Document {
    pub fn open(path: &Path) -> io::Result<Self> {
        let path = path.canonicalize()?;
        let text = read_utf8(&path)?;
        Ok(Self {
            path,
            saved: text.clone(),
            text,
        })
    }
    pub fn is_dirty(&self) -> bool {
        self.text != self.saved
    }
    pub fn save(&mut self) -> io::Result<()> {
        self.check_disk()?;
        if !self.is_dirty() {
            return Ok(());
        }
        atomic_write(&self.path, self.text.as_bytes())?;
        self.saved.clone_from(&self.text);
        Ok(())
    }
    pub fn check_disk(&self) -> io::Result<()> {
        // Read even when clean: a missing/changed file needs a conscious reload.
        if read_utf8(&self.path)? != self.saved {
            return Err(io::Error::other(
                "File changed on disk. Reopen it or save a copy before overwriting.",
            ));
        }
        Ok(())
    }
}
fn read_utf8(path: &Path) -> io::Result<String> {
    String::from_utf8(fs::read(path)?).map_err(|_| io::Error::new(io::ErrorKind::InvalidData,
        format!("{} is not UTF-8. Convert a copy to UTF-8 before opening it in Folio; the original file has not been changed.", path.display())))
}
static SERIAL: AtomicU64 = AtomicU64::new(0);
pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::other("Missing filename"))?
        .to_string_lossy();
    let tmp = path.with_file_name(format!(
        ".{name}.folio-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        if let Ok(meta) = fs::metadata(path) {
            file.set_permissions(meta.permissions())?;
        }
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}
/// One-based lines, zero-based Unicode scalar offsets for egui's text cursor.
pub fn line_char_range(text: &str, line: usize) -> Range<usize> {
    let mut start = 0;
    let mut current = 1;
    let (mut length, mut previous) = (0, '\0');
    for (i, ch) in text.chars().enumerate() {
        length = i + 1;
        if ch == '\n' {
            if current == line.max(1) {
                let end = if i > start && previous == '\r' {
                    i - 1
                } else {
                    i
                };
                return start..end;
            }
            current += 1;
            start = i + 1;
        }
        previous = ch;
    }
    if current == line.max(1) {
        start..length
    } else {
        length..length
    }
}
pub fn char_position(text: &str, offset: usize) -> (usize, usize) {
    let (mut line, mut column) = (1, 1);
    for ch in text.chars().take(offset) {
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}
pub fn char_to_byte(text: &str, offset: usize) -> usize {
    text.char_indices()
        .nth(offset)
        .map_or(text.len(), |(i, _)| i)
}

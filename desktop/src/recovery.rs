//! Crash recovery for unsaved edits. Copies live in the project's ignored
//! `.folio/recovery/` folder and are written away from the frame thread.
use std::{
    fs,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
};

const HEADER: &str = "folio-recovery 1 ";

enum Job {
    Write(PathBuf, String),
    Remove(PathBuf),
}

/// One worker applies writes and removals in order, so a removal queued after
/// a save can never be overtaken by an older copy of the same buffer.
pub struct Recovery {
    tx: Option<mpsc::Sender<Job>>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Default for Recovery {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            for job in rx {
                match job {
                    Job::Write(file, contents) => {
                        // Best effort: a failed copy must never disturb editing.
                        let _ = file
                            .parent()
                            .map_or(Ok(()), fs::create_dir_all)
                            .and_then(|()| {
                                folio_core::document::atomic_write(&file, contents.as_bytes())
                            });
                    }
                    Job::Remove(file) => {
                        let _ = fs::remove_file(file);
                    }
                }
            }
        });
        Self {
            tx: Some(tx),
            worker: Some(worker),
        }
    }
}

impl Recovery {
    /// Keep `text`, an unsaved edit of the file whose saved content is `saved`.
    pub fn write(&self, file: PathBuf, saved: &str, text: &str) {
        self.send(Job::Write(file, encode(saved, text)));
    }
    pub fn remove(&self, file: PathBuf) {
        self.send(Job::Remove(file));
    }
    fn send(&self, job: Job) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(job);
        }
    }
    /// Wait until every queued job has been applied.
    #[cfg(test)]
    pub fn flush(&mut self) {
        self.tx = None;
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        *self = Self::default();
    }
}

impl Drop for Recovery {
    fn drop(&mut self) {
        // Finish queued work so a deliberate discard really removes its copy.
        self.tx = None;
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Where the recovery copy of `source` lives, for a source inside `project`.
pub fn file(project: &Path, source: &Path) -> Option<PathBuf> {
    let relative = source.strip_prefix(project).ok()?;
    let mut name = relative.file_name()?.to_owned();
    name.push(".recover");
    Some(
        project
            .join(".folio/recovery")
            .join(relative)
            .with_file_name(name),
    )
}

// FNV-1a: stable across Rust releases, unlike the standard library's hasher.
fn digest(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn encode(saved: &str, text: &str) -> String {
    format!("{HEADER}{:016x}\n{text}", digest(saved))
}

pub enum Found {
    Nothing,
    /// Unsaved text that was based on exactly the file now on disk.
    Edits(String),
    /// The file changed since the copy was made; the copy was set aside here.
    Orphaned(PathBuf),
}

/// Look for a recovery copy when a source is opened with content `disk`.
pub fn take(file: &Path, disk: &str) -> Found {
    let Ok(contents) = fs::read_to_string(file) else {
        return Found::Nothing;
    };
    let parsed = contents
        .strip_prefix(HEADER)
        .and_then(|rest| rest.split_once('\n'))
        .and_then(|(hash, text)| Some((u64::from_str_radix(hash, 16).ok()?, text)));
    match parsed {
        Some((base, text)) if base == digest(disk) => {
            if text == disk {
                let _ = fs::remove_file(file);
                Found::Nothing
            } else {
                Found::Edits(text.to_owned())
            }
        }
        // Never apply edits to a different base, and never throw them away.
        _ => {
            let mut name = file.file_name().unwrap_or_default().to_owned();
            name.push(".orphaned");
            let aside = file.with_file_name(name);
            match fs::rename(file, &aside) {
                Ok(()) => Found::Orphaned(aside),
                Err(_) => Found::Nothing,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_restore_only_onto_the_file_they_were_based_on() {
        let dir = std::env::temp_dir().join(format!("folio recovery {}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("sections")).unwrap();
        let source = dir.join("sections/intro.tex");
        let file = file(&dir, &source).unwrap();
        assert_eq!(file, dir.join(".folio/recovery/sections/intro.tex.recover"));
        assert!(super::file(&dir, Path::new("/elsewhere/a.tex")).is_none());
        assert!(matches!(take(&file, "saved"), Found::Nothing));

        let mut recovery = Recovery::default();
        recovery.write(file.clone(), "saved", "first edit");
        recovery.write(file.clone(), "saved", "saved\nUnicode édit\n");
        recovery.flush();
        match take(&file, "saved") {
            Found::Edits(text) => assert_eq!(text, "saved\nUnicode édit\n"),
            _ => panic!("the newest copy must be restored"),
        }
        assert!(
            file.exists(),
            "a restored copy stays until the buffer is saved"
        );

        // The file changed underneath: keep the edits, but do not apply them.
        match take(&file, "changed by git pull") {
            Found::Orphaned(aside) => {
                assert!(!file.exists());
                assert!(
                    fs::read_to_string(aside)
                        .unwrap()
                        .ends_with("Unicode édit\n")
                );
            }
            _ => panic!("a copy for another base must be set aside"),
        }

        recovery.write(file.clone(), "saved", "edit");
        recovery.remove(file.clone());
        recovery.flush();
        assert!(!file.exists(), "jobs are applied in order");
        fs::write(&file, "not a recovery file").unwrap();
        assert!(matches!(take(&file, "saved"), Found::Orphaned(_)));
        fs::remove_dir_all(dir).unwrap();
    }
}

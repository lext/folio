use std::{
    fs, io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    PdfLatex,
    XeLatex,
    LuaLatex,
}
impl Engine {
    pub fn label(self) -> &'static str {
        match self {
            Self::PdfLatex => "pdfLaTeX",
            Self::XeLatex => "XeLaTeX",
            Self::LuaLatex => "LuaLaTeX",
        }
    }
}
#[derive(Debug, Clone)]
pub struct BuildConfig {
    pub root: PathBuf,
    pub engine: Engine,
    pub timeout: Duration,
    pub use_latexmkrc: bool,
    pub run_bibliography: bool,
}
impl BuildConfig {
    pub fn new(root: PathBuf, engine: Engine) -> Self {
        Self {
            root,
            engine,
            timeout: Duration::from_secs(120),
            use_latexmkrc: true,
            run_bibliography: true,
        }
    }
    pub fn output_dir(&self) -> PathBuf {
        self.root.parent().unwrap_or(Path::new(".")).join(".folio")
    }
    pub fn pdf_path(&self) -> PathBuf {
        self.output_dir()
            .join(self.root.file_name().unwrap_or_default())
            .with_extension("pdf")
    }
    pub fn arguments(&self) -> Vec<String> {
        let mut args = vec![
            match self.engine {
                Engine::PdfLatex => "-pdf",
                Engine::XeLatex => "-xelatex",
                Engine::LuaLatex => "-lualatex",
            }
            .into(),
            "-synctex=1".into(),
            "-interaction=nonstopmode".into(),
            "-file-line-error".into(),
            "-halt-on-error".into(),
            "-latexoption=-no-shell-escape".into(),
            format!("-outdir={}", self.output_dir().display()),
            self.root
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        ];
        if !self.use_latexmkrc {
            args.insert(0, "-norc".into());
        }
        if !self.run_bibliography {
            args.insert(0, "-bibtex-".into());
        }
        args
    }
}
pub fn executable(name: &str) -> PathBuf {
    // GUI apps launched from Finder often do not inherit TeX's shell PATH.
    if let Some(paths) = std::env::var_os("PATH") {
        for p in std::env::split_paths(&paths) {
            let candidate = p.join(name);
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    for dir in [
        "/Library/TeX/texbin",
        "/opt/homebrew/bin",
        "/usr/local/bin",
        "/usr/bin",
    ] {
        let p = Path::new(dir).join(name);
        if p.is_file() {
            return p;
        }
    }
    PathBuf::from(name)
}
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub path: PathBuf,
    pub line: usize,
    pub message: String,
}
pub fn parse_diagnostics(log: &str) -> Vec<Diagnostic> {
    let mut result = vec![];
    for line in log.lines() {
        // Find :digits: rather than splitting file paths at every colon.
        for (i, _) in line.match_indices(':') {
            let rest = &line[i + 1..];
            if let Some(j) = rest.find(':')
                && let Ok(number) = rest[..j].parse::<usize>()
                && number > 0
                && Path::new(&line[..i]).extension().is_some_and(|e| {
                    // TeX can include arbitrary source extensions such as .tikz.
                    // A numeric clock/date prefix is not a source filename.
                    e.to_string_lossy().chars().any(char::is_alphabetic)
                })
            {
                result.push(Diagnostic {
                    path: line[..i].into(),
                    line: number,
                    message: rest[j + 1..].trim().into(),
                });
                break;
            }
        }
    }
    result
}
#[derive(Debug)]
pub struct BuildResult {
    pub success: bool,
    pub cancelled: bool,
    pub pdf: PathBuf,
    pub log: String,
    pub diagnostics: Vec<Diagnostic>,
    pub elapsed: Duration,
}
pub fn compile(config: &BuildConfig, cancel: Arc<AtomicBool>) -> io::Result<BuildResult> {
    compile_with_program(config, cancel, &executable("latexmk"))
}
fn compile_with_program(
    config: &BuildConfig,
    cancel: Arc<AtomicBool>,
    program: &Path,
) -> io::Result<BuildResult> {
    let root = config.root.canonicalize()?;
    let config = BuildConfig {
        root,
        ..config.clone()
    };
    fs::create_dir_all(config.output_dir())?;
    let interrupted = config.output_dir().join("build-interrupted");
    if interrupted.exists() {
        clean_interrupted_output(&config.output_dir())?;
    }
    let previous_pdf = fingerprint(&config.pdf_path());
    let previous_maps = ["synctex.gz", "synctex"].map(|extension| {
        (
            extension,
            fingerprint(&config.pdf_path().with_extension(extension)),
        )
    });
    let log_path = config.output_dir().join("build-output.txt");
    let stdout = fs::File::create(&log_path)?;
    let mut cmd = Command::new(program);
    // Finder's PATH often omits TeX. latexmk must also find its child engines,
    // bibliography tools and font utilities, not just the latexmk executable.
    let mut paths: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    for tool in [
        "latexmk", "pdflatex", "xelatex", "lualatex", "bibtex", "biber",
    ] {
        if let Some(dir) = executable(tool).parent()
            && !dir.as_os_str().is_empty()
            && !paths.iter().any(|p| p == dir)
        {
            paths.push(dir.to_path_buf());
        }
    }
    cmd.env(
        "PATH",
        std::env::join_paths(paths).map_err(io::Error::other)?,
    );
    cmd.current_dir(config.root.parent().unwrap())
        .args(config.arguments())
        .stdin(Stdio::null())
        .stdout(stdout.try_clone()?)
        .stderr(stdout);
    // Put latexmk and all TeX/BibTeX children in a separate process group.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let start = Instant::now();
    // A crash of Folio itself also leaves a recoverable marker.
    fs::write(&interrupted, b"Auxiliary files may be incomplete.\n")?;
    let mut child = cmd.spawn()?;
    let mut cancelled = false;
    let success = loop {
        if let Some(status) = child.try_wait()? {
            break status.success();
        }
        if cancel.load(Ordering::Relaxed)
            || (!config.timeout.is_zero() && start.elapsed() > config.timeout)
        {
            cancelled = true;
            terminate(&mut child);
            break false;
        }
        thread::sleep(Duration::from_millis(30));
    };
    let mut log = String::from_utf8_lossy(&fs::read(log_path)?).into_owned();
    if cancelled {
        log.push_str("\nBuild cancelled or timed out.\n");
    }
    let diagnostics = parse_diagnostics(&log);
    let success = success && config.pdf_path().is_file();
    let pdf = if success {
        let pdf_changed = previous_pdf != fingerprint(&config.pdf_path());
        let mut maps = Vec::new();
        for (extension, before) in previous_maps {
            let path = config.pdf_path().with_extension(extension);
            if pdf_changed && before == fingerprint(&path) {
                // Remove the stale working map too: a later no-op latexmk run
                // must not pair it with this PDF merely because neither changed.
                if path.is_file() {
                    fs::remove_file(path)?;
                }
            } else {
                maps.push(extension);
            }
        }
        let pdf = snapshot(&config, &maps)?;
        fs::remove_file(interrupted)?;
        pdf
    } else {
        config.pdf_path()
    };
    Ok(BuildResult {
        success,
        cancelled,
        pdf,
        log,
        diagnostics,
        elapsed: start.elapsed(),
    })
}
#[cfg(unix)]
fn signal_group(pid: u32, signal: i32) -> bool {
    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    // The child starts a new group with its PID; no editor process shares it.
    unsafe { kill(-(pid as i32), signal) == 0 }
}
pub(crate) fn terminate(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let pid = child.id();
        signal_group(pid, 15);
        let deadline = Instant::now() + Duration::from_millis(500);
        while Instant::now() < deadline && signal_group(pid, 0) {
            let _ = child.try_wait();
            thread::sleep(Duration::from_millis(20));
        }
        // The supervisor may have exited while a descendant ignored TERM.
        signal_group(pid, 9);
    }
    let _ = child.kill();
    let _ = child.wait();
}
fn clean_interrupted_output(dir: &Path) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            continue;
        }
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if kind.is_dir() && !name.starts_with("preview-") {
            clean_interrupted_output(&path)?;
        } else if kind.is_file()
            && (name.ends_with(".synctex.gz")
                || name.ends_with(".run.xml")
                || path.extension().is_some_and(|e| {
                    [
                        "aux",
                        "toc",
                        "bbl",
                        "bcf",
                        "blg",
                        "out",
                        "log",
                        "fls",
                        "fdb_latexmk",
                        "lof",
                        "lot",
                        "nav",
                        "snm",
                        "vrb",
                        "idx",
                        "ind",
                        "ilg",
                        "glo",
                        "gls",
                        "glg",
                        "acn",
                        "acr",
                        "alg",
                        "synctex",
                        "pdf",
                        "dvi",
                        "xdv",
                    ]
                    .iter()
                    .any(|x| e == *x)
                }))
        {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}
/// Immutable PDF/map pairs keep navigation correct while the next build runs.
/// A PDF without a map is still a useful preview.
fn fingerprint(path: &Path) -> Option<(std::time::SystemTime, u64)> {
    let metadata = fs::metadata(path).ok()?;
    Some((metadata.modified().ok()?, metadata.len()))
}
fn snapshot(config: &BuildConfig, maps: &[&str]) -> io::Result<PathBuf> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_nanos();
    let folder = config.output_dir().join(format!("preview-{stamp}"));
    fs::create_dir(&folder)?;
    let pdf = folder.join(config.pdf_path().file_name().unwrap());
    let result = (|| {
        fs::copy(config.pdf_path(), &pdf)?;
        for extension in maps {
            match fs::copy(
                config.pdf_path().with_extension(extension),
                pdf.with_extension(extension),
            ) {
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    })();
    if let Err(e) = result {
        let _ = fs::remove_dir_all(&folder);
        return Err(e);
    }
    // Keep a short history without accumulating every auto-build on disk.
    let mut previews: Vec<_> = fs::read_dir(config.output_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .strip_prefix("preview-")
                .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        })
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir() && !t.is_symlink()))
        .map(|e| e.path())
        .collect();
    previews.sort();
    for old in previews.iter().take(previews.len().saturating_sub(3)) {
        let _ = fs::remove_dir_all(old);
    }
    Ok(pdf)
}
/// Resolve conventional `% !TeX root = main.tex` directives without loops.
pub fn resolve_root(path: &Path) -> io::Result<PathBuf> {
    let mut path = path.canonicalize()?;
    let mut visited = std::collections::HashSet::new();
    loop {
        if !visited.insert(path.clone()) {
            return Err(io::Error::other("Cycle in TeX root directives"));
        }
        let source = fs::read_to_string(&path)?;
        let target = source.lines().take(32).find_map(|line| {
            let line = line.trim().strip_prefix('%')?.trim();
            let (key, value) = line.split_once('=')?;
            (key.trim().eq_ignore_ascii_case("!tex root") && !value.trim().is_empty())
                .then(|| value.trim().trim_matches('"'))
        });
        match target {
            Some(target) => {
                let next = path.parent().unwrap().join(target).canonicalize()?;
                if next == path {
                    return Ok(path);
                }
                path = next;
            }
            None => return Ok(path),
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{os::unix::fs::PermissionsExt, sync::atomic::AtomicUsize};
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static SEQ: AtomicUsize = AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!(
                "folio supervision {} {}",
                std::process::id(),
                SEQ.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("main.tex"), "fixture").unwrap();
            Self(dir)
        }
        fn program(&self, script: &str) -> PathBuf {
            let path = self.0.join("latexmk");
            fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
            path
        }
        fn config(&self) -> BuildConfig {
            let mut config = BuildConfig::new(self.0.join("main.tex"), Engine::PdfLatex);
            config.timeout = Duration::from_secs(2);
            config
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn interrupted_build_terminates_gracefully_and_cleans_auxiliaries_before_retry() {
        let fixture = Fixture::new();
        let config = fixture.config();
        let tool = fixture.program("trap 'printf term > term-observed; exit 0' TERM\nprintf 'broken aux' > .folio/main.aux\nprintf 'keep' > .folio/user-notes.txt\nwhile :; do /bin/sleep 1; done");
        let result =
            compile_with_program(&config, Arc::new(AtomicBool::new(false)), &tool).unwrap();
        assert!(result.cancelled);
        assert!(
            fixture.0.join("term-observed").is_file(),
            "TERM must precede KILL"
        );
        assert!(config.output_dir().join("build-interrupted").is_file());
        let kept = config.output_dir().join("preview-123");
        fs::create_dir(&kept).unwrap();
        fs::write(kept.join("main.pdf"), "last good PDF").unwrap();
        let tool =
            fixture.program("test ! -e .folio/main.aux || exit 12\nprintf PDF > .folio/main.pdf");
        let result =
            compile_with_program(&config, Arc::new(AtomicBool::new(false)), &tool).unwrap();
        assert!(result.success, "{}", result.log);
        assert_eq!(fs::read(kept.join("main.pdf")).unwrap(), b"last good PDF");
        assert!(config.output_dir().join("user-notes.txt").is_file());
        assert!(!config.output_dir().join("build-interrupted").exists());
    }
    #[test]
    fn cancellation_still_works_with_timeout_disabled_and_an_uncooperative_child() {
        let fixture = Fixture::new();
        let mut config = fixture.config();
        config.timeout = Duration::ZERO;
        let tool = fixture.program("trap '' TERM\nwhile :; do /bin/sleep 1; done");
        let cancel = Arc::new(AtomicBool::new(false));
        let signal = cancel.clone();
        let canceller = thread::spawn(move || {
            thread::sleep(Duration::from_millis(150));
            signal.store(true, Ordering::Relaxed);
        });
        let result = compile_with_program(&config, cancel, &tool).unwrap();
        canceller.join().unwrap();
        assert!(result.cancelled && !result.success);
        assert!(result.elapsed >= Duration::from_millis(150));
        assert!(result.elapsed < Duration::from_secs(3));
    }
    #[test]
    fn snapshot_failure_removes_partial_preview_directory() {
        let fixture = Fixture::new();
        let config = fixture.config();
        fs::create_dir_all(config.output_dir()).unwrap();
        fs::write(config.pdf_path(), "PDF").unwrap();
        fs::create_dir(config.pdf_path().with_extension("synctex.gz")).unwrap();
        assert!(snapshot(&config, &["synctex.gz", "synctex"]).is_err());
        assert!(
            !fs::read_dir(config.output_dir())
                .unwrap()
                .flatten()
                .any(|e| e.file_name().to_string_lossy().starts_with("preview-"))
        );
    }
    #[test]
    fn newly_built_pdf_never_inherits_an_unchanged_old_synctex_map() {
        let fixture = Fixture::new();
        let mut config = fixture.config();
        config.timeout = Duration::from_secs(5);
        let tool = fixture.program(
            "printf 'first PDF' > .folio/main.pdf\nprintf old-map > .folio/main.synctex.gz",
        );
        let first = compile_with_program(&config, Arc::new(AtomicBool::new(false)), &tool).unwrap();
        assert!(first.success, "{}", first.log);
        let tool = fixture.program("printf 'second PDF, SyncTeX disabled' > .folio/main.pdf");
        let second =
            compile_with_program(&config, Arc::new(AtomicBool::new(false)), &tool).unwrap();
        assert!(second.success);
        assert!(
            !second.pdf.with_extension("synctex.gz").exists(),
            "an old map must not point into new PDF bytes"
        );
        assert!(first.pdf.with_extension("synctex.gz").exists());
        let tool = fixture.program("true");
        let unchanged =
            compile_with_program(&config, Arc::new(AtomicBool::new(false)), &tool).unwrap();
        assert!(unchanged.success);
        assert!(
            !unchanged.pdf.with_extension("synctex.gz").exists(),
            "a no-op latexmk run must not revive the stale map"
        );
    }
}

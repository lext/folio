//! Structured, portable projects shared by the desktop and command-line tools.
use crate::build::{BuildConfig, Engine};
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};
use toml_edit::DocumentMut;
pub const MANIFEST: &str = "folio.project.toml";
#[derive(Clone, Debug)]
pub struct Manifest {
    pub name: String,
    pub engine: Engine,
    pub timeout: Duration,
}
impl Manifest {
    pub fn parse(text: &str) -> io::Result<Self> {
        let doc = text.parse::<DocumentMut>().map_err(io::Error::other)?;
        for (key, item) in doc.iter() {
            let allowed: &[&str] = match key {
                "schema" => continue,
                "project" => &["name", "main"],
                "build" => &["engine", "timeout_seconds"],
                _ => return Err(invalid(format!("Unknown project field: {key}"))),
            };
            let table = item
                .as_table_like()
                .ok_or_else(|| invalid(format!("{key} must be a table")))?;
            for (field, _) in table.iter() {
                if !allowed.contains(&field) {
                    return Err(invalid(format!("Unknown project field: {key}.{field}")));
                }
            }
        }
        if doc.get("schema").and_then(|v| v.as_integer()) != Some(1) {
            return Err(invalid("Expected project schema = 1"));
        }
        let string = |section: &str, key: &str| {
            doc.get(section)
                .and_then(|t| t.get(key))
                .and_then(|v| v.as_str())
                .ok_or_else(|| invalid(format!("Missing string {section}.{key}")))
        };
        let name = string("project", "name")?.trim().to_owned();
        if name.is_empty() || name.chars().any(char::is_control) {
            return Err(invalid("Project name must be nonempty and on one line"));
        }
        if string("project", "main")? != "main.tex" {
            return Err(invalid(
                "project.main must be main.tex in a structured Folio project",
            ));
        }
        let engine = match string("build", "engine")? {
            "pdflatex" => Engine::PdfLatex,
            "xelatex" => Engine::XeLatex,
            "lualatex" => Engine::LuaLatex,
            _ => {
                return Err(invalid(
                    "build.engine must be pdflatex, xelatex or lualatex",
                ));
            }
        };
        let seconds = doc.get("build").and_then(|t| t.get("timeout_seconds")).and_then(|v| v.as_integer())
            .filter(|v| (0..=86400).contains(v)).ok_or_else(|| invalid("build.timeout_seconds must be an integer from 0 to 86400 (0 disables the limit)"))?;
        Ok(Self {
            name,
            engine,
            timeout: Duration::from_secs(seconds as u64),
        })
    }
}
#[derive(Clone, Debug)]
pub struct Project {
    pub directory: PathBuf,
    pub manifest: Manifest,
}
impl Project {
    pub fn create(directory: &Path, name: &str, engine: Engine) -> io::Result<Self> {
        Self::create_with_timeout(directory, name, engine, Duration::from_secs(120))
    }
    pub fn create_with_timeout(
        directory: &Path,
        name: &str,
        engine: Engine,
        timeout: Duration,
    ) -> io::Result<Self> {
        let quoted = name.replace('\\', "\\\\").replace('"', "\\\"");
        let engine = match engine {
            Engine::PdfLatex => "pdflatex",
            Engine::XeLatex => "xelatex",
            Engine::LuaLatex => "lualatex",
        };
        let seconds = timeout.as_secs();
        let manifest = format!(
            "schema = 1\n\n[project]\nname = \"{quoted}\"\nmain = \"main.tex\"\n\n[build]\nengine = \"{engine}\"\ntimeout_seconds = {seconds}\n"
        );
        Manifest::parse(&manifest)?;
        fs::create_dir(directory)?;
        let created = (|| {
            for subdir in ["sections", "figures", "tables"] {
                fs::create_dir(directory.join(subdir))?;
            }
            for subdir in ["figures", "tables"] {
                fs::write(directory.join(subdir).join(".gitkeep"), "")?;
            }
            fs::write(directory.join(MANIFEST), manifest)?;
            fs::write(
                directory.join("main.tex"),
                "\\documentclass{article}\n\\usepackage[T1]{fontenc}\n\\usepackage{graphicx}\n\\begin{document}\n\\input{sections/introduction}\n\\bibliographystyle{plain}\n\\bibliography{references}\n\\end{document}\n",
            )?;
            fs::write(
                directory.join("sections/introduction.tex"),
                "\\section{Introduction}\nStart writing here.\n",
            )?;
            fs::write(
                directory.join("references.bib"),
                "% Add BibTeX or BibLaTeX entries here.\n",
            )?;
            fs::write(directory.join(".gitignore"), ".folio/\n.DS_Store\n")?;
            fs::write(
                directory.join("README.md"),
                "# LaTeX project\n\nBuild settings are in folio.project.toml.\nWrite sections in sections/, figures in figures/, and table sources in tables/.\nKeep bibliography entries in references.bib or bibliography/.\nOptional styles/ and data/ hold local packages and supporting data.\n.folio/ contains generated output; do not edit it.\n\nBuild: folio-cli build .\nValidate: folio-cli project check .\nExport: folio-cli project export . ../submission\n",
            )?;
            Self::load(directory)
        })();
        if created.is_err() {
            let _ = fs::remove_dir_all(directory);
        }
        created
    }
    pub fn load(path: &Path) -> io::Result<Self> {
        let directory = if path.is_dir() {
            path
        } else if path.file_name().is_some_and(|name| name == MANIFEST) {
            path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."))
        } else {
            return Err(invalid("Open a project folder or folio.project.toml. Individual files are opened from within a project."));
        }
        .canonicalize()?;
        let manifest_path = directory.join(MANIFEST);
        if fs::symlink_metadata(&manifest_path).map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                invalid(format!("No folio.project.toml in {}. Use New project (folio-cli project new DIRECTORY) to create a project.", directory.display()))
            } else { e }
        })?
            .file_type()
            .is_symlink()
        {
            return Err(invalid("Project manifest must not be a symlink"));
        }
        let manifest = Manifest::parse(&fs::read_to_string(&manifest_path)?)
            .map_err(|e| invalid(format!("{}: {e}", manifest_path.display())))?;
        Ok(Self {
            directory,
            manifest,
        })
    }
    pub fn discover(path: &Path) -> io::Result<Option<Self>> {
        let path = path.canonicalize()?;
        let start = if path.is_dir() {
            path.as_path()
        } else {
            path.parent().unwrap()
        };
        for directory in start.ancestors() {
            match fs::symlink_metadata(directory.join(MANIFEST)) {
                Ok(_) => return Self::load(directory).map(Some),
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        Ok(None)
    }
    pub fn files(&self) -> io::Result<Vec<PathBuf>> {
        for directory in ["sections", "figures", "tables"] {
            let meta = fs::symlink_metadata(self.directory.join(directory))
                .map_err(|_| invalid(format!("Missing required {directory}/ folder")))?;
            if meta.file_type().is_symlink() {
                return Err(invalid(format!("{directory}/ must not be a symlink")));
            }
            if !meta.is_dir() {
                return Err(invalid(format!("{directory}/ must be a directory")));
            }
        }
        let mut files = Vec::new();
        self.walk(&self.directory, &mut files, 0)?;
        if !files.iter().any(|p| p == Path::new("main.tex")) {
            return Err(invalid("Missing main.tex"));
        }
        files.sort();
        Ok(files)
    }
    fn walk(&self, directory: &Path, files: &mut Vec<PathBuf>, depth: usize) -> io::Result<()> {
        if depth > 32 {
            return Err(invalid("Project nesting exceeds 32 directories"));
        }
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            let relative = path.strip_prefix(&self.directory).unwrap();
            if depth == 0 && relative == Path::new(".git") {
                continue;
            }
            if entry.file_name() == ".DS_Store" {
                continue;
            }
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                return Err(invalid(format!(
                    "{} is a symlink; projects must contain their own sources and assets",
                    relative.display()
                )));
            }
            if depth == 0 && relative == Path::new(".folio") {
                if !kind.is_dir() {
                    return Err(invalid(".folio must be a directory"));
                }
                continue;
            }
            if kind.is_dir() {
                if depth == 0
                    && ![
                        "sections",
                        "figures",
                        "tables",
                        "styles",
                        "data",
                        "bibliography",
                    ]
                    .iter()
                    .any(|p| relative == Path::new(p))
                {
                    return Err(invalid(format!(
                        "Unexpected folder {}; use sections/, figures/, tables/, styles/, data/ or bibliography/",
                        relative.display()
                    )));
                }
                self.walk(&path, files, depth + 1)?;
            } else if kind.is_file() {
                if !allowed_file(relative) {
                    return Err(invalid(format!(
                        "Misplaced {}; put text in sections/, figures in figures/, tables in tables/, and packages in styles/",
                        relative.display()
                    )));
                }
                if files.len() >= 10000 {
                    return Err(invalid("Project exceeds the 10000-file limit"));
                }
                files.push(relative.to_owned());
            } else {
                return Err(invalid(format!(
                    "{} is not a regular project file",
                    relative.display()
                )));
            }
        }
        Ok(())
    }
    pub fn build_config(&self) -> io::Result<BuildConfig> {
        let current = Self::load(&self.directory)?;
        current.files()?;
        let mut config = BuildConfig::new(self.directory.join("main.tex"), current.manifest.engine);
        config.timeout = current.manifest.timeout;
        config.use_latexmkrc = false;
        Ok(config)
    }
    /// Create a source or asset in its prescribed role, without replacing a file.
    pub fn write_new(&self, relative: &Path, bytes: &[u8]) -> io::Result<PathBuf> {
        use std::io::Write;
        if relative.as_os_str().is_empty()
            || !relative
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_)))
            || !allowed_file(relative)
        {
            return Err(invalid(
                "Use a project-relative path: sections/name.tex, tables/name.tex, figures/image.pdf, or bibliography/refs.bib",
            ));
        }
        self.files()?; // Reject symlinks before creating directories or writing.
        let path = self.directory.join(relative);
        fs::create_dir_all(path.parent().unwrap())?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        if let Err(e) = file.write_all(bytes).and_then(|()| file.sync_all()) {
            drop(file);
            let _ = fs::remove_file(&path);
            return Err(e);
        }
        Ok(path)
    }
}
fn allowed_file(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let first = path
        .components()
        .next()
        .unwrap()
        .as_os_str()
        .to_string_lossy();
    if path.components().count() == 1 {
        return [
            MANIFEST,
            "main.tex",
            "preamble.tex",
            "references.bib",
            ".gitignore",
            "LICENSE",
            "LICENSE.txt",
        ]
        .contains(&first.as_ref())
            || extension == "md";
    }
    if path.file_name().is_some_and(|p| p == ".gitkeep") {
        return [
            "sections",
            "figures",
            "tables",
            "bibliography",
            "styles",
            "data",
        ]
        .contains(&first.as_ref());
    }
    match first.as_ref() {
        "sections" => extension == "tex",
        "figures" => {
            ["pdf", "png", "jpg", "jpeg", "eps", "svg", "tex"].contains(&extension.as_str())
        }
        "tables" => ["tex", "csv", "tsv", "dat"].contains(&extension.as_str()),
        "bibliography" => extension == "bib",
        "styles" => {
            ["sty", "cls", "bst", "clo", "def", "cfg", "tex", "fd"].contains(&extension.as_str())
        }
        "data" => true,
        _ => false,
    }
}
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

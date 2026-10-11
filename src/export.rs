//! Flat, independently buildable submission copies of structured projects.
use crate::{
    build::{self, BuildConfig},
    project::Project,
    syntax::{self, TokenKind},
};
use std::{
    collections::{BTreeMap, HashSet},
    fs, io,
    ops::Range,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bibliography {
    Keep,
    Inline,
}
#[derive(Debug)]
pub struct ExportResult {
    pub directory: PathBuf,
    pub files: usize,
}
pub fn flat(
    project: &Project,
    destination: &Path,
    bibliography: Bibliography,
    cancel: Arc<AtomicBool>,
) -> io::Result<ExportResult> {
    check_cancel(&cancel)?;
    let project = Project::load(&project.directory)?;
    let files = project.files()?;
    if destination.try_exists()? {
        return Err(error(
            "Export destination already exists; choose a new folder",
        ));
    }
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()?;
    if parent.starts_with(&project.directory) {
        return Err(error("Export outside the authoring project"));
    }
    let destination = parent.join(
        destination
            .file_name()
            .ok_or_else(|| error("Missing export folder name"))?,
    );
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let staging = parent.join(format!(
        ".folio-export-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&staging)?;
    let result = (|| {
        let source = staging.join("source");
        let output = staging.join("flat");
        fs::create_dir(&source)?;
        fs::create_dir(&output)?;
        for dir in ["sections", "figures", "tables"] {
            fs::create_dir(source.join(dir))?;
        }
        for file in &files {
            check_cancel(&cancel)?;
            let copy = source.join(file);
            fs::create_dir_all(copy.parent().unwrap())?;
            fs::copy(project.directory.join(file), copy)?;
        }
        // Confirm the snapshot was not assembled across a concurrent disk edit.
        if files != project.files()?
            || files.iter().any(|file| {
                fs::read(project.directory.join(file)).ok() != fs::read(source.join(file)).ok()
            })
        {
            return Err(error("Project changed while preparing export; retry"));
        }
        preflight(&source)?;
        let snapshot = Project::load(&source)?;
        let config = snapshot.build_config()?;
        let built = build::compile(&config, cancel.clone())?;
        if !built.success {
            return Err(error(format!(
                "Original project did not build; export cancelled.\n{}",
                built.log
            )));
        }
        let resources = used_resources(&source, &files)?;
        let names = resource_names(&resources);
        let mut protected = vec![];
        for file in &files {
            if file.extension().is_some_and(|e| e == "tex") {
                let path = source.join(file);
                fs::write(&path, protect(&fs::read_to_string(&path)?, &mut protected))?;
            }
        }
        let mut flat = run_latexpand(&source, config.timeout, &cancel)?;
        for (key, value) in protected {
            flat = flat.replace(&key, &value);
        }
        flat = rewrite_resources(&flat, &names)?;
        if bibliography == Bibliography::Inline {
            flat = inline_bibliography(&flat, &source.join(".folio/main.bbl"))?;
        }
        fs::write(output.join("main.tex"), &flat)?;
        for (path, name) in &names {
            if bibliography == Bibliography::Inline && path.extension().is_some_and(|e| e == "bib")
            {
                continue;
            }
            let from = source.join(path);
            if path.extension().is_some_and(|e| {
                ["sty", "cls", "bst", "clo", "def", "cfg", "fd"]
                    .iter()
                    .any(|x| e == *x)
            }) {
                fs::write(
                    output.join(name),
                    rewrite_resources(&fs::read_to_string(from)?, &names)?,
                )?;
            } else {
                fs::copy(from, output.join(name))?;
            }
        }
        let mut verification = BuildConfig::new(output.join("main.tex"), config.engine);
        verification.timeout = config.timeout;
        verification.use_latexmkrc = false;
        verification.run_bibliography = bibliography == Bibliography::Keep;
        let checked = build::compile(&verification, cancel.clone())?;
        if !checked.success {
            return Err(error(format!(
                "Flat export did not compile; nothing was published.\n{}",
                checked.log
            )));
        }
        // An exported build must not retain paths back into its source snapshot.
        let record = fs::read_to_string(output.join(".folio/main.fls"))?;
        if record.lines().any(|line| {
            line.strip_prefix("INPUT ").is_some_and(|p| {
                output
                    .join(p)
                    .canonicalize()
                    .is_ok_and(|p| p.starts_with(&source))
            })
        }) {
            return Err(error("Flat export still depends on a source-project path"));
        }
        fs::copy(checked.pdf, output.join("main.pdf"))?;
        if bibliography == Bibliography::Keep && output.join(".folio/main.bbl").exists() {
            fs::copy(output.join(".folio/main.bbl"), output.join("main.bbl"))?;
        }
        fs::remove_dir_all(output.join(".folio"))?;
        if bibliography == Bibliography::Inline {
            let _ = fs::remove_file(output.join("main.bbl"));
        }
        let engine = match config.engine {
            build::Engine::PdfLatex => "-pdf",
            build::Engine::XeLatex => "-xelatex",
            build::Engine::LuaLatex => "-lualatex",
        };
        fs::write(
            output.join("BUILD.txt"),
            format!(
                "Build with TeX Live or MiKTeX:\nlatexmk -norc {engine} {}main.tex\n\nVerified by Folio. Sections and tables are expanded in main.tex.\n",
                if bibliography == Bibliography::Inline {
                    "-bibtex- "
                } else {
                    ""
                }
            ),
        )?;
        check_cancel(&cancel)?;
        if destination.try_exists()? {
            return Err(error(
                "Export destination appeared while exporting; choose another folder",
            ));
        }
        let count = fs::read_dir(&output)?.count();
        fs::rename(output, &destination)?;
        Ok(ExportResult {
            directory: destination.clone(),
            files: count,
        })
    })();
    let _ = fs::remove_dir_all(staging);
    result
}

fn resource_names(paths: &[PathBuf]) -> BTreeMap<PathBuf, String> {
    let mut paths = paths.to_vec();
    paths.sort();
    paths.dedup();
    let mut used = HashSet::from([
        "main.tex".to_owned(),
        "main.pdf".to_owned(),
        "main.bbl".to_owned(),
        "build.txt".to_owned(),
    ]);
    paths
        .into_iter()
        .map(|path| {
            let mut name = path.file_name().unwrap().to_string_lossy().to_string();
            if !used.insert(name.to_lowercase()) {
                let stem = path.file_stem().unwrap_or_default().to_string_lossy();
                let extension = path.extension().unwrap_or_default().to_string_lossy();
                let mut n = 2;
                loop {
                    name = format!("{stem}-{n}.{extension}");
                    if used.insert(name.to_lowercase()) {
                        break;
                    }
                    n += 1;
                }
            }
            (path, name)
        })
        .collect()
}

struct Call {
    name: String,
    start: usize,
    end: usize,
    args: Vec<Range<usize>>,
}
fn calls(text: &str) -> io::Result<Vec<Call>> {
    let mut result = vec![];
    for token in syntax::tokenize(text)
        .into_iter()
        .filter(|t| t.kind == TokenKind::Command)
    {
        let name = text[token.range.clone()]
            .trim_start_matches('\\')
            .trim_end_matches('*');
        let count = match name {
            "import" | "subimport" | "inputminted" => 2,
            "input" | "include" | "subfile" | "includeonly" | "includegraphics" | "includepdf"
            | "graphicspath" | "bibliography" | "addbibresource" | "bibliographystyle"
            | "documentclass" | "usepackage" | "RequirePackage" | "lstinputlisting"
            | "verbatiminput" | "pgfplotstableread" => 1,
            _ => continue,
        };
        let mut end = skip_space(text, token.range.end);
        while text[end..].starts_with('[') {
            let (_, next) = group(text, end, '[', ']')?;
            end = skip_space(text, next);
        }
        let mut args = vec![];
        for _ in 0..count {
            end = skip_space(text, end);
            if text[end..].starts_with('{') {
                let (arg, next) = group(text, end, '{', '}')?;
                args.push(arg);
                end = next;
            } else if name == "input" {
                let length = text[end..]
                    .find(|c: char| c.is_whitespace() || "%{}".contains(c))
                    .unwrap_or(text.len() - end);
                args.push(end..end + length);
                end += length;
            } else {
                return Err(error(format!(
                    "Flat export requires a literal braced argument for \\{name}"
                )));
            }
        }
        result.push(Call {
            name: name.into(),
            start: token.range.start,
            end,
            args,
        });
    }
    Ok(result)
}
fn skip_space(text: &str, mut i: usize) -> usize {
    loop {
        while text[i..].starts_with(char::is_whitespace) {
            i += text[i..].chars().next().unwrap().len_utf8();
        }
        if text[i..].starts_with('%') {
            i += text[i..].find('\n').unwrap_or(text.len() - i);
        } else {
            return i;
        }
    }
}
fn group(text: &str, start: usize, open: char, close: char) -> io::Result<(Range<usize>, usize)> {
    let (mut i, mut depth, mut braces) = (start + 1, 1, 0usize);
    while i < text.len() {
        let c = text[i..].chars().next().unwrap();
        let at = i;
        i += c.len_utf8();
        if c == '\\' {
            if let Some(next) = text[i..].chars().next() {
                i += next.len_utf8();
            }
        } else if c == '%' {
            i += text[i..].find('\n').unwrap_or(text.len() - i);
        } else if open == '[' && c == '{' {
            braces += 1;
        } else if open == '[' && c == '}' {
            braces = braces.saturating_sub(1);
        } else if braces == 0 && c == open {
            depth += 1;
        } else if braces == 0 && c == close {
            depth -= 1;
            if depth == 0 {
                return Ok((start + 1..at, i));
            }
        }
    }
    Err(error("Unterminated file argument in flat export"))
}
fn literal(value: &str) -> io::Result<PathBuf> {
    let value = value.trim().trim_matches('"');
    if value.is_empty()
        || value
            .chars()
            .any(|c| "\\#$%{}~".contains(c) || c.is_control())
    {
        return Err(error(format!(
            "Export cannot resolve computed resource path {value:?}; use a literal project-relative path"
        )));
    }
    let mut path = PathBuf::new();
    for c in Path::new(value).components() {
        match c {
            Component::Normal(p) => path.push(p),
            Component::CurDir => {}
            Component::ParentDir if path.pop() => {}
            _ => {
                return Err(error(format!(
                    "Resource path must stay inside the project: {value}"
                )));
            }
        }
    }
    Ok(path)
}
fn rewrite_resources(text: &str, names: &BTreeMap<PathBuf, String>) -> io::Result<String> {
    let calls = calls(text)?;
    let mut graphic_dirs = vec![PathBuf::new()];
    let mut declared_dirs = None;
    let mut edits = vec![];
    for call in calls {
        if call.name == "graphicspath" {
            if !global_declaration(text, call.start)? {
                return Err(error(
                    "Scoped graphicspath is not supported by flat export; use explicit project-relative figure paths",
                ));
            }
            let value = &text[call.args[0].clone()];
            let mut dirs = vec![PathBuf::new()];
            let mut at = skip_space(value, 0);
            while at < value.len() {
                if !value[at..].starts_with('{') {
                    return Err(error("Export requires literal graphicspath directories"));
                }
                let (range, next) = group(value, at, '{', '}')?;
                dirs.push(literal(&value[range])?);
                at = skip_space(value, next);
            }
            if declared_dirs
                .as_ref()
                .is_some_and(|previous| previous != &dirs)
            {
                return Err(error(
                    "Changing graphicspath is not supported by flat export; use explicit project-relative figure paths",
                ));
            }
            declared_dirs = Some(dirs.clone());
            graphic_dirs = dirs;
            edits.push((call.args[0].clone(), "{./}".to_owned()));
            continue;
        }
        if matches!(
            call.name.as_str(),
            "input" | "include" | "import" | "subimport" | "subfile" | "includeonly"
        ) {
            continue;
        }
        let range = call.args.last().unwrap().clone();
        let mut replacements = vec![];
        for value in text[range.clone()].split(',') {
            let path = literal(value)?;
            let package = matches!(
                call.name.as_str(),
                "documentclass" | "usepackage" | "RequirePackage" | "bibliographystyle"
            );
            let extensions: &[&str] = match call.name.as_str() {
                "bibliography" | "addbibresource" => &["bib"],
                "bibliographystyle" => &["bst"],
                "documentclass" => &["cls"],
                "usepackage" | "RequirePackage" => &["sty"],
                "includegraphics" | "includepdf" => &["pdf", "png", "jpg", "jpeg", "eps", "svg"],
                _ => &[],
            };
            let dirs = if matches!(call.name.as_str(), "includegraphics" | "includepdf") {
                graphic_dirs.clone()
            } else {
                vec![PathBuf::new()]
            };
            let mut found = None;
            for dir in dirs {
                let candidate = dir.join(&path);
                if let Some(name) = names.get(&candidate) {
                    found = Some(name.clone());
                    break;
                }
                for extension in extensions {
                    let mut with_extension = candidate.as_os_str().to_owned();
                    with_extension.push(format!(".{extension}"));
                    if let Some(name) = names.get(Path::new(&with_extension)) {
                        found = Some(name.clone());
                        break;
                    }
                }
                if found.is_some() {
                    break;
                }
            }
            match found {
                Some(name) => {
                    replacements.push(if package || call.name == "bibliography" {
                        Path::new(&name)
                            .with_extension("")
                            .to_string_lossy()
                            .to_string()
                    } else {
                        name
                    });
                }
                None if package && path.components().count() == 1 => {
                    replacements.push(value.trim().to_owned())
                }
                None => {
                    return Err(error(format!(
                        "Export could not resolve {} in \\{}",
                        value, call.name
                    )));
                }
            }
        }
        edits.push((range, replacements.join(",")));
    }
    apply_edits(text, edits)
}
fn global_declaration(text: &str, position: usize) -> io::Result<bool> {
    let mut depth = 0usize;
    for token in syntax::tokenize(&text[..position]) {
        match (token.kind, &text[token.range.clone()]) {
            (TokenKind::Delimiter, "{") | (TokenKind::Command, "\\begingroup" | "\\bgroup") => {
                depth += 1
            }
            (TokenKind::Delimiter, "}") | (TokenKind::Command, "\\endgroup" | "\\egroup") => {
                depth = depth.saturating_sub(1)
            }
            (TokenKind::Command, command @ ("\\begin" | "\\end")) => {
                let start = skip_space(text, token.range.end);
                if text[start..].starts_with('{') {
                    let (name, _) = group(text, start, '{', '}')?;
                    if &text[name] != "document" {
                        if command == "\\begin" {
                            depth += 1;
                        } else {
                            depth = depth.saturating_sub(1);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(depth == 0)
}
fn apply_edits(text: &str, mut edits: Vec<(Range<usize>, String)>) -> io::Result<String> {
    edits.sort_by_key(|(range, _)| range.start);
    let (mut result, mut end) = (String::new(), 0);
    for (range, value) in edits {
        if range.start < end {
            continue;
        }
        result.push_str(&text[end..range.start]);
        result.push_str(&value);
        end = range.end;
    }
    result.push_str(&text[end..]);
    Ok(result)
}
fn protect(text: &str, saved: &mut Vec<(String, String)>) -> String {
    let edits = syntax::tokenize(text)
        .into_iter()
        .filter(|t| matches!(t.kind, TokenKind::Comment | TokenKind::Verbatim))
        .map(|token| {
            let key = format!("FOLIOEXPORTPROTECTED{}TOKEN", saved.len());
            saved.push((key.clone(), text[token.range.clone()].to_owned()));
            (token.range, key)
        })
        .collect();
    apply_edits(text, edits).unwrap()
}
fn preflight(directory: &Path) -> io::Result<()> {
    fn visit(root: &Path, path: &Path, import: &Path, stack: &mut Vec<PathBuf>) -> io::Result<()> {
        let path = path.canonicalize()?;
        if !path.starts_with(root) {
            return Err(error("Include leaves the project"));
        }
        if stack.contains(&path) || stack.len() >= 64 {
            return Err(error(format!(
                "Cyclic or excessively nested include: {}",
                path.display()
            )));
        }
        stack.push(path.clone());
        let text = fs::read_to_string(&path)?;
        if text.contains("FOLIOEXPORTPROTECTED") {
            return Err(error("Reserved export marker occurs in source"));
        }
        for call in calls(&text)? {
            if matches!(call.name.as_str(), "includeonly" | "subfile") {
                return Err(error(format!(
                    "Flat export does not support \\{}; use ordinary inputs for the submission",
                    call.name
                )));
            }
            if !matches!(
                call.name.as_str(),
                "input" | "include" | "import" | "subimport"
            ) {
                continue;
            }
            let base = if call.args.len() == 2 {
                import.join(literal(&text[call.args[0].clone()])?)
            } else {
                import.to_owned()
            };
            let file = literal(&text[call.args.last().unwrap().clone()])?;
            let mut next = root.join(&base).join(file);
            if !next.is_file() && next.extension().is_none() {
                next.set_extension("tex");
            }
            visit(root, &next, &base, stack)?;
        }
        stack.pop();
        Ok(())
    }
    let root = directory.canonicalize()?;
    visit(&root, &root.join("main.tex"), Path::new(""), &mut vec![])
}
fn used_resources(source: &Path, files: &[PathBuf]) -> io::Result<Vec<PathBuf>> {
    let record = fs::read_to_string(source.join(".folio/main.fls"))?;
    let mut used = HashSet::new();
    for input in record.lines().filter_map(|s| s.strip_prefix("INPUT ")) {
        if let Ok(path) = source.join(input).canonicalize()
            && let Ok(relative) = path.strip_prefix(source)
        {
            used.insert(relative.to_owned());
        }
    }
    // BibTeX/Biber read their own resources; these are absent from TeX's recorder.
    Ok(files
        .iter()
        .filter(|path| {
            path.extension().is_some_and(|e| e == "bib" || e == "bst")
                || (used.contains(*path) && path.extension().is_none_or(|e| e != "tex"))
        })
        .cloned()
        .collect())
}
fn inline_bibliography(text: &str, bbl: &Path) -> io::Result<String> {
    let calls = calls(text)?;
    if !calls
        .iter()
        .any(|c| matches!(c.name.as_str(), "bibliography" | "addbibresource"))
    {
        return Ok(text.to_owned());
    }
    let compiled = fs::read_to_string(bbl).map_err(|_| error("No compiled bibliography is available to embed; cite an entry and rebuild, or keep BibTeX files"))?;
    let biblatex = calls.iter().any(|c| c.name == "addbibresource");
    let edits = calls
        .into_iter()
        .filter(|c| {
            matches!(
                c.name.as_str(),
                "bibliography" | "bibliographystyle" | "addbibresource"
            )
        })
        .map(|c| {
            (
                c.start..c.end,
                if c.name == "bibliography" && !biblatex {
                    compiled.clone()
                } else {
                    String::new()
                },
            )
        })
        .collect();
    let flat = apply_edits(text, edits)?;
    if biblatex {
        Ok(format!(
            "% Compiled BibLaTeX data; no .bib file or Biber run is needed.\n\\begin{{filecontents*}}[overwrite]{{main.bbl}}\n{compiled}\n\\end{{filecontents*}}\n{flat}"
        ))
    } else {
        Ok(flat)
    }
}
fn run_latexpand(source: &Path, timeout: Duration, cancel: &AtomicBool) -> io::Result<String> {
    let path = source.join(".folio/expanded.tex");
    let log = source.join(".folio/export.log");
    let executable = build::executable("latexpand");
    let mut cmd = Command::new(&executable);
    let mut paths: Vec<_> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    if let Some(parent) = executable.parent() {
        paths.push(parent.to_owned());
    }
    cmd.current_dir(source)
        .env(
            "PATH",
            std::env::join_paths(paths).map_err(io::Error::other)?,
        )
        .args(["--keep-comments", "--empty-comments", "--fatal", "main.tex"])
        .stdin(Stdio::null())
        .stdout(fs::File::create(&path)?)
        .stderr(fs::File::create(&log)?);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn().map_err(|e| {
        error(format!(
            "Cannot run latexpand ({e}). Install the latexpand package from your TeX distribution."
        ))
    })?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            if !status.success() {
                return Err(error(format!(
                    "Flattening failed: {}",
                    String::from_utf8_lossy(&fs::read(log)?)
                )));
            }
            break;
        }
        if cancel.load(Ordering::Relaxed) || (!timeout.is_zero() && start.elapsed() > timeout) {
            build::terminate(&mut child);
            return Err(error("Export cancelled or timed out"));
        }
        thread::sleep(Duration::from_millis(20));
    }
    fs::read_to_string(path)
}
fn check_cancel(cancel: &AtomicBool) -> io::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(error("Export cancelled"))
    } else {
        Ok(())
    }
}
fn error(message: impl Into<String>) -> io::Error {
    io::Error::other(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changing_or_scoped_graphicspath_is_rejected_instead_of_substituting_the_wrong_figure() {
        let names = BTreeMap::from([
            (PathBuf::from("figures/a/chart.pdf"), "chart.pdf".into()),
            (PathBuf::from("figures/b/chart.pdf"), "chart-2.pdf".into()),
        ]);
        for text in [
            "\\graphicspath{{figures/a/}}\\includegraphics{chart}\\graphicspath{{figures/b/}}\\includegraphics{chart}",
            "{\\graphicspath{{figures/a/}}\\includegraphics{chart}}\\includegraphics{chart}",
            "\\begin{figure}\\graphicspath{{figures/a/}}\\includegraphics{chart}\\end{figure}",
        ] {
            assert!(rewrite_resources(text, &names).is_err(), "{text}");
        }
    }
    #[test]
    fn resource_names_are_flat_deterministic_and_case_insensitively_unique() {
        let paths: Vec<PathBuf> = [
            "figures/a/chart.pdf",
            "figures/b/chart.pdf",
            "figures/Chart.pdf",
            "references.bib",
        ]
        .into_iter()
        .map(Into::into)
        .collect();
        let names = resource_names(&paths);
        assert_eq!(names.len(), 4);
        let unique: std::collections::HashSet<_> =
            names.values().map(|s| s.to_lowercase()).collect();
        assert_eq!(unique.len(), 4);
        assert!(names.values().all(|s| !s.contains('/')));
        let mut reversed = paths;
        reversed.reverse();
        assert_eq!(names, resource_names(&reversed));
        assert_eq!(names[Path::new("references.bib")], "references.bib");
    }
    #[test]
    fn export_rewrites_actual_resource_arguments_and_preserves_comments_and_verbatim() {
        let names = BTreeMap::from([
            (PathBuf::from("figures/chart.pdf"), "chart.pdf".into()),
            (PathBuf::from("bibliography/refs.bib"), "refs.bib".into()),
        ]);
        let text = "% \\includegraphics{figures/chart}\n\\includegraphics[width={.5\\textwidth}]{figures/chart}\n\\bibliography{bibliography/refs}\n\\verb|\\includegraphics{figures/chart}|";
        let rewritten = rewrite_resources(text, &names).unwrap();
        assert!(rewritten.contains("\\includegraphics[width={.5\\textwidth}]{chart.pdf}"));
        assert!(rewritten.contains("\\bibliography{refs}"));
        assert!(rewritten.contains("% \\includegraphics{figures/chart}"));
        assert!(rewritten.contains("\\verb|\\includegraphics{figures/chart}|"));
        assert!(rewrite_resources("\\includegraphics{\\myfigure}", &names).is_err());
    }
    #[test]
    fn protected_source_round_trips_without_exposing_fake_inputs_to_latexpand() {
        let original = "% \\input{missing}\n\\begin{verbatim}\n\\input{missing}\n100%\n\\end{verbatim}\n\\input{sections/real}\n";
        let mut saved = vec![];
        let mut masked = protect(original, &mut saved);
        assert!(!masked.contains("missing"));
        assert!(masked.contains("\\input{sections/real}"));
        for (key, value) in saved {
            masked = masked.replace(&key, &value);
        }
        assert_eq!(masked, original);
    }
    #[test]
    fn cycles_dynamic_inputs_and_includeonly_fail_before_running_external_tools() {
        let root =
            std::env::temp_dir().join(format!("folio-export-preflight-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        for input in [
            "\\input{main}",
            "\\input{\\dynamic}",
            "\\includeonly{one}",
            "\\input{../outside}",
        ] {
            std::fs::write(root.join("main.tex"), input).unwrap();
            assert!(preflight(&root).is_err(), "{input}");
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}

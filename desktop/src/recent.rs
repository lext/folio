//! Recently opened projects, most recent first, one folder per line.
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// Enough for the welcome screen and switcher; older entries fall off.
const LIMIT: usize = 12;

/// Beside the user's settings, on macOS and Linux alike.
pub fn file(home: Option<&Path>) -> Option<PathBuf> {
    Some(home?.join(".config/folio/recent-projects"))
}
/// The listed folders that still hold a project. A missing or unreadable
/// history is an empty one.
pub fn load(file: &Path) -> Vec<PathBuf> {
    // Lossy: a history damaged by another program is still mostly readable.
    let text = String::from_utf8_lossy(&fs::read(file).unwrap_or_default()).into_owned();
    let mut projects: Vec<PathBuf> = vec![];
    for directory in text.lines().map(str::trim).map(PathBuf::from) {
        // Relative or repeated lines can only come from a hand edit or a race.
        if directory.is_absolute()
            && !projects.contains(&directory)
            && directory.join(folio_core::project::MANIFEST).is_file()
        {
            projects.push(directory);
        }
    }
    projects.truncate(LIMIT);
    projects
}
/// Moves `project` to the front of the history and returns the new list.
pub fn remember(file: &Path, project: &Path) -> io::Result<Vec<PathBuf>> {
    let mut projects = load(file);
    projects.retain(|p| p != project);
    projects.insert(0, project.to_path_buf());
    projects.truncate(LIMIT);
    if let Some(directory) = file.parent() {
        fs::create_dir_all(directory)?;
    }
    let text: String = projects
        .iter()
        .map(|p| format!("{}\n", p.display()))
        .collect();
    // Replace in one step, so two Folio windows cannot leave half a file.
    let partial = file.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(&partial, text)?;
    fs::rename(&partial, file)?;
    Ok(projects)
}
/// A path as the user knows it: `~/papers/vqa` rather than `/Users/me/…`.
pub fn display(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use folio_core::{build::Engine, project::Project};
    #[test]
    fn history_orders_by_last_use_drops_missing_projects_and_is_capped() {
        let base = std::env::temp_dir().join(format!("folio-recent-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let home = base.join("home");
        let file = file(Some(&home)).unwrap();
        assert!(load(&file).is_empty(), "no history yet");
        let project = |name: &str| {
            Project::create(&base.join(name), name, Engine::PdfLatex)
                .unwrap()
                .directory
        };
        let (a, b) = (project("a"), project("b"));
        remember(&file, &a).unwrap();
        remember(&file, &b).unwrap();
        assert_eq!(remember(&file, &a).unwrap(), [a.clone(), b.clone()]);
        assert_eq!(load(&file), [a.clone(), b.clone()]);
        // A project deleted or moved since is not offered.
        fs::remove_dir_all(&b).unwrap();
        assert_eq!(load(&file), std::slice::from_ref(&a));
        for i in 0..LIMIT + 3 {
            remember(&file, &project(&format!("p{i}"))).unwrap();
        }
        let kept = load(&file);
        assert_eq!(kept.len(), LIMIT);
        assert!(kept[0].ends_with(format!("p{}", LIMIT + 2)));
        assert!(!kept.contains(&a), "the oldest entries fall off");
        assert_eq!(display(&home.join("papers/x"), Some(&home)), "~/papers/x");
        assert_eq!(display(Path::new("/srv/x"), Some(&home)), "/srv/x");
        fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn a_damaged_history_yields_its_usable_lines_and_is_repaired_on_the_next_open() {
        let base = std::env::temp_dir().join(format!("folio-recent-bad-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let a = Project::create(&base.join("a"), "a", Engine::PdfLatex)
            .unwrap()
            .directory;
        let b = Project::create(&base.join("b b"), "b", Engine::PdfLatex)
            .unwrap()
            .directory;
        let file = base.join("recent-projects");
        let mut bytes = format!(
            "\n  {}  \r\nrelative/path\n{}\n/no/such/project\n{}\n",
            a.display(),
            a.display(),
            b.display()
        )
        .into_bytes();
        bytes.extend_from_slice(b"\xff\xfe not text\n");
        fs::write(&file, bytes).unwrap();
        assert_eq!(load(&file), [a.clone(), b.clone()]);
        assert_eq!(remember(&file, &b).unwrap(), [b.clone(), a.clone()]);
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            format!("{}\n{}\n", b.display(), a.display())
        );
        // No temporary file is left beside the history.
        assert_eq!(fs::read_dir(&base).unwrap().count(), 3);
        // A history that cannot be written is an error, not a panic.
        let blocked = base.join("a/main.tex/recent-projects");
        assert!(remember(&blocked, &a).is_err());
        fs::remove_dir_all(base).unwrap();
    }
}

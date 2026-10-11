//! Disk scanning and bibliography parsing run away from the frame thread.
use crate::file_browser::{self, Kind, Tree};
use folio_core::{
    bibliography,
    completion::Index,
    project::{MANIFEST, Project},
};
use std::{
    collections::HashMap,
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::Arc,
};

pub struct Snapshot {
    /// Identifies the disk state and buffers this snapshot was made from.
    pub key: u64,
    pub stamp: u64,
    pub managed: Option<Result<Project, String>>,
    pub files: Vec<PathBuf>,
    pub tree: Tree,
    pub sources: HashMap<PathBuf, String>,
    pub indexes: HashMap<PathBuf, Arc<Index>>,
}

#[cfg(test)]
pub fn scan(root: &Path, buffers: &[(PathBuf, String)]) -> Snapshot {
    scan_if_changed(root, buffers, None).expect("an unconditional scan always reads the project")
}

/// The periodic scan. Listing and stat-ing the project is cheap; reading and
/// indexing every source is not, so that part is skipped while nothing that
/// feeds a snapshot has changed since the one identified by `previous`.
pub fn scan_if_changed(
    root: &Path,
    buffers: &[(PathBuf, String)],
    previous: Option<u64>,
) -> Option<Snapshot> {
    let directory = root.parent().unwrap();
    let manifest = directory.join(MANIFEST);
    let tree = file_browser::scan(directory);
    let fls = directory
        .join(".folio")
        .join(root.file_name().unwrap())
        .with_extension("fls");
    // Sources the last build read, including dependencies outside the folder.
    let mut inputs = vec![];
    if let Ok(record) = fs::read_to_string(&fls) {
        for line in record.lines() {
            if let Some(input) = line.strip_prefix("INPUT ") {
                let path = directory.join(input);
                if path.extension().is_some_and(|e| {
                    ["tex", "bib", "png", "jpg", "pdf", "eps"]
                        .iter()
                        .any(|x| e == *x)
                }) && let Ok(path) = path.canonicalize()
                    && !path.starts_with(directory.join(".folio"))
                {
                    inputs.push(path);
                }
            }
        }
    }
    inputs.sort();
    inputs.dedup();
    let mut key = std::collections::hash_map::DefaultHasher::new();
    tree.incomplete.hash(&mut key);
    for path in tree
        .entries
        .iter()
        .map(|entry| &entry.path)
        .chain([&manifest, &fls])
        .chain(&inputs)
    {
        path.hash(&mut key);
        if let Ok(meta) = fs::symlink_metadata(path) {
            meta.is_dir().hash(&mut key);
            meta.len().hash(&mut key);
            meta.modified().ok().hash(&mut key);
        }
    }
    buffers.hash(&mut key);
    let key = key.finish();
    if previous == Some(key) {
        return None;
    }
    let managed = manifest
        .exists()
        .then(|| Project::load(directory).map_err(|e| e.to_string()));
    let mut paths = vec![];
    if let Some(Ok(project)) = &managed
        && let Ok(files) = project.files()
    {
        paths.extend(files.into_iter().map(|p| directory.join(p)));
    } else {
        paths.extend(
            tree.entries
                .iter()
                .filter(|e| e.kind != Kind::Directory)
                .map(|e| e.path.clone()),
        );
    }
    if managed.is_some() {
        paths.push(manifest);
    }
    paths.extend(inputs);
    paths.push(root.to_owned());
    paths.sort();
    paths.dedup();
    let mut sources = HashMap::new();
    for path in &paths {
        if file_browser::is_source(path)
            && let Ok(text) = fs::read_to_string(path)
        {
            sources.insert(path.clone(), text);
        }
    }
    let mut resources = vec![];
    for (path, text) in &sources {
        if path.extension().is_some_and(|e| e == "tex") {
            let text = buffers
                .iter()
                .find(|(p, _)| p == path)
                .map_or(text, |(_, text)| text);
            for resource in bibliography::resources(text) {
                let path = directory.join(resource);
                resources.push(path.canonicalize().unwrap_or(path));
            }
        }
    }
    for path in &resources {
        if let Ok(text) = fs::read_to_string(path) {
            sources.insert(path.clone(), text);
        }
    }
    paths.extend(resources);
    paths.push(root.to_owned());
    paths.sort();
    paths.dedup();
    let mut stamp = std::collections::hash_map::DefaultHasher::new();
    for path in &paths {
        path.hash(&mut stamp);
        if let Ok(meta) = fs::metadata(path) {
            meta.len().hash(&mut stamp);
            meta.modified().ok().hash(&mut stamp);
        }
    }
    let indexes = sources
        .iter()
        .filter(|(path, _)| crate::language::Kind::for_path(path) == crate::language::Kind::Latex)
        .map(|(path, text)| {
            let index = if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("bib"))
            {
                Index::bibliography(&bibliography::parse(text))
            } else {
                Index::source(text)
            };
            (path.clone(), Arc::new(index))
        })
        .collect();
    Some(Snapshot {
        key,
        managed,
        tree,
        stamp: stamp.finish(),
        files: paths
            .into_iter()
            .filter(|p| file_browser::is_source(p))
            .collect(),
        sources,
        indexes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn periodic_scan_is_skipped_until_disk_or_buffers_change() {
        let dir = std::env::temp_dir().join(format!("folio-scan-key-{}", std::process::id()));
        fs::create_dir_all(dir.join("project/sections")).unwrap();
        fs::create_dir_all(dir.join("project/.folio")).unwrap();
        fs::write(dir.join("outside.tex"), "External dependency").unwrap();
        fs::write(dir.join("project/main.tex"), "Document").unwrap();
        fs::write(dir.join("project/sections/a.tex"), "Section").unwrap();
        fs::write(
            dir.join("project/.folio/main.fls"),
            "INPUT ../outside.tex\n",
        )
        .unwrap();
        let root = dir.join("project/main.tex").canonicalize().unwrap();
        let section = root.parent().unwrap().join("sections/a.tex");
        let mut key = scan(&root, &[]).key;
        assert!(scan_if_changed(&root, &[], Some(key)).is_none());
        let mut changed = |what: &str, buffers: &[(PathBuf, String)]| {
            let snapshot = scan_if_changed(&root, buffers, Some(key))
                .unwrap_or_else(|| panic!("{what} must trigger a scan"));
            key = snapshot.key;
            assert!(
                scan_if_changed(&root, buffers, Some(key)).is_none(),
                "{what}: an identical state must not be scanned again"
            );
            snapshot
        };
        fs::write(&section, "Section, edited externally").unwrap();
        assert!(changed("an edited file", &[]).sources[&section].contains("externally"));
        fs::write(root.parent().unwrap().join("sections/b.tex"), "New").unwrap();
        changed("a new file", &[]);
        fs::remove_file(root.parent().unwrap().join("sections/b.tex")).unwrap();
        changed("a deleted file", &[]);
        fs::write(dir.join("outside.tex"), "External dependency, edited").unwrap();
        changed("a dependency outside the folder", &[]);
        let buffer = [(section.clone(), "Unsaved text".to_owned())];
        changed("an unsaved buffer", &buffer);
        changed("closing the buffer", &[]);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn browser_inventory_is_local_and_tracks_external_file_changes() {
        let dir = std::env::temp_dir().join(format!("folio-browser-scan-{}", std::process::id()));
        fs::create_dir_all(dir.join("project/.folio")).unwrap();
        fs::write(dir.join("outside.tex"), "External dependency").unwrap();
        fs::write(dir.join("project/main.tex"), "Document").unwrap();
        fs::write(
            dir.join("project/.folio/main.fls"),
            "INPUT ../outside.tex\n",
        )
        .unwrap();
        let root = dir.join("project/main.tex").canonicalize().unwrap();
        let first = scan(&root, &[]);
        assert!(
            first
                .files
                .contains(&dir.join("outside.tex").canonicalize().unwrap())
        );
        assert_eq!(
            first.tree.entries.len(),
            1,
            "dependencies outside the project are not browser entries"
        );
        fs::create_dir_all(dir.join("project/tables")).unwrap();
        fs::write(dir.join("project/tables/results.tex"), "Results").unwrap();
        let second = scan(&root, &[]);
        assert_eq!(second.tree.entries.len(), 3);
        assert!(
            second
                .files
                .contains(&root.parent().unwrap().join("tables/results.tex"))
        );
        fs::remove_file(dir.join("project/tables/results.tex")).unwrap();
        let third = scan(&root, &[]);
        assert_eq!(
            third.tree.entries.len(),
            2,
            "empty folder stays visible after a file is deleted"
        );
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn directory_walk_enforces_the_limit_inside_a_wide_directory() {
        let dir = std::env::temp_dir().join(format!("folio walk limit {}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("one.tex"), "").unwrap();
        fs::write(dir.join("two.tex"), "").unwrap();
        let mut tree = file_browser::scan(&dir);
        tree.entries = vec![tree.entries[0].clone(); 3999];
        file_browser::walk(&dir, 0, &mut tree, 4000);
        assert_eq!(tree.entries.len(), 4000);
        assert!(tree.incomplete);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn included_bibliography_paths_use_the_build_working_directory() {
        let dir =
            std::env::temp_dir().join(format!("folio bibliography paths {}", std::process::id()));
        fs::create_dir_all(dir.join("project/chapters")).unwrap();
        let root = dir.join("project/main.tex");
        fs::write(&root, "\\input{chapters/one}").unwrap();
        fs::write(
            dir.join("project/chapters/one.tex"),
            "\\addbibresource{../shared.bib}",
        )
        .unwrap();
        let bib = dir.join("shared.bib");
        fs::write(&bib, "@book{shared,title={Shared},}").unwrap();
        let snapshot = scan(&root, &[]);
        assert!(snapshot.indexes.contains_key(&bib.canonicalize().unwrap()));
        fs::remove_dir_all(dir).unwrap();
    }
}

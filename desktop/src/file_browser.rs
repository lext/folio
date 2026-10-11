//! Project inventory is built on the scan worker; frames only paint cached rows.
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Directory,
    Text,
    Asset,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub depth: usize,
    pub kind: Kind,
}

#[derive(Default, PartialEq, Eq)]
pub struct Tree {
    pub entries: Vec<Entry>,
    pub incomplete: bool,
}

pub fn scan(directory: &Path) -> Tree {
    let mut tree = Tree::default();
    walk(directory, 0, &mut tree, 4000);
    tree
}

pub(crate) fn walk(directory: &Path, depth: usize, tree: &mut Tree, limit: usize) {
    if depth > 12 {
        tree.incomplete = true;
        return;
    }
    let Ok(items) = fs::read_dir(directory) else {
        tree.incomplete = true;
        return;
    };
    let mut entries = Vec::new();
    for item in items {
        let Ok(item) = item else {
            tree.incomplete = true;
            continue;
        };
        let Ok(kind) = item.file_type() else {
            tree.incomplete = true;
            continue;
        };
        let name = item.file_name().to_string_lossy().into_owned();
        if kind.is_symlink()
            || (!kind.is_dir() && !kind.is_file())
            || (name.starts_with('.') && name != ".gitignore")
            || (kind.is_dir() && ["target", "dist", "node_modules"].contains(&name.as_str()))
        {
            continue;
        }
        // Bound directory enumeration as well as the eventual flattened tree.
        if entries.len() >= limit.saturating_sub(tree.entries.len()) {
            tree.incomplete = true;
            break;
        }
        let path = item.path();
        entries.push(Entry {
            kind: if kind.is_dir() {
                Kind::Directory
            } else if is_asset(&path) {
                Kind::Asset
            } else {
                Kind::Text
            },
            path,
            name,
            depth,
        });
    }
    entries.sort_by_cached_key(|entry| {
        (
            entry.kind != Kind::Directory,
            entry.name.to_lowercase(),
            entry.name.clone(),
        )
    });
    for entry in entries {
        if tree.entries.len() >= limit {
            tree.incomplete = true;
            break;
        }
        let directory = (entry.kind == Kind::Directory).then(|| entry.path.clone());
        tree.entries.push(entry);
        if let Some(directory) = directory {
            walk(&directory, depth + 1, tree, limit);
        }
    }
}

fn is_asset(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        [
            "pdf", "png", "jpg", "jpeg", "gif", "webp", "tif", "tiff", "eps", "svg",
        ]
        .contains(&extension.to_string_lossy().to_ascii_lowercase().as_str())
    })
}

/// File types worth reading/indexing in the background. Other files remain
/// visible and can be explicitly opened, without preloading arbitrary data.
pub fn is_source(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        [
            "tex", "bib", "sty", "cls", "bst", "clo", "def", "cfg", "fd", "toml", "json", "csv",
            "tsv", "dat", "md", "txt",
        ]
        .contains(&extension.to_string_lossy().to_ascii_lowercase().as_str())
    })
}

#[derive(Default)]
pub struct Browser {
    pub open: bool,
    directory: Option<PathBuf>,
    pub tree: Tree,
    collapsed: HashSet<PathBuf>,
    pub visible: Vec<usize>,
    pub scroll_to: Option<PathBuf>,
    /// Rows painted last frame, so revealing an already visible file does not scroll.
    pub shown_rows: std::ops::Range<usize>,
}

impl Browser {
    pub fn replace(&mut self, directory: PathBuf, tree: Tree) {
        if self.directory.as_ref() != Some(&directory) {
            let pending = self
                .scroll_to
                .take()
                .filter(|path| path.starts_with(&directory));
            self.clear();
            self.directory = Some(directory);
            self.scroll_to = pending;
        }
        if self.tree == tree {
            return;
        }
        self.collapsed.retain(|path| {
            tree.entries
                .iter()
                .any(|e| &e.path == path && e.kind == Kind::Directory)
        });
        self.tree = tree;
        self.rebuild_rows();
    }
    pub fn clear(&mut self) {
        self.directory = None;
        self.tree = Tree::default();
        self.visible.clear();
        self.collapsed.clear();
        self.scroll_to = None;
    }
    pub fn expanded(&self, path: &Path) -> bool {
        !self.collapsed.contains(path)
    }
    pub fn toggle_folder(&mut self, path: &Path) {
        if !self.collapsed.remove(path) {
            self.collapsed.insert(path.to_owned());
        }
        self.rebuild_rows();
    }
    pub fn reveal(&mut self, path: &Path) {
        let mut changed = false;
        for parent in path.ancestors().skip(1) {
            changed |= self.collapsed.remove(parent);
        }
        if changed {
            self.rebuild_rows();
        }
        self.scroll_to = Some(path.to_owned());
    }
    fn rebuild_rows(&mut self) {
        self.visible.clear();
        let mut hidden_below = None;
        for (index, entry) in self.tree.entries.iter().enumerate() {
            if hidden_below.is_some_and(|depth| entry.depth > depth) {
                continue;
            }
            hidden_below = None;
            self.visible.push(index);
            if entry.kind == Kind::Directory && self.collapsed.contains(&entry.path) {
                hidden_below = Some(entry.depth);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn inventory_contains_empty_folders_assets_and_text_but_not_internal_files() {
        let dir =
            std::env::temp_dir().join(format!("folio-browser-inventory-{}", std::process::id()));
        for folder in [
            "sections/nested",
            "figures",
            "tables",
            ".folio",
            ".git",
            "target",
        ] {
            fs::create_dir_all(dir.join(folder)).unwrap();
        }
        for file in [
            "main.tex",
            "folio.project.toml",
            ".gitignore",
            "sections/nested/a.tex",
            "figures/plot.PDF",
            "tables/.gitkeep",
            ".folio/main.tex",
            ".git/config",
            "target/no.tex",
        ] {
            fs::write(dir.join(file), "").unwrap();
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(std::env::temp_dir(), dir.join("outside")).unwrap();
        let tree = scan(&dir);
        let names: Vec<_> = tree
            .entries
            .iter()
            .map(|e| {
                e.path
                    .strip_prefix(&dir)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(
            names,
            [
                "figures",
                "figures/plot.PDF",
                "sections",
                "sections/nested",
                "sections/nested/a.tex",
                "tables",
                ".gitignore",
                "folio.project.toml",
                "main.tex"
            ]
        );
        assert!(!tree.incomplete);
        assert_eq!(tree.entries[1].kind, Kind::Asset);
        assert_eq!(tree.entries[4].depth, 2);
        assert_eq!(tree.entries[8].kind, Kind::Text);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn expansion_survives_refresh_reveals_active_files_and_resets_for_new_projects() {
        let dir =
            std::env::temp_dir().join(format!("folio-browser-expansion-{}", std::process::id()));
        fs::create_dir_all(dir.join("sections/nested")).unwrap();
        let file = dir.join("sections/nested/chapter.tex");
        fs::write(&file, "chapter").unwrap();
        let mut browser = Browser::default();
        browser.replace(dir.clone(), scan(&dir));
        browser.toggle_folder(&dir.join("sections"));
        assert_eq!(browser.visible.len(), 1);
        fs::write(dir.join("sections/added.tex"), "new").unwrap();
        browser.replace(dir.clone(), scan(&dir));
        assert_eq!(
            browser.visible.len(),
            1,
            "refresh must not reopen collapsed folders"
        );
        browser.reveal(&file);
        assert_eq!(browser.visible.len(), 4);
        browser.toggle_folder(&dir.join("sections"));
        browser.replace(dir.join("different"), Tree::default());
        assert!(browser.visible.is_empty());
        assert!(browser.collapsed.is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn wide_inventory_is_bounded_and_reports_truncation() {
        let dir = std::env::temp_dir().join(format!("folio-browser-limit-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("one.tex"), "").unwrap();
        fs::write(dir.join("two.tex"), "").unwrap();
        let mut tree = Tree::default();
        walk(&dir, 0, &mut tree, 1);
        assert_eq!(tree.entries.len(), 1);
        assert!(tree.incomplete);
        fs::remove_dir_all(dir).unwrap();
    }
}

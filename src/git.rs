//! Read-only Git comparisons, including unsaved editor buffers.
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
}
#[derive(Clone, Debug)]
pub struct Hunk {
    pub old_start: usize,
    pub old_count: usize,
    pub new_start: usize,
    pub new_count: usize,
    pub before: Vec<String>,
    pub after: Vec<String>,
}
impl Hunk {
    pub fn kind(&self) -> ChangeKind {
        if self.old_count == 0 {
            ChangeKind::Added
        } else if self.new_count == 0 {
            ChangeKind::Deleted
        } else {
            ChangeKind::Modified
        }
    }
    pub fn anchor_line(&self) -> usize {
        self.new_start.max(1)
    }
    pub fn contains_line(&self, line: usize) -> bool {
        line >= self.anchor_line() && line < self.anchor_line() + self.new_count.max(1)
    }
}
#[derive(Clone, Debug)]
pub struct Commit {
    pub oid: String,
    pub subject: String,
}
#[derive(Clone, Debug)]
pub struct Comparison {
    pub root: PathBuf,
    pub commit: Option<String>,
    pub hunks: Vec<Hunk>,
}
/// A cheap fingerprint of the repository state a comparison depends on, read
/// with a few `stat` calls and no Git process. It changes when HEAD moves
/// (commit, checkout, pull, reset); `None` means `path` is not in a work tree.
pub fn state(path: &Path) -> Option<u64> {
    use std::hash::{Hash, Hasher};
    let dot_git = path
        .ancestors()
        .skip(1)
        .map(|d| d.join(".git"))
        .find(|p| p.exists())?;
    // A linked work tree or submodule has a `.git` file naming its real directory.
    let directory = if dot_git.is_file() {
        let text = fs::read_to_string(&dot_git).ok()?;
        dot_git
            .parent()?
            .join(text.lines().next()?.strip_prefix("gitdir:")?.trim())
    } else {
        dot_git
    };
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    directory.hash(&mut hasher);
    for name in ["HEAD", "logs/HEAD", "packed-refs"] {
        if let Ok(meta) = fs::metadata(directory.join(name)) {
            name.hash(&mut hasher);
            meta.len().hash(&mut hasher);
            meta.modified().ok().hash(&mut hasher);
        }
    }
    Some(hasher.finish())
}
fn command(dir: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(dir)
        .args(["-c", "core.fsmonitor=false"])
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_PAGER", "cat")
        .env("LC_ALL", "C");
    cmd
}
fn output(cmd: &mut Command) -> Result<Output, String> {
    cmd.output().map_err(|e| format!("Cannot run Git: {e}"))
}
fn checked(cmd: &mut Command) -> Result<Vec<u8>, String> {
    let result = output(cmd)?;
    if result.status.success() {
        Ok(result.stdout)
    } else {
        Err(String::from_utf8_lossy(&result.stderr).trim().into())
    }
}
fn repository(path: &Path) -> Result<(PathBuf, PathBuf), String> {
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    let root = checked(
        command(path.parent().ok_or("File has no directory")?)
            .args(["rev-parse", "--show-toplevel"]),
    )?;
    let root = PathBuf::from(
        String::from_utf8(root)
            .map_err(|e| e.to_string())?
            .trim_end(),
    );
    let relative = path
        .strip_prefix(&root)
        .map_err(|e| e.to_string())?
        .to_path_buf();
    Ok((root, relative))
}
pub fn history(path: &Path) -> Result<Vec<Commit>, String> {
    let (root, _) = repository(path)?;
    let bytes = checked(command(&root).args(["log", "-30", "--format=%H%x00%s"]))?;
    Ok(String::from_utf8_lossy(&bytes)
        .lines()
        .filter_map(|line| {
            line.split_once('\0').map(|(oid, subject)| Commit {
                oid: oid.into(),
                subject: subject.into(),
            })
        })
        .collect())
}
pub fn compare(path: &Path, revision: &str, text: &str) -> Result<Comparison, String> {
    let (root, relative) = repository(path)?;
    let resolved = output(command(&root).args([
        "rev-parse",
        "--verify",
        "--end-of-options",
        &format!("{revision}^{{commit}}"),
    ]))?;
    let commit = if resolved.status.success() {
        Some(String::from_utf8_lossy(&resolved.stdout).trim().to_string())
    } else if revision == "HEAD"
        && checked(command(&root).args(["rev-list", "--all", "--max-count=1"]))?.is_empty()
    {
        None
    } else {
        return Err(format!("Cannot resolve Git commit: {revision}"));
    };
    let mut before = vec![];
    if let Some(oid) = &commit {
        let entry = checked(
            command(&root)
                .args(["ls-tree", "-z", oid, "--"])
                .arg(&relative),
        )?;
        if !entry.is_empty() {
            // Object ID is resolved above; this cannot be interpreted as an option.
            before = checked(
                command(&root).args(["show", &format!("{oid}:{}", relative.to_string_lossy())]),
            )?;
        }
    }
    let hunks = diff_buffers(&before, text.as_bytes())?;
    Ok(Comparison {
        root,
        commit,
        hunks,
    })
}
static TEMP_ID: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn diff_buffers(before: &[u8], after: &[u8]) -> Result<Vec<Hunk>, String> {
    if before == after {
        return Ok(vec![]);
    }
    let dir = std::env::temp_dir().join(format!(
        "folio-diff-{}-{}",
        std::process::id(),
        TEMP_ID.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&dir).map_err(|e| e.to_string())?;
    let temp = Temp(dir);
    let old = temp.0.join("before");
    let new = temp.0.join("after");
    fs::write(&old, before)
        .and_then(|_| fs::write(&new, after))
        .map_err(|e| e.to_string())?;
    let result = output(
        command(&temp.0)
            .args([
                "diff",
                "--no-index",
                "--no-ext-diff",
                "--no-textconv",
                "--no-color",
                "--text",
                "--ignore-cr-at-eol",
                "--unified=0",
                "--",
            ])
            .arg(old)
            .arg(new),
    )?;
    if !matches!(result.status.code(), Some(0 | 1)) {
        return Err(String::from_utf8_lossy(&result.stderr).into());
    }
    parse_hunks(&String::from_utf8_lossy(&result.stdout))
}
pub fn parse_hunks(diff: &str) -> Result<Vec<Hunk>, String> {
    let mut result: Vec<Hunk> = vec![];
    for line in diff.lines() {
        if line.starts_with("@@") {
            let mut fields = line.split_whitespace();
            fields.next();
            let range = |s: Option<&str>, prefix: char| -> Result<(usize, usize), String> {
                let s = s
                    .and_then(|s| s.strip_prefix(prefix))
                    .ok_or("Malformed Git hunk")?;
                let (start, count) = s.split_once(',').unwrap_or((s, "1"));
                Ok((
                    start.parse().map_err(|_| "Malformed Git line")?,
                    count.parse().map_err(|_| "Malformed Git count")?,
                ))
            };
            let (old_start, old_count) = range(fields.next(), '-')?;
            let (new_start, new_count) = range(fields.next(), '+')?;
            if fields.next() != Some("@@") {
                return Err("Malformed Git hunk".into());
            }
            result.push(Hunk {
                old_start,
                old_count,
                new_start,
                new_count,
                before: vec![],
                after: vec![],
            });
        } else if let Some(hunk) = result.last_mut() {
            if let Some(s) = line.strip_prefix('-') {
                hunk.before.push(s.into());
            } else if let Some(s) = line.strip_prefix('+') {
                hunk.after.push(s.into());
            }
        }
    }
    Ok(result)
}

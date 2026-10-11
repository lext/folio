use folio_core::git::{self, ChangeKind};
use std::{fs, path::Path, process::Command};
fn git_cmd(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}
#[test]
fn hunks_cover_add_modify_delete_and_missing_newlines() {
    let diff = "--- old\n+++ new\n@@ -0,0 +1,2 @@\n+alpha\n+βeta\n@@ -3,2 +5 @@\n-before\n-second\n+after\n@@ -8 +9,0 @@\n-gone\n\\ No newline at end of file\n";
    let hunks = git::parse_hunks(diff).unwrap();
    assert_eq!(hunks.len(), 3);
    assert_eq!(hunks[0].kind(), ChangeKind::Added);
    assert_eq!(hunks[0].new_start, 1);
    assert_eq!(hunks[0].new_count, 2);
    assert_eq!(hunks[1].kind(), ChangeKind::Modified);
    assert_eq!(hunks[1].before, ["before", "second"]);
    assert_eq!(hunks[1].after, ["after"]);
    assert_eq!(hunks[2].kind(), ChangeKind::Deleted);
    assert_eq!(hunks[2].anchor_line(), 9);
    assert!(git::parse_hunks("@@ broken @@").is_err());
}
#[test]
fn actual_git_compares_unsaved_text_to_selected_commits_without_writing_repo() {
    let dir = std::env::temp_dir().join(format!("folio git changes {}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    git_cmd(&dir, &["init", "-q"]);
    git_cmd(&dir, &["config", "user.email", "test@example.invalid"]);
    git_cmd(&dir, &["config", "user.name", "Folio test"]);
    let path = dir.join("chapter with spaces.tex");
    fs::write(&path, "first\noriginal\nlast\n").unwrap();
    git_cmd(&dir, &["add", "--", "chapter with spaces.tex"]);
    git_cmd(&dir, &["commit", "-qm", "First version"]);
    let first = git_cmd(&dir, &["rev-parse", "HEAD"]);
    fs::write(&path, "first\ncommitted\nlast\n").unwrap();
    git_cmd(&dir, &["commit", "-qam", "Second version"]);
    let index_before = fs::read(dir.join(".git/index")).unwrap();
    let head_before = git_cmd(&dir, &["rev-parse", "HEAD"]);
    let crlf = git::compare(&path, "HEAD", "first\r\ncommitted\r\nlast\r\n").unwrap();
    assert!(
        crlf.hunks.is_empty(),
        "line ending conversion is not a source edit"
    );
    let crlf_edit = git::compare(&path, "HEAD", "first\r\nchanged\r\nlast\r\n").unwrap();
    assert_eq!(crlf_edit.hunks.len(), 1);
    assert_eq!(crlf_edit.hunks[0].new_count, 1);
    let comparison = git::compare(&path, "HEAD", "first\nunsaved α\nlast\nextra\n").unwrap();
    assert_eq!(comparison.hunks.len(), 2);
    assert_eq!(comparison.hunks[0].before, ["committed"]);
    assert_eq!(comparison.hunks[0].after, ["unsaved α"]);
    assert_eq!(comparison.hunks[1].kind(), ChangeKind::Added);
    let old = git::compare(&path, &first, "first\nunsaved α\nlast\n").unwrap();
    assert_eq!(old.hunks[0].before, ["original"]);
    let commits = git::history(&path).unwrap();
    assert_eq!(commits[0].subject, "Second version");
    assert_eq!(commits[1].oid, first);
    assert!(git::compare(&path, "--help", "x").is_err());
    assert_eq!(fs::read(dir.join(".git/index")).unwrap(), index_before);
    assert_eq!(git_cmd(&dir, &["rev-parse", "HEAD"]), head_before);
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "first\ncommitted\nlast\n"
    );
    let untracked = dir.join("new.tex");
    fs::write(&untracked, "new\n").unwrap();
    assert_eq!(
        git::compare(&untracked, "HEAD", "new\n").unwrap().hunks[0].kind(),
        ChangeKind::Added
    );
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn outside_repo_and_unborn_head_are_explicit_states() {
    let dir = std::env::temp_dir().join(format!("folio unborn {}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("new.tex");
    fs::write(&path, "new\n").unwrap();
    assert!(git::compare(&path, "HEAD", "new\n").is_err());
    git_cmd(&dir, &["init", "-q"]);
    let result = git::compare(&path, "HEAD", "new\n").unwrap();
    assert!(result.commit.is_none());
    assert_eq!(result.hunks[0].kind(), ChangeKind::Added);
    fs::remove_dir_all(dir).unwrap();
}

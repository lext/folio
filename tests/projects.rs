use folio_core::{
    build::Engine,
    project::{MANIFEST, Manifest, Project},
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "folio-project-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        )))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn opening_requires_a_project_directory_or_manifest() {
    let f = Fixture::new();
    let project = Project::create(&f.0, "Paper", Engine::PdfLatex).unwrap();
    assert_eq!(Project::load(&f.0).unwrap().directory, project.directory);
    assert_eq!(
        Project::load(&f.0.join(MANIFEST)).unwrap().directory,
        project.directory
    );
    let error = Project::load(&f.0.join("main.tex"))
        .unwrap_err()
        .to_string();
    assert!(error.contains("project folder"), "{error}");
    assert!(Project::load(&f.0.join("sections")).is_err());
}

#[test]
fn new_project_records_configured_build_defaults_in_its_manifest() {
    let f = Fixture::new();
    let project = Project::create_with_timeout(
        &f.0,
        "Paper",
        Engine::XeLatex,
        std::time::Duration::from_secs(900),
    )
    .unwrap();
    let config = project.build_config().unwrap();
    assert_eq!(config.engine, Engine::XeLatex);
    assert_eq!(config.timeout.as_secs(), 900);
}

#[test]
fn creates_a_strict_portable_project_and_never_overwrites_an_existing_directory() {
    let f = Fixture::new();
    let project = Project::create(&f.0, "Élodie's \"paper\"", Engine::XeLatex).unwrap();
    for name in [
        MANIFEST,
        "main.tex",
        "supplement.tex",
        "references.bib",
        "sections/introduction.tex",
    ] {
        assert!(f.0.join(name).is_file(), "{name}");
    }
    for name in ["sections", "figures", "tables"] {
        assert!(f.0.join(name).is_dir());
    }
    for name in ["figures/.gitkeep", "tables/.gitkeep"] {
        assert!(
            f.0.join(name).is_file(),
            "required empty folders must survive Git checkout"
        );
    }
    assert_eq!(project.manifest.name, "Élodie's \"paper\"");
    assert_eq!(project.manifest.engine, Engine::XeLatex);
    let before = fs::read(f.0.join("main.tex")).unwrap();
    assert!(Project::create(&f.0, "Replacement", Engine::PdfLatex).is_err());
    assert_eq!(fs::read(f.0.join("main.tex")).unwrap(), before);
    let found = Project::discover(&f.0.join("sections/introduction.tex"))
        .unwrap()
        .unwrap();
    assert_eq!(found.directory, f.0.canonicalize().unwrap());
    let config = found.build_config().unwrap();
    assert_eq!(config.root, found.directory.join("main.tex"));
    assert_eq!(config.engine, Engine::XeLatex);
}

#[test]
fn manifest_rejects_typos_unknown_versions_and_invalid_build_values() {
    let good = "schema = 1\n[project]\nname = 'Paper'\nmain = 'main.tex'\n[build]\nengine = 'lualatex'\ntimeout_seconds = 600\n";
    let parsed = Manifest::parse(good).unwrap();
    assert_eq!(parsed.timeout.as_secs(), 600);
    assert_eq!(parsed.engine, Engine::LuaLatex);
    for bad in [
        good.replace("schema = 1", "schema = 99"),
        good.replace("600", "-1"),
        good.replace("engine", "engien"),
        good.replace("main.tex", "../outside.tex"),
        good.replace("lualatex", "sh"),
        format!("{good}command = 'rm -rf /'\n"),
    ] {
        assert!(Manifest::parse(&bad).is_err(), "{bad}");
    }
}

#[test]
fn supplement_is_an_optional_standalone_root_document() {
    let f = Fixture::new();
    let p = Project::create(&f.0, "Paper", Engine::PdfLatex).unwrap();
    let supplement = fs::read_to_string(f.0.join("supplement.tex")).unwrap();
    assert!(supplement.contains("\\documentclass") && supplement.contains("\\end{document}"));
    assert!(
        !fs::read_to_string(f.0.join("main.tex"))
            .unwrap()
            .contains("supplement"),
        "the supplement is its own document"
    );
    assert!(
        p.files()
            .unwrap()
            .contains(&std::path::PathBuf::from("supplement.tex"))
    );
    // Projects created before the supplement existed remain valid.
    fs::remove_file(f.0.join("supplement.tex")).unwrap();
    assert!(
        !p.files()
            .unwrap()
            .contains(&std::path::PathBuf::from("supplement.tex"))
    );
}

#[test]
fn hidden_tooling_and_editor_backup_files_never_block_a_project() {
    let f = Fixture::new();
    let p = Project::create(&f.0, "Paper", Engine::PdfLatex).unwrap();
    let before = p.files().unwrap();
    fs::create_dir_all(f.0.join(".github/workflows")).unwrap();
    for stray in [
        ".github/workflows/ci.yml",
        ".gitattributes",
        ".editorconfig",
        ".main.tex.folio-123-0",
        ".main.tex.swp",
        "main.tex~",
        "main.tex.bak",
        "sections/introduction.tex~",
        "sections/.introduction.tex.swp",
        "figures/.DS_Store",
        "Thumbs.db",
    ] {
        fs::write(f.0.join(stray), "").unwrap();
        assert_eq!(p.files().unwrap(), before, "{stray} must be ignored");
    }
    p.build_config().unwrap();
    // A visible file in the wrong place is still reported.
    fs::write(f.0.join("Makefile"), "").unwrap();
    assert!(p.files().unwrap_err().to_string().contains("Makefile"));
}

#[test]
fn misplaced_files_and_missing_folders_are_actionable_validation_errors() {
    let f = Fixture::new();
    let p = Project::create(&f.0, "Paper", Engine::PdfLatex).unwrap();
    fs::write(f.0.join("chapter.tex"), "misplaced").unwrap();
    let error = p.files().unwrap_err().to_string();
    assert!(
        error.contains("chapter.tex") && error.contains("sections"),
        "{error}"
    );
    fs::rename(f.0.join("chapter.tex"), f.0.join("sections/chapter.tex")).unwrap();
    assert!(
        p.files()
            .unwrap()
            .iter()
            .any(|p| p.ends_with("sections/chapter.tex"))
    );
    fs::remove_dir_all(f.0.join("figures")).unwrap();
    assert!(p.files().unwrap_err().to_string().contains("figures"));
}

#[test]
fn adding_project_files_enforces_roles_containment_and_no_overwrite() {
    let f = Fixture::new();
    let p = Project::create(&f.0, "Paper", Engine::PdfLatex).unwrap();
    let path = p
        .write_new(
            std::path::Path::new("sections/methods/details.tex"),
            b"Methods",
        )
        .unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"Methods");
    assert!(
        p.write_new(
            std::path::Path::new("sections/methods/details.tex"),
            b"Overwrite"
        )
        .is_err()
    );
    for bad in [
        "../outside.tex",
        "/tmp/outside.tex",
        "chapter.tex",
        "sections/../main.tex",
        "figures/bad.exe",
    ] {
        assert!(
            p.write_new(std::path::Path::new(bad), b"bad").is_err(),
            "{bad}"
        );
    }
    assert_eq!(fs::read(&path).unwrap(), b"Methods");
}

#[test]
fn manifest_configuration_reloads_from_disk_and_relocation_preserves_discovery() {
    let f = Fixture::new();
    let p = Project::create(&f.0, "Paper", Engine::PdfLatex).unwrap();
    let manifest = fs::read_to_string(f.0.join(MANIFEST)).unwrap();
    fs::write(
        f.0.join(MANIFEST),
        manifest
            .replace("pdflatex", "xelatex")
            .replace("120", "345"),
    )
    .unwrap();
    let config = p.build_config().unwrap();
    assert_eq!(config.engine, Engine::XeLatex);
    assert_eq!(config.timeout.as_secs(), 345);
    let relocated = Fixture::new();
    fs::rename(&f.0, &relocated.0).unwrap();
    let p = Project::discover(&relocated.0.join("sections/introduction.tex"))
        .unwrap()
        .unwrap();
    assert_eq!(
        p.build_config().unwrap().root,
        relocated.0.canonicalize().unwrap().join("main.tex")
    );
}

#[test]
#[cfg(unix)]
fn managed_projects_reject_symlinks_including_required_folders() {
    let f = Fixture::new();
    let p = Project::create(&f.0, "Paper", Engine::PdfLatex).unwrap();
    std::os::unix::fs::symlink("../main.tex", f.0.join("sections/link.tex")).unwrap();
    assert!(p.files().unwrap_err().to_string().contains("symlink"));
    fs::remove_file(f.0.join("sections/link.tex")).unwrap();
    std::os::unix::fs::symlink("tables", f.0.join(".folio")).unwrap();
    assert!(
        p.build_config()
            .unwrap_err()
            .to_string()
            .contains("symlink")
    );
    fs::remove_file(f.0.join(".folio")).unwrap();
    fs::remove_dir_all(f.0.join("figures")).unwrap();
    std::os::unix::fs::symlink("tables", f.0.join("figures")).unwrap();
    assert!(p.files().unwrap_err().to_string().contains("symlink"));
}

use std::process::Command;

#[test]
fn cli_build_rejects_files_and_folders_without_projects() {
    let dir = std::env::temp_dir().join(format!("folio-project-only-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.tex"), "source").unwrap();
    for path in [&dir, &dir.join("main.tex")] {
        let output = Command::new(env!("CARGO_BIN_EXE_folio-cli"))
            .arg("build")
            .arg(path)
            .output()
            .unwrap();
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains("project"), "{error}");
        assert!(
            !dir.join(".folio").exists(),
            "refusal must happen before running TeX"
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn cli_creates_validates_and_rejects_invalid_managed_projects() {
    use std::fs;
    let root = std::env::temp_dir().join(format!("folio-cli-project-{}", std::process::id()));
    let binary = env!("CARGO_BIN_EXE_folio-cli");
    let created = Command::new(binary)
        .args(["project", "new"])
        .arg(&root)
        .arg("xe")
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    assert!(
        fs::read_to_string(root.join("folio.project.toml"))
            .unwrap()
            .contains("xelatex")
    );
    let checked = Command::new(binary)
        .args(["project", "check"])
        .arg(root.join("folio.project.toml"))
        .output()
        .unwrap();
    assert!(checked.status.success());
    let relative = Command::new(binary)
        .current_dir(&root)
        .args(["project", "check", "folio.project.toml"])
        .output()
        .unwrap();
    assert!(
        relative.status.success(),
        "{}",
        String::from_utf8_lossy(&relative.stderr)
    );
    fs::write(root.join("misplaced.tex"), "not structured").unwrap();
    let invalid = Command::new(binary)
        .args(["project", "check"])
        .arg(&root)
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("misplaced.tex"));
    let override_engine = Command::new(binary)
        .arg("build")
        .arg(&root)
        .arg("pdf")
        .output()
        .unwrap();
    assert!(!override_engine.status.success());
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
fn fake_build(script: &str, label: &str) -> (std::path::PathBuf, std::process::Output) {
    use std::{fs, os::unix::fs::PermissionsExt};
    let dir = std::env::temp_dir().join(format!("folio-build-{label}-{}", std::process::id()));
    fs::create_dir_all(dir.join("bin")).unwrap();
    let project = folio_core::project::Project::create(
        &dir.join("paper"),
        "Paper",
        folio_core::build::Engine::PdfLatex,
    )
    .unwrap();
    fs::write(project.directory.join("main.tex"), "source").unwrap();
    let tool = dir.join("bin/latexmk");
    fs::write(
        &tool,
        format!("#!/bin/sh\nprintf 'PDF fixture' > .folio/main.pdf\n{script}\n"),
    )
    .unwrap();
    fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_folio-cli"))
        .env("PATH", dir.join("bin"))
        .arg("build")
        .arg(dir.join("paper"))
        .output()
        .unwrap();
    (dir, result)
}

#[test]
#[cfg(unix)]
fn successful_build_accepts_non_utf8_output() {
    let (dir, result) = fake_build(
        "printf '\\377 Latin-1 log\\n'\nprintf 'map' > .folio/main.synctex.gz",
        "bytes",
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[cfg(unix)]
fn successful_pdf_accepts_optional_compressed_or_plain_synctex() {
    for (label, script, extension) in [
        ("no-map", "true", None),
        (
            "plain-map",
            "printf 'map' > .folio/main.synctex",
            Some("synctex"),
        ),
    ] {
        let (dir, result) = fake_build(script, label);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let path = std::path::PathBuf::from(String::from_utf8(result.stdout).unwrap().trim());
        assert!(path.is_file());
        if let Some(extension) = extension {
            assert!(path.with_extension(extension).is_file());
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
#[test]
fn cli_explains_commands_and_rejects_unknown_arguments() {
    let binary = env!("CARGO_BIN_EXE_folio-cli");
    let help = Command::new(binary).arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("build"));
    assert!(
        !Command::new(binary)
            .arg("nonsense")
            .output()
            .unwrap()
            .status
            .success()
    );
}
#[test]
fn cli_reports_missing_inputs_without_panicking() {
    let output = Command::new(env!("CARGO_BIN_EXE_folio-cli"))
        .args(["build", "/does/not/exist.tex"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
}

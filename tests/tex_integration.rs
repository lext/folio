use folio_core::{build::*, synctex};
use std::{
    fs,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};
#[test]
#[ignore = "requires latexmk, TeX, BibTeX, Biber and latexpand"]
fn structured_project_and_flat_exports_build_with_both_bibliography_modes() {
    use folio_core::{
        export::{self, Bibliography},
        project::Project,
    };
    let base = std::env::temp_dir().join(format!("folio-structured-export-{}", std::process::id()));
    fs::create_dir_all(&base).unwrap();
    let p = Project::create(&base.join("paper"), "Export paper", Engine::PdfLatex).unwrap();
    let starter = compile(&p.build_config().unwrap(), Arc::new(AtomicBool::new(false))).unwrap();
    assert!(starter.success, "{}", starter.log);
    // Dotted stems, graphicspath, duplicate basenames and a local BibTeX style
    // must survive relocation into a single directory.
    fs::create_dir_all(p.directory.join("figures/a")).unwrap();
    fs::create_dir_all(p.directory.join("figures/b")).unwrap();
    fs::copy(&starter.pdf, p.directory.join("figures/a/chart.v1.pdf")).unwrap();
    fs::copy(&starter.pdf, p.directory.join("figures/b/chart.v1.pdf")).unwrap();
    fs::create_dir(p.directory.join("styles")).unwrap();
    let bst = std::process::Command::new(executable("kpsewhich"))
        .arg("plain.bst")
        .output()
        .unwrap();
    assert!(bst.status.success());
    fs::copy(
        String::from_utf8(bst.stdout).unwrap().trim(),
        p.directory.join("styles/references.bst"),
    )
    .unwrap();
    fs::write(p.directory.join("sections/introduction.tex"), "\\section{Introduction}\nA paper cites Knuth~\\cite{knuth}.\n\\input{tables/results}\n\\begin{verbatim}\n\\input{not-a-real-file}\n\\end{verbatim}\n").unwrap();
    fs::write(
        p.directory.join("tables/results.tex"),
        "\\begin{tabular}{ll}Method & Score \\\\ Alpha & 42\\end{tabular}\n",
    )
    .unwrap();
    fs::write(p.directory.join("references.bib"), "@book{knuth,author={Donald Knuth},title={The TeXbook},year={1984},publisher={Addison-Wesley}}\n").unwrap();
    let main = fs::read_to_string(p.directory.join("main.tex")).unwrap()
        .replace("\\bibliographystyle{plain}", "\\bibliographystyle{styles/references}")
        .replace("\\begin{document}", "\\graphicspath{{figures/a/}}\n\\begin{document}\n\\includegraphics[width=1cm]{chart.v1}\n\\includegraphics[width=1cm]{figures/b/chart.v1}\n");
    fs::write(p.directory.join("main.tex"), &main).unwrap();
    for mode in [Bibliography::Keep, Bibliography::Inline] {
        let destination = base.join(format!("export-{mode:?}"));
        let result =
            export::flat(&p, &destination, mode, Arc::new(AtomicBool::new(false))).unwrap();
        assert!(result.files >= 2);
        let flat = fs::read_to_string(destination.join("main.tex")).unwrap();
        assert!(flat.contains("Score"));
        assert!(flat.contains("{chart.v1.pdf}"), "{flat}");
        assert!(flat.contains("{chart.v1-2.pdf}"), "{flat}");
        assert!(destination.join("chart.v1.pdf").is_file());
        assert!(destination.join("chart.v1-2.pdf").is_file());
        assert!(!flat.contains("\\input{tables/") && !flat.contains("\\input{sections/"));
        assert!(
            flat.contains("\\input{not-a-real-file}"),
            "verbatim must be preserved"
        );
        assert_eq!(
            destination.join("references.bib").exists(),
            mode == Bibliography::Keep
        );
        if mode == Bibliography::Inline {
            assert!(flat.contains("The TeXbook"));
        }
        assert!(
            fs::read_dir(&destination)
                .unwrap()
                .all(|e| e.unwrap().file_type().unwrap().is_file()),
            "flat means no nested output/cache directories"
        );
        assert!(destination.join("main.pdf").is_file());
        assert_eq!(
            fs::read_to_string(p.directory.join("main.tex")).unwrap(),
            main
        );
        assert!(
            export::flat(&p, &destination, mode, Arc::new(AtomicBool::new(false))).is_err(),
            "never overwrite an export"
        );
    }
    let cancelled = base.join("cancelled");
    assert!(
        export::flat(
            &p,
            &cancelled,
            Bibliography::Keep,
            Arc::new(AtomicBool::new(true))
        )
        .is_err()
    );
    assert!(!cancelled.exists());
    fs::write(
        p.directory.join("sections/introduction.tex"),
        "\\undefinedExportCommand",
    )
    .unwrap();
    let failed = base.join("failed");
    assert!(
        export::flat(
            &p,
            &failed,
            Bibliography::Keep,
            Arc::new(AtomicBool::new(false))
        )
        .is_err()
    );
    assert!(!failed.exists());
    assert!(!fs::read_dir(&base).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".folio-export-")
    }));
    fs::remove_dir_all(base).unwrap();
}
#[test]
#[ignore = "requires latexmk, TeX, Biber, BibLaTeX and latexpand"]
fn flat_biblatex_exports_rebuild_with_retained_or_embedded_bibliography() {
    use folio_core::{
        export::{self, Bibliography},
        project::Project,
    };
    let base = std::env::temp_dir().join(format!("folio-biblatex-export-{}", std::process::id()));
    fs::create_dir_all(&base).unwrap();
    let p = Project::create(&base.join("paper"), "BibLaTeX paper", Engine::PdfLatex).unwrap();
    fs::write(p.directory.join("main.tex"), "\\documentclass{article}\n\\usepackage[backend=biber]{biblatex}\n\\addbibresource{references.bib}\n\\begin{document}\nA citation~\\cite{knuth}.\n\\printbibliography\n\\end{document}\n").unwrap();
    fs::write(p.directory.join("references.bib"), "@book{knuth,author={Donald Knuth},title={The TeXbook},year={1984},publisher={Addison-Wesley}}\n").unwrap();
    for mode in [Bibliography::Keep, Bibliography::Inline] {
        let destination = base.join(format!("export-{mode:?}"));
        export::flat(&p, &destination, mode, Arc::new(AtomicBool::new(false))).unwrap();
        assert_eq!(
            destination.join("references.bib").exists(),
            mode == Bibliography::Keep
        );
        let text = fs::read_to_string(destination.join("main.tex")).unwrap();
        if mode == Bibliography::Inline {
            assert!(text.contains("The TeXbook"));
            assert!(!text.contains("\\addbibresource"));
            assert!(!destination.join("main.bbl").exists());
        }
        // Rebuild after publication with the author's project removed from view.
        let mut config = BuildConfig::new(destination.join("main.tex"), Engine::PdfLatex);
        config.use_latexmkrc = false;
        config.run_bibliography = mode == Bibliography::Keep;
        let result = compile(&config, Arc::new(AtomicBool::new(false))).unwrap();
        assert!(result.success, "{}", result.log);
        let final_log = fs::read_to_string(config.output_dir().join("main.log")).unwrap();
        assert!(!final_log.contains("Empty bibliography"), "{final_log}");
        assert!(
            !final_log.contains("There were undefined references"),
            "{final_log}"
        );
    }
    fs::remove_dir_all(base).unwrap();
}
#[test]
#[ignore = "requires macOS TeX at /Library/TeX/texbin"]
#[cfg(target_os = "macos")]
fn finder_launch_builds_without_tex_in_shell_path() {
    let dir = std::env::temp_dir().join(format!("folio finder {}", std::process::id()));
    folio_core::project::Project::create(&dir, "Finder paper", Engine::PdfLatex).unwrap();
    let main = dir.join("main.tex");
    fs::write(
        &main,
        "\\documentclass{article}\\begin{document}Finder launch.\\end{document}",
    )
    .unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_folio-cli"))
        .env("PATH", "/usr/bin:/bin")
        .args(["build", dir.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let pdf = String::from_utf8(result.stdout).unwrap();
    assert!(std::path::Path::new(pdf.trim()).is_file());
    assert!(pdf.trim().ends_with("main.pdf"));
    fs::remove_dir_all(dir).unwrap();
}
#[test]
#[ignore = "requires latexmk, pdflatex and synctex"]
fn real_multifile_build_and_bidirectional_navigation() {
    let dir = std::env::temp_dir().join(format!("folio tex integration {}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let main = dir.join("main.tex");
    let chapter = dir.join("chapter.tex");
    fs::write(
        &main,
        "\\documentclass{article}\n\\begin{document}\n\\input{chapter}\n\\end{document}\n",
    )
    .unwrap();
    fs::write(&chapter,"\\section{Navigation}\nThis sentence belongs to the included chapter.\nAnother sentence makes a useful source target.\n").unwrap();
    let config = BuildConfig::new(main.clone(), Engine::PdfLatex);
    let result = compile(&config, Arc::new(AtomicBool::new(false))).unwrap();
    assert!(result.success, "{}", result.log);
    assert!(config.output_dir().join("main.synctex.gz").is_file());
    let hits = synctex::forward(&result.pdf, &chapter, 2, 0).unwrap();
    assert!(!hits.is_empty());
    let h = &hits[0];
    let back = synctex::inverse(&result.pdf, h.page, h.x, h.y).unwrap();
    assert_eq!(
        back.path.canonicalize().unwrap(),
        chapter.canonicalize().unwrap()
    );
    assert!((1..=3).contains(&back.line));
    let good_pdf = fs::read(&result.pdf).unwrap();
    let good_map = fs::read(result.pdf.with_extension("synctex.gz")).unwrap();
    fs::write(&chapter, "\\undefinedFolioCommand\n").unwrap();
    let failure = compile(&config, Arc::new(AtomicBool::new(false))).unwrap();
    assert!(!failure.success);
    assert_eq!(
        fs::read(&result.pdf).unwrap(),
        good_pdf,
        "last successful PDF must survive a failed build"
    );
    assert_eq!(
        fs::read(result.pdf.with_extension("synctex.gz")).unwrap(),
        good_map,
        "SyncTeX must describe the visible PDF, not a failed build"
    );
    assert!(!failure.diagnostics.is_empty(), "{}", failure.log);
    fs::remove_dir_all(dir).unwrap();
}
#[test]
#[ignore = "requires latexmk and pdflatex"]
fn runaway_tex_is_terminated_by_timeout() {
    let dir = std::env::temp_dir().join(format!("folio timeout {}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let main = dir.join("main.tex");
    fs::write(
        &main,
        "\\documentclass{article}\n\\begin{document}\\loop\\iftrue\\repeat\\end{document}",
    )
    .unwrap();
    let mut config = BuildConfig::new(main, Engine::PdfLatex);
    config.timeout = Duration::from_millis(200);
    let result = compile(&config, Arc::new(AtomicBool::new(false))).unwrap();
    assert!(result.cancelled);
    assert!(!result.success);
    assert!(result.elapsed < Duration::from_secs(5));
    fs::write(
        config.output_dir().join("main.aux"),
        "\\newlabel{unfinished",
    )
    .unwrap();
    fs::write(
        &config.root,
        "\\documentclass{article}\\begin{document}Recovered.\\end{document}",
    )
    .unwrap();
    config.timeout = Duration::from_secs(20);
    let recovered = compile(&config, Arc::new(AtomicBool::new(false))).unwrap();
    assert!(recovered.success, "{}", recovered.log);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "requires latexmk and XeLaTeX"]
fn xelatex_generates_a_navigable_unicode_pdf() {
    let dir = std::env::temp_dir().join(format!("folio xelatex {}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let main = dir.join("unicode.tex");
    fs::write(&main, "\\documentclass{article}\n\\usepackage{fontspec}\n\\begin{document}\nUnicode: café, naïve, résumé.\n\\end{document}\n").unwrap();
    let result = compile(
        &BuildConfig::new(main.clone(), Engine::XeLatex),
        Arc::new(AtomicBool::new(false)),
    )
    .unwrap();
    assert!(result.success, "{}", result.log);
    let hits = synctex::forward(&result.pdf, &main, 4, 0).unwrap();
    assert!(!hits.is_empty());
    assert_eq!(
        synctex::inverse(&result.pdf, hits[0].page, hits[0].x, hits[0].y)
            .unwrap()
            .path
            .canonicalize()
            .unwrap(),
        main.canonicalize().unwrap()
    );
    fs::remove_dir_all(dir).unwrap();
}
#[test]
#[ignore = "requires latexmk, pdflatex and synctex"]
fn real_pdf_builds_with_plain_or_disabled_synctex_without_reusing_old_maps() {
    let dir = std::env::temp_dir().join(format!("folio optional map {}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let main = dir.join("main.tex");
    let config = BuildConfig::new(main.clone(), Engine::PdfLatex);
    for (mode, expected) in [
        (1, Some("synctex.gz")),
        (0, None),
        (-1, Some("synctex")),
        (0, None),
    ] {
        fs::write(
            dir.join(".latexmkrc"),
            format!("$pdflatex = 'pdflatex %O -synctex={mode} %S';\n"),
        )
        .unwrap();
        fs::write(&main, format!("\\documentclass{{article}}\n\\synctex={mode}\n\\begin{{document}}\nSyncTeX mode {mode}.\n\\end{{document}}\n")).unwrap();
        let result = compile(&config, Arc::new(AtomicBool::new(false))).unwrap();
        assert!(result.success, "{}", result.log);
        for extension in ["synctex.gz", "synctex"] {
            assert_eq!(
                result.pdf.with_extension(extension).exists(),
                expected == Some(extension),
                "mode={mode}, extension={extension}: {}",
                result.log
            );
        }
        if expected.is_some() {
            assert!(
                !synctex::forward(&result.pdf, &main, 4, 0)
                    .unwrap()
                    .is_empty()
            );
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

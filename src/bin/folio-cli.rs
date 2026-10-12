use folio_core::{
    build::{self, Engine},
    export::{self, Bibliography},
    project::Project,
    synctex,
};
use std::{
    io,
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usage = "Folio local LaTeX tools\n\n  folio-cli doctor\n  folio-cli project new DIRECTORY [pdf|xe|lua]\n  folio-cli project check PROJECT\n  folio-cli project export PROJECT DESTINATION [--bibliography bibtex|inline]\n  folio-cli build PROJECT\n  folio-cli forward FILE.pdf SOURCE.tex LINE\n  folio-cli inverse FILE.pdf PAGE X Y\n\nDesktop: cargo run --manifest-path desktop/Cargo.toml -- PROJECT";
    match args.first().map(String::as_str) {
        None | Some("--help" | "-h") => println!("{usage}"),
        Some("doctor") if args.len() == 1 => {
            let mut missing = false;
            // Bibliography tools are needed only by documents that cite.
            for (name, purpose) in [
                ("latexmk", None),
                ("pdflatex", None),
                ("xelatex", None),
                ("lualatex", None),
                ("synctex", None),
                ("latexpand", None),
                ("bibtex", Some("BibTeX bibliographies")),
                ("biber", Some("BibLaTeX bibliographies")),
            ] {
                let path = build::executable(name);
                let exists = path.is_file();
                missing |= !exists && purpose.is_none();
                println!(
                    "{}  {name}: {}{}",
                    match (exists, purpose) {
                        (true, _) => "OK",
                        (false, None) => "MISSING",
                        (false, Some(_)) => "ABSENT",
                    },
                    path.display(),
                    purpose
                        .filter(|_| !exists)
                        .map(|p| format!(" (needed for {p})"))
                        .unwrap_or_default()
                );
            }
            if missing {
                return Err(io::Error::other("One or more TeX tools are missing from PATH").into());
            }
        }
        Some("build") if (2..=3).contains(&args.len()) => {
            if args.len() == 3 {
                return Err(
                    io::Error::other("Set the project engine in folio.project.toml.").into(),
                );
            }
            let config = required_project(Path::new(&args[1]))?.build_config()?;
            let result = build::compile(&config, Arc::new(AtomicBool::new(false)))?;
            if !result.success {
                eprintln!("{}", result.log);
                return Err(io::Error::other(match build::missing_tool(&result.log) {
                    Some(tool) => format!("LaTeX build failed: {}", build::missing_tool_hint(tool)),
                    None => "LaTeX build failed".into(),
                })
                .into());
            }
            for name in build::missing_bibliographies(&result.log) {
                eprintln!("Warning: {}", build::missing_bibliography_hint(name));
            }
            println!("{}", result.pdf.display());
        }
        Some("project") => match args.get(1).map(String::as_str) {
            Some("new") if (3..=4).contains(&args.len()) => {
                let path = Path::new(&args[2]);
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("LaTeX project");
                let project = Project::create(
                    path,
                    name,
                    parse_engine(args.get(3).map(String::as_str).unwrap_or("pdf"))?,
                )?;
                println!("{}", project.directory.display());
            }
            Some("check") if args.len() == 3 => {
                let project = required_project(Path::new(&args[2]))?;
                let files = project.files()?;
                println!(
                    "{} · {} files · {}",
                    project.manifest.name,
                    files.len(),
                    project.manifest.engine.label()
                );
            }
            Some("export")
                if args.len() == 4 || (args.len() == 6 && args[4] == "--bibliography") =>
            {
                let project = required_project(Path::new(&args[2]))?;
                let bibliography = match args.get(5).map(String::as_str).unwrap_or("bibtex") {
                    "bibtex" => Bibliography::Keep,
                    "inline" => Bibliography::Inline,
                    _ => {
                        return Err(
                            io::Error::other("Bibliography must be bibtex or inline").into()
                        );
                    }
                };
                let result = export::flat(
                    &project,
                    Path::new(&args[3]),
                    bibliography,
                    Arc::new(AtomicBool::new(false)),
                )?;
                println!(
                    "{} ({} files; compiled successfully)",
                    result.directory.display(),
                    result.files
                );
            }
            _ => return Err(io::Error::other(format!("Invalid project command.\n{usage}")).into()),
        },
        Some("forward") if args.len() == 4 => {
            let pdf = Path::new(&args[1]).canonicalize()?;
            let source = Path::new(&args[2]).canonicalize()?;
            for hit in synctex::forward(&pdf, &source, args[3].parse()?, 0)? {
                println!("page={} x={} y={}", hit.page, hit.x, hit.y);
            }
        }
        Some("inverse") if args.len() == 5 => {
            let pdf = Path::new(&args[1]).canonicalize()?;
            let hit = synctex::inverse(&pdf, args[2].parse()?, args[3].parse()?, args[4].parse()?)?;
            println!("{}:{}:{}", hit.path.display(), hit.line, hit.column);
        }
        _ => return Err(io::Error::other(format!("Invalid arguments.\n{usage}")).into()),
    }
    Ok(())
}
fn parse_engine(value: &str) -> io::Result<Engine> {
    match value {
        "pdf" => Ok(Engine::PdfLatex),
        "xe" => Ok(Engine::XeLatex),
        "lua" => Ok(Engine::LuaLatex),
        _ => Err(io::Error::other("Engine must be pdf, xe or lua")),
    }
}
fn required_project(path: &Path) -> io::Result<Project> {
    Project::load(path)
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Folio: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

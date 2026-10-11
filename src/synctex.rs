use crate::{build::executable, pdf::Rect};
use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    process::Command,
};
#[derive(Debug, Clone)]
pub struct PdfLocation {
    pub page: usize,
    pub x: f32,
    pub y: f32,
    pub h: Option<f32>,
    pub v: Option<f32>,
    pub width: Option<f32>,
    pub height: Option<f32>,
}
impl PdfLocation {
    pub fn rect(&self) -> Rect {
        let height = self.height.unwrap_or(12.);
        Rect {
            x: self.h.unwrap_or(self.x),
            y: self.v.map_or(self.y, |v| v - height),
            width: self.width.unwrap_or(80.),
            height,
        }
    }
}
#[derive(Debug, Clone)]
pub struct SourceLocation {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
}
fn invalid() -> io::Error {
    io::Error::other("No valid SyncTeX location. Build the document with SyncTeX enabled first.")
}
fn number(fields: &HashMap<&str, &str>, name: &str) -> Option<f32> {
    fields
        .get(name)?
        .parse::<f32>()
        .ok()
        .filter(|n| n.is_finite())
}
pub fn parse_view(output: &str) -> io::Result<Vec<PdfLocation>> {
    let mut records = vec![];
    let mut fields = HashMap::new();
    let mut inside = false;
    for line in output.lines() {
        if line == "SyncTeX result begin" {
            inside = true;
            continue;
        }
        if line == "SyncTeX result end" {
            break;
        }
        if !inside {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            if key == "Page" && fields.contains_key("Page") {
                records.push(fields);
                fields = HashMap::new();
            }
            fields.insert(key, value.trim());
        }
    }
    if !fields.is_empty() {
        records.push(fields);
    }
    let result: Vec<_> = records
        .into_iter()
        .filter_map(|f| {
            let page = f.get("Page")?.parse::<usize>().ok()?;
            if page == 0 {
                return None;
            }
            Some(PdfLocation {
                page,
                x: number(&f, "x")?,
                y: number(&f, "y")?,
                h: number(&f, "h"),
                v: number(&f, "v"),
                width: number(&f, "W"),
                height: number(&f, "H"),
            })
        })
        .collect();
    if result.is_empty() {
        Err(invalid())
    } else {
        Ok(result)
    }
}
pub fn parse_edit(output: &str) -> io::Result<SourceLocation> {
    let mut path = None;
    let mut line = None;
    let mut column = 0;
    for row in output.lines() {
        if let Some((k, v)) = row.split_once(':') {
            match k {
                "Input" => path = Some(PathBuf::from(v)),
                "Line" => line = v.parse::<usize>().ok(),
                "Column" => column = v.parse::<isize>().unwrap_or(0).max(0) as usize,
                _ => {}
            }
        }
    }
    match (path, line) {
        (Some(path), Some(line)) if line > 0 && !path.as_os_str().is_empty() => {
            Ok(SourceLocation { path, line, column })
        }
        _ => Err(invalid()),
    }
}
pub fn forward(
    pdf: &Path,
    source: &Path,
    line: usize,
    column: usize,
) -> io::Result<Vec<PdfLocation>> {
    require_map(pdf)?;
    let output = Command::new(executable("synctex"))
        .args(["view", "-i"])
        .arg(format!("{}:{}:{}", line.max(1), column, source.display()))
        .arg("-o")
        .arg(pdf)
        .output()?;
    if !output.status.success() {
        return Err(invalid());
    }
    parse_view(&String::from_utf8_lossy(&output.stdout))
}
pub fn inverse(pdf: &Path, page: usize, x: f32, y: f32) -> io::Result<SourceLocation> {
    require_map(pdf)?;
    if page == 0 || !x.is_finite() || !y.is_finite() {
        return Err(invalid());
    }
    let output = Command::new(executable("synctex"))
        .args(["edit", "-o"])
        .arg(format!("{page}:{x}:{y}:{}", pdf.display()))
        .output()?;
    if !output.status.success() {
        return Err(invalid());
    }
    let mut result = parse_edit(&String::from_utf8_lossy(&output.stdout))?;
    if result.path.is_relative() {
        // Immutable previews may be nested below .folio.
        let directory = pdf.parent().unwrap_or(Path::new("."));
        let base = directory
            .ancestors()
            .find(|p| p.file_name().is_some_and(|n| n == ".folio"))
            .and_then(Path::parent)
            .unwrap_or(directory);
        result.path = base.join(&result.path);
    }
    Ok(result)
}
fn require_map(pdf: &Path) -> io::Result<()> {
    if ["synctex.gz", "synctex"]
        .iter()
        .any(|extension| pdf.with_extension(extension).is_file())
    {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "This PDF has no SyncTeX map. Enable SyncTeX and recompile to use source navigation.",
        ))
    }
}

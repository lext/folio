//! References from the paper into its supplement, made literal for a paper
//! exported without one.
//!
//! The paper cites supplementary material with xr or xr-hyper:
//! `\externaldocument[S-]{supplement}` imports the labels of
//! `supplement.aux`, so `\ref{S-tab:scores}` prints the supplement's
//! number. A journal copy has no supplement to import from, so each such
//! reference is replaced by the text it typesets, and the import is removed.
use super::{apply_edits, error, group, skip_space};
use crate::syntax::{self, TokenKind};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    ops::Range,
    path::Path,
};

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Label {
    number: String,
    page: String,
    title: String,
    anchor: String,
    cref: Option<Cref>,
}
/// cleveref's record of a label: its type, and the counter values that
/// order and compress lists such as `\cref{a,b,c}`.
#[derive(Clone, Debug, PartialEq)]
struct Cref {
    kind: String,
    value: u64,
    enclosing: String,
    text: String,
}

/// The labels a compiled document defines, from its `.aux` and the
/// `.aux` files of its `\include`d parts.
pub(super) fn labels(aux: &Path) -> io::Result<BTreeMap<String, Label>> {
    let mut labels = BTreeMap::new();
    let mut crefs = BTreeMap::new();
    read_aux(aux, &mut labels, &mut crefs, 0)?;
    for (key, cref) in crefs {
        if let Some(label) = labels.get_mut(&key) {
            label.cref = Some(cref);
        }
    }
    Ok(labels)
}
fn read_aux(
    aux: &Path,
    labels: &mut BTreeMap<String, Label>,
    crefs: &mut BTreeMap<String, Cref>,
    depth: usize,
) -> io::Result<()> {
    let text = fs::read_to_string(aux).map_err(|_| {
        error(format!(
            "No compiled supplement labels ({}); the supplement must build first",
            aux.display()
        ))
    })?;
    let mut at = 0;
    while let Some(found) = text[at..].find(['\\']) {
        let start = at + found;
        at = start + 1;
        let rest = &text[start..];
        if let Some(name) = rest.strip_prefix("\\@input{") {
            let file = &name[..name.find('}').unwrap_or(0)];
            let path = aux.parent().unwrap_or(Path::new("")).join(file);
            if depth < 16 && path.is_file() {
                read_aux(&path, labels, crefs, depth + 1)?;
            }
            continue;
        }
        if !rest.starts_with("\\newlabel{") {
            continue;
        }
        let (key, next) = group(&text, start + "\\newlabel".len(), '{', '}')?;
        let value_start = skip_space(&text, next);
        if !text[value_start..].starts_with('{') {
            continue;
        }
        let (value, next) = group(&text, value_start, '{', '}')?;
        at = next;
        let fields = fields(&text[value.clone()])?;
        let key = text[key].to_owned();
        if let Some(base) = key.strip_suffix("@cref") {
            if let Some(cref) = fields.first().and_then(|f| parse_cref(f)) {
                crefs.insert(base.to_owned(), cref);
            }
            continue;
        }
        let field = |i: usize| fields.get(i).cloned().unwrap_or_default();
        labels.insert(
            key,
            Label {
                number: field(0),
                page: field(1),
                title: field(2),
                anchor: field(3),
                cref: None,
            },
        );
    }
    Ok(())
}
/// The braced fields of a `\newlabel` value, such as `{S1}{3}{Scores}{table.1}{}`.
fn fields(value: &str) -> io::Result<Vec<String>> {
    let mut result = vec![];
    let mut at = skip_space(value, 0);
    while value[at..].starts_with('{') {
        let (range, next) = group(value, at, '{', '}')?;
        result.push(value[range].to_owned());
        at = skip_space(value, next);
    }
    Ok(result)
}
/// `[table][1][]S1`: type, counter value, enclosing counter values, label.
fn parse_cref(field: &str) -> Option<Cref> {
    let mut parts = vec![];
    let mut rest = field;
    for _ in 0..3 {
        let inner = rest.strip_prefix('[')?;
        let end = inner.find(']')?;
        parts.push(&inner[..end]);
        rest = &inner[end + 1..];
    }
    Some(Cref {
        kind: parts[0].to_owned(),
        value: parts[1].trim().parse().unwrap_or(0),
        enclosing: parts[2].to_owned(),
        text: rest.to_owned(),
    })
}

/// Replace the paper's references into the supplement with literal text and
/// remove the import. A paper that does not import the supplement is returned
/// unchanged.
pub(super) fn resolve(text: &str, supplement: &BTreeMap<String, Label>) -> io::Result<String> {
    let commands = commands(text);
    let mut prefixes = BTreeSet::new();
    let mut edits = vec![];
    let mut other_imports = false;
    for command in &commands {
        if command.name != "externaldocument" {
            continue;
        }
        let mut at = skip_space(text, command.range.end);
        let mut prefix = String::new();
        let mut options = 0;
        while text[at..].starts_with('[') {
            let (range, next) = group(text, at, '[', ']')?;
            if options == 0 {
                prefix = text[range].trim().to_owned();
            }
            options += 1;
            at = skip_space(text, next);
        }
        if !text[at..].starts_with('{') {
            return Err(error(
                "Flat export requires a literal braced argument for \\externaldocument",
            ));
        }
        let (file, mut end) = group(text, at, '{', '}')?;
        // xr-hyper accepts a trailing [url] for the external PDF.
        let after = skip_space(text, end);
        if text[after..].starts_with('[') {
            end = group(text, after, '[', ']')?.1;
        }
        let file = text[file].trim();
        let file = file.strip_prefix("./").unwrap_or(file);
        if matches!(file, "supplement" | "supplement.tex" | "supplement.aux") {
            prefixes.insert(prefix);
            edits.push((command.range.start..end, String::new()));
        } else {
            other_imports = true;
        }
    }
    if prefixes.is_empty() {
        return Ok(text.to_owned());
    }
    let external: BTreeMap<String, &Label> = prefixes
        .iter()
        .flat_map(|prefix| {
            supplement
                .iter()
                .map(move |(key, label)| (format!("{prefix}{key}"), label))
        })
        .collect();
    for command in &commands {
        let name = command.name.as_str();
        let starred = name.ends_with('*');
        let base = name.trim_end_matches('*');
        let handled = matches!(
            base,
            "ref" | "pageref" | "eqref" | "nameref" | "autoref" | "cref" | "Cref" | "hyperref"
        );
        let unsupported = matches!(
            base,
            "crefrange"
                | "Crefrange"
                | "cpageref"
                | "Cpageref"
                | "cpagerefrange"
                | "Cpagerefrange"
                | "labelcref"
                | "labelcpageref"
                | "namecref"
                | "nameCref"
                | "lcnamecref"
                | "namecrefs"
                | "nameCrefs"
                | "lcnamecrefs"
                | "vref"
                | "Vref"
                | "vpageref"
                | "autopageref"
        );
        if base == "usepackage" || base == "RequirePackage" {
            if !other_imports && let Some(edit) = drop_xr(text, command.range.clone())? {
                edits.push(edit);
            }
            continue;
        }
        if !handled && !unsupported {
            continue;
        }
        let mut at = skip_space(text, command.range.end);
        if base == "hyperref" {
            // Only the label form, \hyperref[key]{text}, refers to a label.
            if !text[at..].starts_with('[') {
                continue;
            }
            let (key, next) = group(text, at, '[', ']')?;
            let key = text[key].trim();
            if !external.contains_key(key) {
                continue;
            }
            let start = skip_space(text, next);
            if !text[start..].starts_with('{') {
                continue;
            }
            let (body, end) = group(text, start, '{', '}')?;
            edits.push((command.range.start..end, text[body].to_owned()));
            continue;
        }
        if starred
            && !matches!(
                base,
                "ref" | "pageref" | "nameref" | "autoref" | "cref" | "Cref"
            )
        {
            continue;
        }
        if !text[at..].starts_with('{') {
            continue;
        }
        let (argument, end) = group(text, at, '{', '}')?;
        at = end;
        let keys: Vec<&str> = if matches!(base, "cref" | "Cref") || unsupported {
            text[argument.clone()].split(',').map(str::trim).collect()
        } else {
            vec![text[argument.clone()].trim()]
        };
        let found: Vec<_> = keys.iter().filter_map(|k| external.get(*k)).collect();
        if found.is_empty() {
            continue;
        }
        let shown = text[command.range.start..at].to_owned();
        if unsupported {
            return Err(error(format!(
                "Flat export without the supplement cannot resolve {shown}; use \\ref or \\cref for supplementary references"
            )));
        }
        if found.len() != keys.len() {
            return Err(error(format!(
                "{shown} mixes labels from the paper and the supplement; split it into separate references"
            )));
        }
        let label = found[0];
        let replacement = match base {
            "ref" => label.number.clone(),
            "pageref" => label.page.clone(),
            "nameref" => label.title.clone(),
            "eqref" => format!("\\textup{{({})}}", label.number),
            "autoref" => {
                let kind = label.anchor.split('.').next().unwrap_or("");
                if kind.is_empty() || !kind.chars().all(|c| c.is_ascii_alphabetic()) {
                    return Err(error(format!(
                        "{shown} has no reference type; load hyperref in supplement.tex or use \\ref"
                    )));
                }
                format!("\\{kind}autorefname~{}", label.number)
            }
            _ => cref(base == "Cref", &found, &shown)?,
        };
        edits.push((command.range.start..at, replacement));
    }
    apply_edits(text, edits)
}

/// cleveref's own text for a list of supplement labels, from the paper's
/// format macros so `\crefname`/`\crefformat` settings still apply. The
/// hyperlink arguments are empty: there is nothing to link to.
fn cref(capital: bool, labels: &[&&Label], shown: &str) -> io::Result<String> {
    let mut crefs = vec![];
    for label in labels {
        crefs.push(label.cref.as_ref().ok_or_else(|| {
            error(format!(
                "{shown} needs cleveref in supplement.tex as well, so its labels record their type"
            ))
        })?);
    }
    let kind = &crefs[0].kind;
    if crefs.iter().any(|c| &c.kind != kind) {
        return Err(error(format!(
            "{shown} mixes reference types from the supplement; split it by type"
        )));
    }
    crefs.sort_by(|a, b| (&a.enclosing, a.value).cmp(&(&b.enclosing, b.value)));
    crefs.dedup_by(|a, b| a.enclosing == b.enclosing && a.value == b.value);
    // Like cleveref, compress three or more consecutive values into a range.
    let mut groups: Vec<Vec<&Cref>> = vec![];
    for c in crefs {
        match groups.last_mut() {
            Some(run)
                if run.last().is_some_and(|last| {
                    last.enclosing == c.enclosing && last.value + 1 == c.value
                }) =>
            {
                run.push(c)
            }
            _ => groups.push(vec![c]),
        }
    }
    let groups: Vec<Vec<&Cref>> = groups
        .into_iter()
        .flat_map(|run| {
            if run.len() >= 3 {
                vec![run]
            } else {
                run.into_iter().map(|c| vec![c]).collect()
            }
        })
        .collect();
    let (single, range) = if capital {
        ("Cref", "Crefrange")
    } else {
        ("cref", "crefrange")
    };
    let count = groups.len();
    Ok(groups
        .iter()
        .enumerate()
        .map(|(i, run)| {
            let position = match (count, i) {
                (1, _) => "",
                (_, 0) => "@first",
                (2, _) => "@second",
                _ if i + 1 == count => "@last",
                _ => "@middle",
            };
            if run.len() == 1 {
                format!(
                    "\\csname {single}@{kind}@format{position}\\endcsname{{{}}}{{}}{{}}",
                    run[0].text
                )
            } else {
                format!(
                    "\\csname {range}@{kind}@format{position}\\endcsname{{{}}}{{{}}}{{}}{{}}{{}}{{}}",
                    run[0].text,
                    run[run.len() - 1].text
                )
            }
        })
        .collect())
}

/// Remove xr and xr-hyper from a package list, or the whole call when they
/// are all it loads.
fn drop_xr(text: &str, command: Range<usize>) -> io::Result<Option<(Range<usize>, String)>> {
    let mut at = skip_space(text, command.end);
    while text[at..].starts_with('[') {
        at = skip_space(text, group(text, at, '[', ']')?.1);
    }
    if !text[at..].starts_with('{') {
        return Ok(None);
    }
    let (list, mut end) = group(text, at, '{', '}')?;
    let packages: Vec<&str> = text[list.clone()].split(',').map(str::trim).collect();
    let kept: Vec<&str> = packages
        .iter()
        .copied()
        .filter(|p| !matches!(*p, "xr" | "xr-hyper"))
        .collect();
    if kept.len() == packages.len() {
        return Ok(None);
    }
    if kept.is_empty() {
        // A trailing [version] belongs to the call.
        let after = skip_space(text, end);
        if text[after..].starts_with('[') {
            end = group(text, after, '[', ']')?.1;
        }
        return Ok(Some((command.start..end, String::new())));
    }
    Ok(Some((list, kept.join(","))))
}

struct Command {
    name: String,
    range: Range<usize>,
}
/// Control sequences outside comments and verbatim, including those inside
/// math, which the editor's tokenizer folds into a single token.
fn commands(text: &str) -> Vec<Command> {
    let protected: Vec<Range<usize>> = syntax::tokenize(text)
        .into_iter()
        .filter(|t| matches!(t.kind, TokenKind::Comment | TokenKind::Verbatim))
        .map(|t| t.range)
        .collect();
    let mut result = vec![];
    let mut skip = protected.iter().peekable();
    let mut i = 0;
    while let Some(found) = text[i..].find('\\') {
        let start = i + found;
        while skip.peek().is_some_and(|r| r.end <= start) {
            skip.next();
        }
        if let Some(r) = skip.peek()
            && r.start <= start
        {
            i = r.end;
            continue;
        }
        let mut end = start + 1;
        let letters = text[end..]
            .find(|c: char| !c.is_ascii_alphabetic())
            .unwrap_or(text.len() - end);
        if letters == 0 {
            // \\, \% and other control symbols.
            i = end + text[end..].chars().next().map_or(0, char::len_utf8);
            continue;
        }
        end += letters;
        if text[end..].starts_with('*') {
            end += 1;
        }
        result.push(Command {
            name: text[start + 1..end].to_owned(),
            range: start..end,
        });
        i = end;
    }
    result
}

/// References and citations a LaTeX log reports as undefined, such as
/// `Reference `tab:x' on page 1 undefined` or BibLaTeX's
/// `Citation 'knuth' on page 1 undefined`.
pub(super) fn undefined(log: &str) -> BTreeSet<String> {
    log.lines()
        .filter_map(|line| {
            let (kind, rest) = ["Reference `", "Citation `", "Citation '"]
                .iter()
                .find_map(|marker| Some((*marker, line.split(marker).nth(1)?)))?;
            let end = rest.find('\'')?;
            rest[end..].contains("undefined").then(|| {
                let kind = kind.trim_end_matches(['`', '\'', ' ']).to_lowercase();
                format!("{kind} {}", &rest[..end])
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn label(number: &str, anchor: &str, cref: Option<(&str, u64)>) -> Label {
        Label {
            number: number.into(),
            page: "4".into(),
            title: "Scores".into(),
            anchor: anchor.into(),
            cref: cref.map(|(kind, value)| Cref {
                kind: kind.into(),
                value,
                enclosing: String::new(),
                text: number.into(),
            }),
        }
    }
    fn supplement() -> BTreeMap<String, Label> {
        BTreeMap::from([
            ("tab:a".into(), label("S1", "table.1", Some(("table", 1)))),
            ("tab:b".into(), label("S2", "table.2", Some(("table", 2)))),
            ("tab:c".into(), label("S3", "table.3", Some(("table", 3)))),
            ("tab:e".into(), label("S5", "table.5", Some(("table", 5)))),
            ("fig:a".into(), label("S1", "figure.1", Some(("figure", 1)))),
        ])
    }
    #[test]
    fn aux_labels_include_hyperref_fields_cleveref_types_and_included_parts() {
        let dir = std::env::temp_dir().join(format!("folio-xr-aux-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("supplement.aux"),
            "\\relax\n\\newlabel{tab:scores}{{S1}{3}{Scores \\& more}{table.1}{}}\n\\newlabel{tab:scores@cref}{{[table][1][]S1}{[1][3][]3}{}{}{}}\n\\@input{part.aux}\n\\newlabel{plain}{{2}{5}}\n",
        )
        .unwrap();
        fs::write(
            dir.join("part.aux"),
            "\\newlabel{fig:x}{{S4}{9}{}{figure.4}{}}\n",
        )
        .unwrap();
        let labels = labels(&dir.join("supplement.aux")).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        let scores = &labels["tab:scores"];
        assert_eq!(
            (
                scores.number.as_str(),
                scores.page.as_str(),
                scores.title.as_str()
            ),
            ("S1", "3", "Scores \\& more")
        );
        assert_eq!(scores.cref.as_ref().unwrap().kind, "table");
        assert_eq!(labels["plain"].number, "2");
        assert_eq!(labels["fig:x"].anchor, "figure.4");
        assert!(!labels.contains_key("tab:scores@cref"));
    }
    #[test]
    fn supplementary_references_become_literal_and_the_import_is_removed() {
        let text = "\\usepackage{xr-hyper}\n\\usepackage{amsmath,xr}\n\\externaldocument[S-]{supplement}\nSee \\ref{S-tab:a}, \\ref{own}, \\autoref{S-fig:a}, \\nameref{S-tab:a}, page~\\pageref{S-tab:a}, \\eqref{S-tab:b}, \\hyperref[S-tab:a]{the table}.\n% \\ref{S-tab:a}\n$x_{\\ref{S-tab:b}}$ \\\\ref{S-tab:a}";
        let flat = resolve(text, &supplement()).unwrap();
        assert!(
            !flat.contains("externaldocument") && !flat.contains("xr"),
            "{flat}"
        );
        assert!(flat.contains("\\usepackage{amsmath}"), "{flat}");
        assert!(
            flat.contains("See S1, \\ref{own}, \\figureautorefname~S1, Scores, page~4, \\textup{(S2)}, the table."),
            "{flat}"
        );
        assert!(
            flat.contains("% \\ref{S-tab:a}"),
            "comments are left as written"
        );
        assert!(flat.contains("$x_{S2}$"), "{flat}");
        assert!(
            flat.contains("\\\\ref{S-tab:a}"),
            "a line break is not a command"
        );
    }
    #[test]
    fn cleveref_lists_use_the_documents_formats_sorted_and_compressed() {
        let text = "\\externaldocument{supplement}\\cref{tab:b}|\\Cref{tab:e,tab:a}|\\cref{tab:c,tab:a,tab:b}|\\cref{tab:a,tab:b,tab:c,tab:e}";
        let flat = resolve(text, &supplement()).unwrap();
        let parts: Vec<&str> = flat.split('|').collect();
        assert_eq!(parts[0], "\\csname cref@table@format\\endcsname{S2}{}{}");
        assert_eq!(
            parts[1],
            "\\csname Cref@table@format@first\\endcsname{S1}{}{}\\csname Cref@table@format@second\\endcsname{S5}{}{}"
        );
        assert_eq!(
            parts[2],
            "\\csname crefrange@table@format\\endcsname{S1}{S3}{}{}{}{}"
        );
        assert_eq!(
            parts[3],
            "\\csname crefrange@table@format@first\\endcsname{S1}{S3}{}{}{}{}\\csname cref@table@format@second\\endcsname{S5}{}{}"
        );
    }
    #[test]
    fn ambiguous_supplementary_references_are_rejected() {
        for text in [
            "\\externaldocument{supplement}\\cref{own,tab:a}",
            "\\externaldocument{supplement}\\cref{tab:a,fig:a}",
            "\\externaldocument{supplement}\\vref{tab:a}",
        ] {
            assert!(resolve(text, &supplement()).is_err(), "{text}");
        }
        // Another external document keeps xr loaded.
        let flat = resolve(
            "\\usepackage{xr}\\externaldocument{supplement}\\externaldocument{other}",
            &supplement(),
        )
        .unwrap();
        assert_eq!(flat, "\\usepackage{xr}\\externaldocument{other}");
        let unchanged = "\\ref{tab:a}";
        assert_eq!(resolve(unchanged, &supplement()).unwrap(), unchanged);
    }
    #[test]
    fn undefined_references_are_read_from_the_log() {
        let log = "LaTeX Warning: Reference `S-tab:x' on page 1 undefined on input line 7.\nLaTeX Warning: Label `y' multiply defined.\nLaTeX Warning: Citation `knuth' on page 1 undefined on input line 9.\nLaTeX Warning: Citation 'lamport' on page 2 undefined on input line 3.\n";
        assert_eq!(
            undefined(log),
            BTreeSet::from([
                "reference S-tab:x".to_owned(),
                "citation knuth".to_owned(),
                "citation lamport".to_owned()
            ])
        );
    }
}

use crate::{bibliography::Entry, syntax};
use std::ops::Range;

#[derive(Clone, Debug)]
pub struct Item {
    pub insert: String,
    pub detail: String,
}
#[derive(Clone, Debug)]
pub struct Suggestions {
    pub range: Range<usize>,
    pub items: Vec<Item>,
}

/// Parsed once per file revision; queries never tokenize the project again.
#[derive(Clone, Debug, Default)]
pub struct Index {
    commands: Vec<String>,
    labels: Vec<String>,
    citations: Vec<Citation>,
    ignored: Vec<Range<usize>>,
}
#[derive(Clone, Debug)]
struct Citation {
    item: Item,
    key: String,
    details: String,
}
impl Index {
    pub fn source(source: &str) -> Self {
        let mut index = Self::default();
        for token in syntax::tokenize(source) {
            match token.kind {
                syntax::TokenKind::Command => {
                    let name = &source[token.range.clone()];
                    index.commands.push(name.into());
                    if let Some(key) = syntax::braced(source, token.range.end) {
                        match name {
                            "\\label" => index.labels.push(key),
                            "\\bibitem" => index.add_citation(Item {
                                insert: key,
                                detail: "Bibliography item".into(),
                            }),
                            _ => {}
                        }
                    }
                }
                syntax::TokenKind::Comment | syntax::TokenKind::Verbatim => {
                    index.ignored.push(token.range)
                }
                _ => {}
            }
        }
        index.commands.sort();
        index.commands.dedup();
        index.labels.sort();
        index.labels.dedup();
        index
    }
    pub fn bibliography(entries: &[Entry]) -> Self {
        let mut index = Self::default();
        for entry in entries {
            index.add_citation(Item {
                insert: entry.key.clone(),
                detail: format!("{} · {} · {}", entry.author, entry.title, entry.year),
            });
        }
        index
    }
    fn add_citation(&mut self, item: Item) {
        self.citations.push(Citation {
            key: item.insert.to_lowercase(),
            details: item.detail.to_lowercase(),
            item,
        });
    }
}

pub fn suggest(
    source: &str,
    cursor: usize,
    project: &str,
    bibliography: &[Entry],
) -> Option<Suggestions> {
    let active = Index::source(source);
    let project = Index::source(project);
    let bibliography = Index::bibliography(bibliography);
    suggest_indexed(source, cursor, &active, [&project, &bibliography])
}
pub fn suggest_indexed<'a>(
    source: &str,
    cursor: usize,
    active: &Index,
    project: impl IntoIterator<Item = &'a Index>,
) -> Option<Suggestions> {
    if cursor > source.len() || !source.is_char_boundary(cursor) {
        return None;
    }
    let before = &source[..cursor];
    if active
        .ignored
        .iter()
        .any(|r| r.start < cursor && cursor <= r.end)
    {
        return None;
    }
    let start = before
        .char_indices()
        .rev()
        .find(|(_, c)| !c.is_alphanumeric() && !matches!(c, '\\' | ':' | '_' | '-' | '@' | '.'))
        .map_or(0, |(i, c)| i + c.len_utf8());
    let prefix = &source[start..cursor];
    let mut end = cursor;
    for ch in source[cursor..].chars() {
        if !(ch.is_alphanumeric() || matches!(ch, ':' | '_' | '-' | '@' | '.')) {
            break;
        }
        end += ch.len_utf8();
    }
    let lower = prefix.to_lowercase();
    let mut items = Vec::new();
    if prefix.starts_with('\\') {
        if prefix.len() < 2 {
            return None;
        }
        items = syntax::COMMANDS
            .iter()
            .copied()
            .chain(
                project
                    .into_iter()
                    .flat_map(|i| i.commands.iter().map(String::as_str)),
            )
            .filter(|s| s.starts_with(prefix))
            .map(|insert| Item {
                insert: insert.into(),
                detail: "Command".into(),
            })
            .collect();
    } else {
        let open = before.rfind('{')?;
        // Only the currently open argument: optional [prenote][postnote] is allowed.
        if before[open + 1..].contains(['{', '}']) {
            return None;
        }
        let command = argument_command(&before[..open])?;
        if matches!(
            command,
            "\\cite"
                | "\\citep"
                | "\\citet"
                | "\\parencite"
                | "\\textcite"
                | "\\autocite"
                | "\\footcite"
                | "\\nocite"
                | "\\smartcite"
                | "\\supercite"
        ) {
            let mut matches = std::collections::BTreeMap::new();
            for index in project {
                for citation in &index.citations {
                    if citation.key.starts_with(&lower)
                        || (!lower.is_empty() && citation.details.contains(&lower))
                    {
                        let key = citation.item.insert.as_str();
                        if matches.len() == 40
                            && matches
                                .last_key_value()
                                .is_some_and(|(&last, _)| key >= last)
                        {
                            continue;
                        }
                        matches.entry(key).or_insert(&citation.item);
                        if matches.len() > 40 {
                            matches.pop_last();
                        }
                    }
                }
            }
            items = matches.into_values().cloned().collect();
        } else if matches!(
            command,
            "\\ref" | "\\eqref" | "\\pageref" | "\\autoref" | "\\cref" | "\\Cref" | "\\vref"
        ) {
            for key in project.into_iter().flat_map(|i| &i.labels) {
                if key.to_lowercase().starts_with(&lower) {
                    items.push(Item {
                        insert: key.clone(),
                        detail: "Label".into(),
                    });
                }
            }
        } else if matches!(command, "\\begin" | "\\end") {
            for env in [
                "document",
                "equation",
                "equation*",
                "align",
                "align*",
                "aligned",
                "gather",
                "gather*",
                "itemize",
                "enumerate",
                "description",
                "figure",
                "table",
                "tabular",
                "abstract",
                "proof",
                "theorem",
                "verbatim",
                "lstlisting",
                "minipage",
                "center",
                "cases",
                "matrix",
                "pmatrix",
                "bmatrix",
            ] {
                if env.starts_with(prefix) {
                    items.push(Item {
                        insert: env.into(),
                        detail: "Environment".into(),
                    });
                }
            }
        } else {
            return None;
        }
    }
    items.sort_by(|a, b| a.insert.cmp(&b.insert));
    items.dedup_by(|a, b| a.insert == b.insert);
    items.truncate(40);
    (!items.is_empty()).then_some(Suggestions {
        range: start..end,
        items,
    })
}
fn argument_command(mut head: &str) -> Option<&str> {
    head = head.trim_end();
    while head.ends_with(']') {
        let (mut depth, mut braces, mut opening) = (0usize, 0usize, None);
        for (i, ch) in head.char_indices().rev() {
            let escaped = head[..i].bytes().rev().take_while(|b| *b == b'\\').count() % 2 == 1;
            if escaped {
                continue;
            }
            match ch {
                '}' => braces += 1,
                '{' => braces = braces.saturating_sub(1),
                ']' if braces == 0 => depth += 1,
                '[' if braces == 0 => {
                    depth -= 1;
                    if depth == 0 {
                        opening = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        head = head[..opening?].trim_end();
    }
    let start = head.rfind('\\')?;
    Some(head[start..].trim_end_matches('*'))
}

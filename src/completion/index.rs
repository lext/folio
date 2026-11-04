//! What one file contributes to completion, built once per file revision.
use super::{Item, catalog, fuzzy};
use crate::{bibliography::Entry, syntax};
use std::{collections::HashMap, ops::Range};

#[derive(Clone, Debug, Default)]
pub struct Index {
    /// Commands used in the file, with how often.
    pub(super) commands: Vec<(String, u32)>,
    /// Commands the file defines.
    pub(super) macros: Vec<Macro>,
    /// Environments opened with `\begin`, with how often.
    pub(super) environments: Vec<(String, u32)>,
    /// Environments the file defines (`\newenvironment`, `\newtheorem`), with a description.
    pub(super) defined_environments: Vec<(String, String)>,
    pub(super) labels: Vec<Label>,
    pub(super) citations: Vec<Citation>,
    /// Citation keys used in the file, with how often.
    pub(super) cited: Vec<(String, u32)>,
    /// Loaded packages and the document class.
    pub(super) packages: Vec<String>,
    pub(super) colors: Vec<String>,
    /// Comments and verbatim text: no completion inside.
    pub(super) ignored: Vec<Range<usize>>,
    /// Inline and display math, including math environments.
    pub(super) math: Vec<Range<usize>>,
    /// Every environment instance, from `\begin` to the end of its `\end`
    /// (or the end of the file when it is not closed yet).
    pub(super) scopes: Vec<(String, Range<usize>)>,
    /// Sectioning commands: where the title ends, and the title.
    pub(super) sections: Vec<(usize, String)>,
}

#[derive(Clone, Debug)]
pub(super) struct Macro {
    pub name: String,
    pub arguments: usize,
    pub optional: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LabelKind {
    Equation,
    Figure,
    Table,
    Section,
    Item,
    Theorem,
    Other,
}

#[derive(Clone, Debug)]
pub(super) struct Label {
    pub kind: LabelKind,
    /// Key and description, ready to offer.
    pub item: Item,
}

#[derive(Clone, Debug)]
pub(super) struct Citation {
    pub item: Item,
    /// Folded key and metadata, for accent- and case-insensitive search.
    pub key: String,
    pub details: String,
}

fn skip_space(text: &str, mut i: usize) -> usize {
    while let Some(c) = text[i..].chars().next() {
        if !c.is_whitespace() {
            break;
        }
        i += c.len_utf8();
    }
    i
}

/// The balanced group opening at `i` (after whitespace): its content and the
/// index just past its closing delimiter.
pub(super) fn group(text: &str, i: usize, open: char, close: char) -> Option<(String, usize)> {
    let mut i = skip_space(text, i);
    if !text[i..].starts_with(open) {
        return None;
    }
    i += 1;
    let start = i;
    let mut depth = 0usize;
    while i < text.len() {
        let c = text[i..].chars().next()?;
        match c {
            '\\' => {
                i += 1;
                if let Some(c) = text[i..].chars().next() {
                    i += c.len_utf8();
                }
                continue;
            }
            '{' => depth += 1,
            '}' if depth > 0 => depth -= 1,
            c if c == close && depth == 0 => return Some((text[start..i].to_owned(), i + 1)),
            '}' => return None,
            _ => {}
        }
        i += c.len_utf8();
    }
    None
}

/// The first mandatory argument after `i`, skipping optional `[...]` groups.
fn argument(text: &str, mut i: usize) -> Option<(String, usize)> {
    while let Some((_, end)) = group(text, i, '[', ']') {
        i = end;
    }
    group(text, i, '{', '}')
}

/// A command name written directly (`\foo`) or braced (`{\foo}`).
fn command_name(text: &str, i: usize) -> Option<(String, usize)> {
    if let Some((name, end)) = group(text, i, '{', '}') {
        let name = name.trim();
        return (name.starts_with('\\') && name.len() > 1).then(|| (name.to_owned(), end));
    }
    let i = skip_space(text, i);
    let rest = text[i..].strip_prefix('\\')?;
    let length = rest
        .char_indices()
        .find(|(_, c)| !(c.is_alphabetic() || *c == '@'))
        .map_or(rest.len(), |(n, _)| n);
    (length > 0).then(|| (format!("\\{}", &rest[..length]), i + 1 + length))
}

fn is_float(name: &str) -> Option<LabelKind> {
    let name = name.trim_end_matches('*');
    if name.contains("figure") || name == "subfloat" {
        Some(LabelKind::Figure)
    } else if name.contains("table") || name == "longtable" {
        Some(LabelKind::Table)
    } else {
        None
    }
}

const SECTIONS: &[&str] = &[
    "\\part",
    "\\chapter",
    "\\section",
    "\\subsection",
    "\\subsubsection",
    "\\paragraph",
    "\\subparagraph",
];

pub(super) fn is_cite(command: &str) -> bool {
    let lower = command.to_ascii_lowercase();
    lower.contains("cite") && !lower.contains("citestyle") && !lower.ends_with("citeprefix")
}

fn bump(map: &mut HashMap<String, u32>, key: &str) {
    *map.entry(key.to_owned()).or_default() += 1;
}

fn sorted(map: HashMap<String, u32>) -> Vec<(String, u32)> {
    let mut list: Vec<_> = map.into_iter().collect();
    list.sort();
    list
}

impl Index {
    pub fn source(source: &str) -> Self {
        let mut index = Self::default();
        let mut commands = HashMap::new();
        let mut environments = HashMap::new();
        let mut cited = HashMap::new();
        // Open environments: name, start, scope number.
        let mut stack: Vec<(String, usize, usize)> = vec![];
        let mut captions: HashMap<usize, String> = HashMap::new();
        // Labels waiting for their float's caption: label number, scope number.
        let mut pending: Vec<(usize, usize)> = vec![];
        let mut theorems: Vec<String> = vec![];
        let mut last_section: Option<(usize, String)> = None;
        let line_starts: Vec<usize> = std::iter::once(0)
            .chain(source.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        let line = |at: usize| line_starts.partition_point(|&s| s <= at);
        for token in syntax::tokenize(source) {
            let at = token.range.start;
            let end = token.range.end;
            match token.kind {
                syntax::TokenKind::Comment | syntax::TokenKind::Verbatim => {
                    index.ignored.push(token.range);
                    continue;
                }
                syntax::TokenKind::Math => {
                    index.math.push(token.range);
                    continue;
                }
                syntax::TokenKind::Command => {}
                _ => continue,
            }
            let name = &source[token.range.clone()];
            if name[1..].starts_with(|c: char| c.is_alphabetic() || c == '@') {
                bump(&mut commands, name);
            }
            let bare = name.trim_end_matches('*');
            match bare {
                "\\begin" => {
                    if let Some((env, _)) = group(source, end, '{', '}') {
                        let env = env.trim().to_owned();
                        bump(&mut environments, &env);
                        let number = index.scopes.len();
                        index.scopes.push((env.clone(), at..source.len()));
                        stack.push((env, at, number));
                    }
                }
                "\\end" => {
                    if let Some((env, after)) = group(source, end, '{', '}') {
                        let env = env.trim();
                        if let Some(position) = stack.iter().rposition(|(name, ..)| name == env) {
                            for (_, _, number) in stack.drain(position..) {
                                index.scopes[number].1.end = after;
                            }
                        }
                    }
                }
                "\\label" => {
                    if let Some((key, _)) = group(source, end, '{', '}') {
                        let key = key.trim().to_owned();
                        let mut kind = LabelKind::Other;
                        let mut float = None;
                        let mut what = String::new();
                        for (env, _, number) in stack.iter().rev() {
                            if catalog::MATH_ENVIRONMENTS.contains(&env.as_str()) {
                                kind = LabelKind::Equation;
                                what = "Equation".into();
                            } else if let Some(found) = is_float(env) {
                                kind = found;
                                float = Some(*number);
                                what = if found == LabelKind::Figure {
                                    "Figure"
                                } else {
                                    "Table"
                                }
                                .into();
                            } else if theorems.contains(env)
                                || [
                                    "theorem",
                                    "lemma",
                                    "proposition",
                                    "corollary",
                                    "definition",
                                    "remark",
                                    "example",
                                ]
                                .contains(&env.as_str())
                            {
                                kind = LabelKind::Theorem;
                                let mut title = env.clone();
                                if let Some(first) = title.get_mut(0..1) {
                                    first.make_ascii_uppercase();
                                }
                                what = title;
                            } else if matches!(env.as_str(), "enumerate") {
                                kind = LabelKind::Item;
                                what = "Item".into();
                            } else {
                                continue;
                            }
                            break;
                        }
                        let mut context = String::new();
                        if kind == LabelKind::Other
                            && let Some((title_end, title)) = &last_section
                            && source[*title_end..at].trim().is_empty()
                        {
                            kind = LabelKind::Section;
                            what = "Section".into();
                            context = title.clone();
                        } else if let Some((_, title)) = &last_section {
                            context = format!("§ {title}");
                        }
                        if what.is_empty() {
                            what = "Label".into();
                        }
                        let detail = [what, context, format!("line {}", line(at))]
                            .into_iter()
                            .filter(|s| !s.is_empty())
                            .collect::<Vec<_>>()
                            .join(" · ");
                        if let Some(number) = float {
                            pending.push((index.labels.len(), number));
                        }
                        index.labels.push(Label {
                            kind,
                            item: Item::new(&key, &detail),
                        });
                    }
                }
                "\\caption" | "\\captionof" | "\\subcaption" => {
                    let start = if bare == "\\captionof" {
                        group(source, end, '{', '}').map_or(end, |(_, after)| after)
                    } else {
                        end
                    };
                    if let Some((text, _)) = argument(source, start)
                        && let Some((_, _, number)) =
                            stack.iter().rev().find(|(env, ..)| is_float(env).is_some())
                    {
                        captions.entry(*number).or_insert_with(|| plain(&text));
                    }
                }
                "\\bibitem" => {
                    if let Some((key, _)) = argument(source, end) {
                        index.add_citation(Item::new(key.trim(), "Bibliography item"));
                    }
                }
                "\\usepackage" | "\\RequirePackage" | "\\documentclass" => {
                    if let Some((list, _)) = argument(source, end) {
                        index.packages.extend(
                            list.split(',')
                                .map(str::trim)
                                .filter(|p| !p.is_empty())
                                .map(str::to_owned),
                        );
                    }
                }
                "\\newcommand" | "\\renewcommand" | "\\providecommand" => {
                    if let Some((macro_name, mut after)) = command_name(source, end) {
                        let mut arguments = 0;
                        let mut optional = false;
                        if let Some((count, next)) = group(source, after, '[', ']') {
                            arguments = count.trim().parse().unwrap_or(0);
                            after = next;
                            optional = group(source, after, '[', ']').is_some();
                        }
                        index.macros.push(Macro {
                            name: macro_name,
                            arguments,
                            optional,
                        });
                    }
                }
                "\\DeclareMathOperator" => {
                    if let Some((macro_name, _)) = command_name(source, end) {
                        index.macros.push(Macro {
                            name: macro_name,
                            arguments: 0,
                            optional: false,
                        });
                    }
                }
                "\\def" | "\\gdef" | "\\edef" | "\\xdef" => {
                    if let Some((macro_name, after)) = command_name(source, end) {
                        let parameters =
                            &source[after..source[after..].find('{').map_or(after, |n| after + n)];
                        let arguments = parameters.matches('#').count();
                        index.macros.push(Macro {
                            name: macro_name,
                            arguments,
                            optional: false,
                        });
                    }
                }
                "\\NewDocumentCommand"
                | "\\RenewDocumentCommand"
                | "\\ProvideDocumentCommand"
                | "\\DeclareDocumentCommand" => {
                    if let Some((macro_name, after)) = command_name(source, end)
                        && let Some((spec, _)) = group(source, after, '{', '}')
                    {
                        // Mandatory (`m`) arguments get braces on insert; optional ones are skipped.
                        let arguments = spec.chars().filter(|c| *c == 'm').count();
                        index.macros.push(Macro {
                            name: macro_name,
                            arguments,
                            optional: false,
                        });
                    }
                }
                "\\newenvironment" | "\\renewenvironment" | "\\NewDocumentEnvironment" => {
                    if let Some((env, _)) = group(source, end, '{', '}') {
                        index
                            .defined_environments
                            .push((env.trim().to_owned(), "Defined environment".into()));
                    }
                }
                "\\newtheorem" | "\\declaretheorem" => {
                    let found = if bare == "\\declaretheorem" {
                        argument(source, end)
                    } else {
                        group(source, end, '{', '}')
                    };
                    if let Some((env, after)) = found {
                        let env = env.trim().to_owned();
                        // \newtheorem{name}[counter]{Title} or {name}{Title}[within]
                        let after = group(source, after, '[', ']').map_or(after, |(_, next)| next);
                        let title = group(source, after, '{', '}').map(|(t, _)| t);
                        let detail = title.map_or("Theorem-like".into(), |t| {
                            format!("Theorem-like · {}", t.trim())
                        });
                        theorems.push(env.clone());
                        index.defined_environments.push((env, detail));
                    }
                }
                "\\definecolor" | "\\colorlet" => {
                    if let Some((color, _)) = group(source, end, '{', '}') {
                        index.colors.push(color.trim().to_owned());
                    }
                }
                _ if SECTIONS.contains(&bare) => {
                    if let Some((title, after)) = argument(source, end) {
                        let title = plain(&title);
                        index.sections.push((after, title.clone()));
                        last_section = Some((after, title));
                    }
                }
                _ if is_cite(bare) => {
                    if let Some((keys, _)) = argument(source, end) {
                        for key in keys.split(',').map(str::trim).filter(|k| !k.is_empty()) {
                            bump(&mut cited, key);
                        }
                    }
                }
                _ => {}
            }
        }
        for (label, scope) in pending {
            if let Some(caption) = captions.get(&scope) {
                let label = &mut index.labels[label];
                let detail = &label.item.detail;
                let (what, rest) = detail.split_once(" · ").unwrap_or((detail, ""));
                label.item.detail = format!("{what} · {caption} · {rest}");
            }
        }
        for (env, range) in &index.scopes {
            if catalog::MATH_ENVIRONMENTS.contains(&env.as_str()) {
                index.math.push(range.clone());
            }
        }
        index.commands = sorted(commands);
        index.environments = sorted(environments);
        index.cited = sorted(cited);
        index.packages.sort();
        index.packages.dedup();
        index.colors.sort();
        index.colors.dedup();
        index
    }
    pub fn bibliography(entries: &[Entry]) -> Self {
        let mut index = Self::default();
        for entry in entries {
            let detail = [&entry.author, &entry.title, &entry.year]
                .into_iter()
                .filter(|s| !s.is_empty())
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(" · ");
            index.add_citation(Item::new(&entry.key, &detail));
        }
        index
    }
    fn add_citation(&mut self, item: Item) {
        self.citations.push(Citation {
            key: fuzzy::fold(&item.insert),
            details: fuzzy::fold(&item.detail),
            item,
        });
    }
    /// Whether `at` is inside math.
    pub(super) fn in_math(&self, at: usize) -> bool {
        self.math.iter().any(|r| r.start < at && at < r.end)
    }
}

/// A title or caption as readable text: commands and braces removed.
fn plain(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                // Drop the command name; keep its argument text.
                while chars.peek().is_some_and(|c| c.is_alphabetic()) {
                    chars.next();
                }
                if chars
                    .peek()
                    .is_some_and(|c| !c.is_alphanumeric() && *c != '{')
                {
                    chars.next();
                }
            }
            '{' | '}' | '~' => {
                if c == '~' {
                    out.push(' ');
                }
            }
            c => out.push(c),
        }
    }
    let joined = out.split_whitespace().collect::<Vec<_>>().join(" ");
    if joined.chars().count() > 60 {
        format!("{}…", joined.chars().take(59).collect::<String>())
    } else {
        joined
    }
}

//! Rule-based, project-aware LaTeX completion.
//!
//! Each file is indexed once per revision ([`Index`]). A query works out what
//! the caret is in — a command name, or a particular argument of a particular
//! command — collects candidates for that context from the catalogue, the
//! project indexes and the project's files, then ranks them.
mod catalog;
mod fuzzy;
mod index;

use crate::bibliography::Entry;
pub use index::Index;
use index::LabelKind;
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// What the list shows.
    pub label: String,
    /// Replaces the suggestion range when accepted.
    pub insert: String,
    pub detail: String,
    /// Caret position after accepting, as a byte offset into `insert`.
    pub cursor: usize,
}
impl Item {
    fn new(text: &str, detail: &str) -> Self {
        Self {
            label: text.into(),
            insert: text.into(),
            detail: detail.into(),
            cursor: text.len(),
        }
    }
    /// `¦` in `text` marks the caret.
    fn snippet(label: &str, text: &str, detail: &str) -> Self {
        let cursor = text.find('¦').unwrap_or(text.len());
        Self {
            label: label.into(),
            insert: text.replacen('¦', "", 1),
            detail: detail.into(),
            cursor,
        }
    }
}
#[derive(Clone, Debug)]
pub struct Suggestions {
    pub range: Range<usize>,
    pub items: Vec<Item>,
}

/// Most suggestions a query returns.
pub const LIMIT: usize = 40;

/// Convenience for one-off queries: indexes everything first.
pub fn suggest(
    source: &str,
    cursor: usize,
    project: &str,
    bibliography: &[Entry],
) -> Option<Suggestions> {
    let active = Index::source(source);
    let project = Index::source(project);
    let bibliography = Index::bibliography(bibliography);
    suggest_in(source, cursor, &active, &[&project, &bibliography], &[])
}
pub fn suggest_indexed<'a>(
    source: &str,
    cursor: usize,
    active: &Index,
    project: impl IntoIterator<Item = &'a Index>,
) -> Option<Suggestions> {
    let project: Vec<&Index> = project.into_iter().collect();
    suggest_in(source, cursor, active, &project, &[])
}

/// Complete at byte `cursor` of `source`. `active` indexes `source` itself;
/// `project` holds every project file's index (it may include `active`);
/// `files` lists project files relative to the main file's folder, with `/`.
pub fn suggest_in(
    source: &str,
    cursor: usize,
    active: &Index,
    project: &[&Index],
    files: &[String],
) -> Option<Suggestions> {
    if cursor > source.len() || !source.is_char_boundary(cursor) {
        return None;
    }
    if active
        .ignored
        .iter()
        .any(|r| r.start < cursor && cursor <= r.end)
    {
        return None;
    }
    let query = Query {
        source,
        cursor,
        active,
        project,
        files,
        loaded: loaded(active, project),
    };
    let before = &source[..cursor];
    let (range, mut ranked) = if let Some(start) = command_start(before) {
        let end = cursor
            + source[cursor..]
                .char_indices()
                .find(|(_, c)| !(c.is_alphabetic() || *c == '@'))
                .map_or(source.len() - cursor, |(n, _)| n);
        let prefix = &source[start + 1..cursor];
        if prefix.is_empty() {
            return None;
        }
        (
            start..end,
            query.commands(prefix, &source[start..end], &source[end..]),
        )
    } else {
        let argument = Argument::at(before)?;
        let start = argument.item_start;
        let end = cursor
            + source[cursor..]
                .char_indices()
                .find(|(_, c)| matches!(c, ',' | '}' | ']' | '{' | '[') || c.is_whitespace())
                .map_or(source.len() - cursor, |(n, _)| n);
        let prefix = source[start..cursor].trim_start();
        let start = cursor - prefix.len();
        (
            start..end,
            query.argument(&argument, prefix, &source[end..])?,
        )
    };
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.label.cmp(&b.1.label)));
    let mut seen = HashSet::new();
    let items: Vec<Item> = ranked
        .into_iter()
        .map(|(_, item)| item)
        .filter(|item| seen.insert(item.label.clone()))
        .take(LIMIT)
        .collect();
    (!items.is_empty()).then_some(Suggestions { range, items })
}

/// Where a command name being typed starts (its backslash), if the caret
/// ends one. `\\` is a line break, not a command.
fn command_start(before: &str) -> Option<usize> {
    let letters = before
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_alphabetic() || *c == '@')
        .last()
        .map_or(before.len(), |(i, _)| i);
    let head = &before[..letters];
    let slash = head.strip_suffix('\\')?;
    let run = slash.bytes().rev().take_while(|b| *b == b'\\').count();
    (run % 2 == 0).then_some(letters - 1)
}

/// The argument the caret is in.
struct Argument<'a> {
    /// Command name without backslash or star.
    command: &'a str,
    /// `{` or `[`.
    opener: char,
    /// How many mandatory arguments precede this one.
    position: usize,
    /// Contents of the preceding mandatory arguments, first to last.
    previous: Vec<&'a str>,
    /// Text of the whole argument up to the caret.
    text: &'a str,
    /// Where the current comma-separated item starts.
    item_start: usize,
}

fn escaped(text: &str, at: usize) -> bool {
    text[..at].bytes().rev().take_while(|b| *b == b'\\').count() % 2 == 1
}

/// The matching opener for the closer at `close`, scanning backwards.
fn opening(text: &str, close: usize, open: char, closer: char) -> Option<usize> {
    let mut depth = 0usize;
    for (i, c) in text[..=close].char_indices().rev() {
        if escaped(text, i) {
            continue;
        }
        if c == closer {
            depth += 1;
        } else if c == open {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

impl<'a> Argument<'a> {
    fn at(before: &'a str) -> Option<Self> {
        // The innermost unclosed `{` or `[`, within the current paragraph.
        let floor = before
            .rfind("\n\n")
            .unwrap_or(0)
            .max(before.len().saturating_sub(4000));
        let floor = (floor..=before.len()).find(|i| before.is_char_boundary(*i))?;
        let (mut braces, mut brackets) = (0usize, 0usize);
        let mut found = None;
        for (i, c) in before[floor..].char_indices().rev() {
            let i = floor + i;
            if escaped(before, i) {
                continue;
            }
            match c {
                '}' => braces += 1,
                ']' => brackets += 1,
                '{' if braces == 0 => {
                    found = Some((i, '{'));
                    break;
                }
                '{' => braces -= 1,
                '[' if brackets == 0 && braces == 0 => {
                    found = Some((i, '['));
                    break;
                }
                '[' => brackets = brackets.saturating_sub(1),
                _ => {}
            }
        }
        let (open, opener) = found?;
        let text = &before[open + 1..];
        let mut head = &before[..open];
        let mut previous = vec![];
        loop {
            let trimmed = head.trim_end();
            if trimmed.ends_with('}') && !escaped(trimmed, trimmed.len() - 1) {
                let at = opening(trimmed, trimmed.len() - 1, '{', '}')?;
                previous.push(&trimmed[at + 1..trimmed.len() - 1]);
                head = &trimmed[..at];
            } else if trimmed.ends_with(']') && !escaped(trimmed, trimmed.len() - 1) {
                head = &trimmed[..opening(trimmed, trimmed.len() - 1, '[', ']')?];
            } else if trimmed.ends_with('>') {
                head = &trimmed[..trimmed.rfind('<')?];
            } else {
                head = trimmed;
                break;
            }
        }
        previous.reverse();
        let head = head.strip_suffix('*').unwrap_or(head);
        let name_start = head
            .char_indices()
            .rev()
            .take_while(|(_, c)| c.is_alphabetic() || *c == '@')
            .last()?
            .0;
        if !head[..name_start].ends_with('\\') || escaped(head, name_start - 1) {
            return None;
        }
        let command = &head[name_start..];
        let item_start = open + 1 + text.rfind(',').map_or(0, |n| n + 1);
        Some(Self {
            command,
            opener,
            position: previous.len(),
            previous,
            text,
            item_start,
        })
    }
}

type Ranked = Vec<(i32, Item)>;

/// Packages a package or class loads for you.
const IMPLIES: &[(&str, &[&str])] = &[
    ("mathtools", &["amsmath"]),
    ("physics", &["amsmath"]),
    ("amsart", &["amsmath", "amsthm", "amssymb"]),
    ("amsbook", &["amsmath", "amsthm", "amssymb"]),
    ("beamer", &["amsthm", "xcolor", "hyperref", "graphicx"]),
    ("tikz", &["xcolor"]),
    ("pgfplots", &["tikz", "xcolor"]),
    ("subcaption", &["caption"]),
    ("cleveref", &["hyperref"]),
    ("algpseudocode", &["algorithmic"]),
    ("tcolorbox", &["xcolor"]),
    ("todonotes", &["xcolor", "tikz"]),
    (
        "acmart",
        &[
            "amsmath", "amssymb", "graphicx", "hyperref", "xcolor", "natbib", "booktabs",
        ],
    ),
    ("IEEEtran", &[]),
    ("revtex4-2", &["amsmath"]),
];

fn loaded(active: &Index, project: &[&Index]) -> HashSet<String> {
    let mut set: HashSet<String> = std::iter::once(active)
        .chain(project.iter().copied())
        .flat_map(|i| i.packages.iter().cloned())
        .collect();
    let mut added = true;
    while added {
        added = false;
        for (package, implied) in IMPLIES {
            if set.contains(*package) {
                for p in *implied {
                    added |= set.insert((*p).to_owned());
                }
            }
        }
    }
    set
}

struct Query<'a> {
    source: &'a str,
    cursor: usize,
    active: &'a Index,
    project: &'a [&'a Index],
    files: &'a [String],
    loaded: HashSet<String>,
}

/// Turn argument hints into the text to insert: empty mandatory groups,
/// optional ones dropped, the caret in the first group.
fn arguments(hints: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    let mut skipping = None;
    for c in hints.chars() {
        if let Some(close) = skipping {
            if c == close {
                skipping = None;
            }
            continue;
        }
        match c {
            '[' if depth == 0 => skipping = Some(']'),
            '<' if depth == 0 => skipping = Some('>'),
            '{' => {
                if depth == 0 {
                    out.push_str(if out.contains('¦') { "{" } else { "{¦" });
                }
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    out.push('}');
                }
            }
            _ => {}
        }
    }
    out
}

fn package_detail(hints: &str, package: &str) -> String {
    match (hints.is_empty(), package.is_empty()) {
        (true, true) => String::new(),
        (false, true) => hints.into(),
        (true, false) => package.into(),
        (false, false) => format!("{hints} · {package}"),
    }
}

impl Query<'_> {
    fn all(&self) -> impl Iterator<Item = &Index> {
        std::iter::once(self.active).chain(self.project.iter().copied())
    }
    fn counts<'b>(
        &'b self,
        pick: impl Fn(&'b Index) -> &'b [(String, u32)],
    ) -> HashMap<&'b str, u32> {
        let mut counts = HashMap::new();
        for index in self.project.iter().copied().chain(
            // The active file's own usage counts once even when it is also a project index.
            (!self.project.iter().any(|i| std::ptr::eq(*i, self.active))).then_some(self.active),
        ) {
            for (name, n) in pick(index) {
                *counts.entry(name.as_str()).or_default() += n;
            }
        }
        counts
    }
    fn available(&self, package: &str) -> bool {
        package.is_empty() || self.loaded.contains(package)
    }

    fn commands(&self, prefix: &str, typed: &str, after: &str) -> Ranked {
        let math = self.active.in_math(self.cursor);
        // Arguments already follow: replace the name only.
        let bare = after.starts_with(['{', '[']);
        let usage = self.counts(|i| &i.commands);
        let used = |name: &str| usage.get(name).copied().unwrap_or(0).min(30) as i32 * 4;
        let mut ranked = vec![];
        let mut known = HashSet::new();
        for (order, &(name, hints, package, mode)) in catalog::COMMANDS.iter().enumerate() {
            let Some(mut score) = fuzzy::score(name, prefix) else {
                continue;
            };
            // The catalogue lists common commands first within each group.
            score -= order as i32 / 40;
            let label = format!("\\{name}");
            let mut detail = package_detail(hints, package);
            if !self.available(package) {
                if score < 900 {
                    continue;
                }
                score -= 400;
                detail = format!("{hints} · needs \\usepackage{{{package}}}");
            }
            score += match (mode, math) {
                (catalog::Mode::Math, true) | (catalog::Mode::Text, false) => 60,
                (catalog::Mode::Math, false) => -80,
                (catalog::Mode::Text, true) => -150,
                (catalog::Mode::Any, _) => 0,
            };
            score += used(&label);
            let insert = if bare {
                label.clone()
            } else {
                format!("{label}{}", arguments(hints))
            };
            known.insert(label.clone());
            ranked.push((
                score,
                Item::snippet(&label, &insert, detail.trim_start_matches(" · ")),
            ));
        }
        for index in self.all() {
            for m in &index.macros {
                let Some(score) = fuzzy::score(&m.name[1..], prefix) else {
                    continue;
                };
                let mut insert = m.name.clone();
                if !bare {
                    let mandatory = m.arguments - usize::from(m.optional && m.arguments > 0);
                    for n in 0..mandatory {
                        insert.push_str(if n == 0 { "{¦}" } else { "{}" });
                    }
                }
                let detail = match m.arguments {
                    0 => "Defined in project".to_owned(),
                    1 => "Defined in project · 1 argument".to_owned(),
                    n => format!("Defined in project · {n} arguments"),
                };
                known.insert(m.name.clone());
                ranked.push((
                    score + 100 + used(&m.name),
                    Item::snippet(&m.name, &insert, &detail),
                ));
            }
        }
        for (&name, &count) in &usage {
            // The command being typed is in the index too; do not offer it back.
            if known.contains(name) || (count == 1 && name == typed) {
                continue;
            }
            if let Some(score) = fuzzy::score(&name[1..], prefix) {
                ranked.push((score - 50 + used(name), Item::new(name, "Used in project")));
            }
        }
        ranked
    }

    fn argument(&self, argument: &Argument, prefix: &str, after: &str) -> Option<Ranked> {
        let command = argument.command;
        if argument.opener == '[' {
            return self.options(argument, prefix);
        }
        let ranked = match (command, argument.position) {
            ("begin", 0) => self.environments(prefix, after),
            ("end", 0) => self.closing(prefix, after),
            ("label", 0) => self.label_key(prefix),
            (c, _) if index::is_cite(c) || c == "nocite" => self.citations(argument, prefix),
            (
                "ref" | "pageref" | "eqref" | "autoref" | "Autoref" | "nameref" | "cref" | "Cref"
                | "cpageref" | "Cpageref" | "labelcref" | "vref" | "Vref" | "subref" | "crefrange"
                | "Crefrange",
                _,
            ) => self.labels(command, prefix),
            ("input" | "include" | "subfile" | "includeonly" | "InputIfFileExists", 0) => {
                self.files(prefix, &["tex"], true)
            }
            ("includegraphics", 0) => {
                self.files(prefix, &["pdf", "png", "jpg", "jpeg", "eps", "svg"], false)
            }
            ("includepdf", 0) => self.files(prefix, &["pdf"], false),
            ("lstinputlisting" | "verbatiminput", 0) | ("inputminted", 1) => {
                self.files(prefix, &[], false)
            }
            ("addbibresource", 0) => self.files(prefix, &["bib"], false),
            ("bibliography", 0) => self.files(prefix, &["bib"], true),
            ("bibliographystyle", 0) => {
                let mut ranked = self.files(prefix, &["bst"], true);
                ranked.extend(words(
                    catalog::BIBLIOGRAPHY_STYLES,
                    prefix,
                    "Bibliography style",
                ));
                ranked
            }
            ("usepackage" | "RequirePackage", 0) => {
                let mut ranked = self.files(prefix, &["sty"], true);
                for (package, description) in catalog::PACKAGES {
                    if let Some(score) = fuzzy::score(package, prefix) {
                        ranked.push((score, Item::new(package, description)));
                    }
                }
                ranked
            }
            ("documentclass", 0) => {
                let mut ranked = self.files(prefix, &["cls"], true);
                for (class, description) in catalog::CLASSES {
                    if let Some(score) = fuzzy::score(class, prefix) {
                        ranked.push((score, Item::new(class, description)));
                    }
                }
                ranked
            }
            ("color" | "textcolor" | "colorbox" | "pagecolor" | "rowcolor" | "cellcolor", 0)
            | ("fcolorbox", 0 | 1)
            | ("colorlet", 1) => {
                let mut ranked = words(catalog::COLORS, prefix, "Colour");
                for color in self.all().flat_map(|i| &i.colors) {
                    if let Some(score) = fuzzy::score(color, prefix) {
                        ranked.push((score + 50, Item::new(color, "Defined colour")));
                    }
                }
                ranked
            }
            ("pagestyle" | "thispagestyle", 0) => words(catalog::PAGE_STYLES, prefix, "Page style"),
            ("pagenumbering", 0) => words(
                &["arabic", "roman", "Roman", "alph", "Alph"],
                prefix,
                "Numbering",
            ),
            ("theoremstyle", 0) => {
                words(&["plain", "definition", "remark"], prefix, "Theorem style")
            }
            ("setlength" | "addtolength", 0) => words(catalog::LENGTHS, prefix, "Length"),
            ("usetikzlibrary", 0) => words(catalog::TIKZ_LIBRARIES, prefix, "TikZ library"),
            _ => return None,
        };
        Some(ranked)
    }

    fn options(&self, argument: &Argument, prefix: &str) -> Option<Ranked> {
        if prefix.contains('=') {
            return None;
        }
        Some(match argument.command {
            "includegraphics" => words(catalog::GRAPHICS_KEYS, prefix, "Graphics option"),
            "documentclass" => words(catalog::CLASS_OPTIONS, prefix, "Class option"),
            "hyperref" => self.labels("ref", prefix),
            "begin"
                if argument.previous.first().is_some_and(|env| {
                    ["figure", "figure*", "table", "table*", "algorithm"].contains(&env.trim())
                }) =>
            {
                if !argument.text.is_empty() && argument.text != prefix {
                    return None;
                }
                catalog::FLOAT_PLACEMENTS
                    .iter()
                    .filter(|(p, _)| p.starts_with(prefix))
                    .enumerate()
                    .map(|(n, (p, d))| (-(n as i32), Item::new(p, d)))
                    .collect()
            }
            _ => return None,
        })
    }

    fn environments(&self, prefix: &str, after: &str) -> Ranked {
        let usage = self.counts(|i| &i.environments);
        let used = |name: &str| usage.get(name).copied().unwrap_or(0).min(30) as i32 * 4;
        let line_start = self.source[..self.cursor].rfind('\n').map_or(0, |n| n + 1);
        let indent: String = self.source[line_start..]
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        // Renaming an existing environment: insert only the name.
        let renaming = after.starts_with('}');
        let make = |name: &str, hints: &str, detail: &str| {
            if renaming {
                return Item::new(name, detail);
            }
            let mut body = catalog::environment_body(name).to_owned();
            let mut head = arguments(hints);
            if head.contains('¦') {
                body = body.replace('¦', "");
            } else {
                head.clear();
            }
            let inner = format!("{indent}  ");
            let body = body
                .lines()
                .map(|l| format!("{inner}{l}"))
                .collect::<Vec<_>>()
                .join("\n");
            let text = format!("{name}}}{head}\n{body}\n{indent}\\end{{{name}}}");
            let mut item = Item::snippet(name, &text, detail);
            if !text.contains('¦') {
                item.cursor = item.insert.len();
            }
            item
        };
        let mut ranked = vec![];
        let mut known = HashSet::new();
        for &(name, hints, package) in catalog::ENVIRONMENTS {
            let Some(mut score) = fuzzy::score(name, prefix) else {
                continue;
            };
            let mut detail = package_detail(hints, package);
            if !self.available(package) {
                if score < 900 {
                    continue;
                }
                score -= 400;
                detail = format!("needs \\usepackage{{{package}}}");
            }
            known.insert(name.to_owned());
            ranked.push((score + used(name), make(name, hints, &detail)));
        }
        for index in self.all() {
            for (name, detail) in &index.defined_environments {
                if let Some(score) = fuzzy::score(name, prefix) {
                    known.insert(name.clone());
                    ranked.push((score + 100 + used(name), make(name, "", detail)));
                }
            }
        }
        for name in usage.keys() {
            if !known.contains(*name)
                && let Some(score) = fuzzy::score(name, prefix)
            {
                ranked.push((score - 50 + used(name), make(name, "", "Used in project")));
            }
        }
        ranked
    }

    fn closing(&self, prefix: &str, after: &str) -> Ranked {
        let close = if after.starts_with('}') { "" } else { "}" };
        // Open environments around the caret, innermost first.
        let mut open: Vec<&(String, Range<usize>)> = self
            .active
            .scopes
            .iter()
            .filter(|(_, r)| r.start < self.cursor && self.cursor <= r.end)
            .collect();
        open.sort_by_key(|(_, r)| std::cmp::Reverse(r.start));
        let mut ranked = vec![];
        for (n, (name, _)) in open.iter().enumerate() {
            if let Some(score) = fuzzy::score(name, prefix) {
                let detail = if n == 0 {
                    "Close the open environment"
                } else {
                    "Open environment"
                };
                ranked.push((
                    score + 2000 - n as i32 * 10,
                    Item::new(name, detail).with_suffix(close),
                ));
            }
        }
        for &(name, _, _) in catalog::ENVIRONMENTS {
            if let Some(score) = fuzzy::score(name, prefix) {
                ranked.push((score, Item::new(name, "Environment").with_suffix(close)));
            }
        }
        ranked
    }

    fn label_key(&self, prefix: &str) -> Ranked {
        let scope = self
            .active
            .scopes
            .iter()
            .filter(|(_, r)| r.start < self.cursor && self.cursor <= r.end)
            .max_by_key(|(_, r)| r.start);
        let suggestion = match scope.map(|(name, _)| name.as_str()) {
            Some(env) if catalog::MATH_ENVIRONMENTS.contains(&env) => "eq:".to_owned(),
            Some(env) if env.contains("figure") => "fig:".into(),
            Some(env) if env.contains("table") => "tab:".into(),
            Some("enumerate") => "item:".into(),
            Some(env)
                if self
                    .all()
                    .any(|i| i.defined_environments.iter().any(|(e, _)| e == env)) =>
            {
                format!("{}:", &env[..env.len().min(3)])
            }
            _ => match self
                .active
                .sections
                .iter()
                .rev()
                .find(|(end, _)| *end <= self.cursor)
            {
                Some((end, title))
                    if self.source[*end..self.cursor]
                        .trim_end_matches(|c: char| c != '{')
                        .trim_end_matches('{')
                        .trim_end_matches("\\label")
                        .trim()
                        .is_empty() =>
                {
                    format!("sec:{}", slug(title))
                }
                _ => return vec![],
            },
        };
        let taken: HashSet<&str> = self
            .all()
            .flat_map(|i| &i.labels)
            .map(|l| l.item.insert.as_str())
            .collect();
        if taken.contains(suggestion.as_str())
            || !suggestion.starts_with(prefix)
            || suggestion == prefix
        {
            return vec![];
        }
        vec![(0, Item::new(&suggestion, "Suggested label"))]
    }

    fn labels(&self, command: &str, prefix: &str) -> Ranked {
        let expected = match command {
            "eqref" => Some(LabelKind::Equation),
            _ => None,
        };
        let mut ranked = vec![];
        for label in self.all().flat_map(|i| &i.labels) {
            if let Some(mut score) = fuzzy::score(&label.item.insert, prefix) {
                if expected.is_some_and(|kind| kind == label.kind) {
                    score += 300;
                }
                ranked.push((score, &label.item));
            }
        }
        best(ranked)
    }

    fn citations(&self, argument: &Argument, prefix: &str) -> Ranked {
        // Keys already in this citation are not offered again.
        // Few keys, compared only for matches: a list beats hashing every key.
        let present: Vec<&str> = argument
            .text
            .split(',')
            .map(str::trim)
            .filter(|k| *k != prefix)
            .collect();
        let usage = self.counts(|i| &i.cited);
        let words: Vec<String> = prefix.split_whitespace().map(fuzzy::fold).collect();
        let mut ranked = vec![];
        for citation in self
            .project
            .iter()
            .flat_map(|i| &i.citations)
            .chain(&self.active.citations)
        {
            let mut total = 0;
            let mut matched = true;
            for (n, word) in words.iter().enumerate() {
                let by_key = if n == 0 {
                    fuzzy::score(&citation.key, word)
                } else {
                    None
                };
                match by_key {
                    Some(score) if score >= 600 => total += score,
                    _ if citation.details.contains(word.as_str()) => total += 300,
                    Some(score) => total += score,
                    None => {
                        matched = false;
                        break;
                    }
                }
            }
            let key = citation.item.insert.as_str();
            if matched && !present.contains(&key) {
                if !usage.is_empty() {
                    total += usage.get(key).copied().unwrap_or(0).min(10) as i32 * 5;
                }
                ranked.push((total, &citation.item));
            }
        }
        best(ranked)
    }

    fn files(&self, prefix: &str, extensions: &[&str], strip: bool) -> Ranked {
        let mut ranked = vec![];
        for path in self.files {
            let extension = path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
            if !extensions.is_empty()
                && !extension
                    .as_deref()
                    .is_some_and(|e| extensions.contains(&e))
            {
                continue;
            }
            let text = if strip {
                path.rsplit_once('.')
                    .map_or(path.as_str(), |(stem, _)| stem)
            } else {
                path.as_str()
            };
            if let Some(score) = fuzzy::score(text, prefix) {
                ranked.push((score + 100, Item::new(text, "File")));
            }
        }
        ranked
    }
}

impl Item {
    fn with_suffix(mut self, suffix: &str) -> Self {
        self.insert.push_str(suffix);
        self.cursor = self.insert.len();
        self
    }
}

fn order<T: std::ops::Deref<Target = Item>>(a: &(i32, T), b: &(i32, T)) -> std::cmp::Ordering {
    b.0.cmp(&a.0).then_with(|| a.1.label.cmp(&b.1.label))
}

/// The best candidates of a large borrowed set, cloned only once chosen.
fn best(mut ranked: Vec<(i32, &Item)>) -> Ranked {
    let keep = LIMIT * 4;
    if ranked.len() > keep {
        ranked.select_nth_unstable_by(keep, order);
        ranked.truncate(keep);
    }
    ranked
        .into_iter()
        .map(|(score, item)| (score, item.clone()))
        .collect()
}

fn words(list: &[&str], prefix: &str, detail: &str) -> Ranked {
    list.iter()
        .filter_map(|w| fuzzy::score(w, prefix).map(|s| (s, Item::new(w, detail))))
        .collect()
}

/// `Related Work` → `related-work`.
fn slug(title: &str) -> String {
    let mut slug = String::new();
    for c in fuzzy::fold(title).chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_end_matches('-').chars().take(40).collect()
}

#[cfg(test)]
mod tests;

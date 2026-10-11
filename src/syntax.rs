use std::{collections::BTreeSet, ops::Range};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Text,
    Command,
    Comment,
    Math,
    Delimiter,
    Verbatim,
}
#[derive(Debug, Clone)]
pub struct Token {
    pub range: Range<usize>,
    pub kind: TokenKind,
}
/// Lossless UTF-8 lexer. Unknown macros are first-class commands, not errors.
pub fn tokenize(text: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let start = i;
        let ch = text[i..].chars().next().unwrap();
        let kind = match ch {
            '%' => {
                i = text[i..].find('\n').map_or(text.len(), |n| i + n);
                TokenKind::Comment
            }
            '\\' => {
                let verb_env = [
                    "comment",
                    "verbatim",
                    "verbatim*",
                    "lstlisting",
                    "minted",
                    "asy",
                    "asydef",
                    "pycode",
                    "luacode",
                    "luacode*",
                    "sagesilent",
                    "sageblock",
                ]
                .iter()
                .find_map(|env| {
                    let begin = format!("\\begin{{{env}}}");
                    text[i..]
                        .starts_with(&begin)
                        .then(|| (begin, format!("\\end{{{env}}}")))
                });
                if text[i..].starts_with("\\iffalse")
                    && !text[i + 8..].chars().next().is_some_and(command_letter)
                {
                    i += 8;
                    i = false_block_end(text, i);
                    TokenKind::Comment
                } else if let Some((begin, end)) = verb_env {
                    i = text[i + begin.len()..]
                        .find(&end)
                        .map_or(text.len(), |n| i + begin.len() + n + end.len());
                    TokenKind::Verbatim
                } else if text[i..].starts_with("\\verb")
                    && text[i + 5..]
                        .chars()
                        .next()
                        .is_some_and(|c| !c.is_alphabetic())
                {
                    i += 5;
                    if text[i..].starts_with('*') {
                        i += 1;
                    }
                    if let Some(delim) = text[i..].chars().next()
                        && delim != '\n'
                        && delim != '\r'
                    {
                        i += delim.len_utf8();
                        while i < text.len() {
                            let c = text[i..].chars().next().unwrap();
                            if c == '\n' {
                                break;
                            }
                            i += c.len_utf8();
                            if c == delim {
                                break;
                            }
                        }
                    }
                    TokenKind::Verbatim
                } else if text[i..].starts_with("\\(") || text[i..].starts_with("\\[") {
                    let end = if text[i..].starts_with("\\(") {
                        "\\)"
                    } else {
                        "\\]"
                    };
                    i += 2;
                    i = text[i..].find(end).map_or_else(
                        || text[i..].find('\n').map_or(text.len(), |n| i + n),
                        |n| i + n + 2,
                    );
                    TokenKind::Math
                } else {
                    i += 1;
                    if let Some(c) = text[i..].chars().next() {
                        if c.is_alphabetic() || c == '@' {
                            while i < text.len() {
                                let c = text[i..].chars().next().unwrap();
                                if !(c.is_alphabetic() || c == '@') {
                                    break;
                                }
                                i += c.len_utf8();
                            }
                            if text[i..].starts_with('*') {
                                i += 1;
                            }
                        } else {
                            i += c.len_utf8();
                        }
                    }
                    TokenKind::Command
                }
            }
            '$' => {
                let delim = if text[i..].starts_with("$$") {
                    "$$"
                } else {
                    "$"
                };
                i += delim.len();
                let content = i;
                let mut closed = false;
                while i < text.len() {
                    if text[i..].starts_with('\\') {
                        i += 1;
                        if let Some(c) = text[i..].chars().next() {
                            i += c.len_utf8();
                        }
                    } else if text[i..].starts_with(delim) {
                        i += delim.len();
                        closed = true;
                        break;
                    } else {
                        i += text[i..].chars().next().unwrap().len_utf8();
                    }
                }
                if !closed {
                    i = text[content..]
                        .find('\n')
                        .map_or(text.len(), |n| content + n);
                }
                TokenKind::Math
            }
            '{' | '}' | '[' | ']' | '&' | '#' | '_' | '^' => {
                i += ch.len_utf8();
                TokenKind::Delimiter
            }
            _ => {
                i += ch.len_utf8();
                while i < text.len() {
                    let c = text[i..].chars().next().unwrap();
                    if "\\%${}[]&#_^".contains(c) {
                        break;
                    }
                    i += c.len_utf8();
                }
                TokenKind::Text
            }
        };
        tokens.push(Token {
            range: start..i,
            kind,
        });
    }
    tokens
}
fn command_letter(c: char) -> bool {
    c.is_alphabetic() || matches!(c, '_' | ':' | '@')
}
fn false_block_end(text: &str, mut i: usize) -> usize {
    let mut depth = 1usize;
    while i < text.len() {
        let ch = text[i..].chars().next().unwrap();
        i += ch.len_utf8();
        if ch == '%' {
            i = text[i..].find('\n').map_or(text.len(), |n| i + n);
        } else if ch == '\\' {
            let start = i;
            while i < text.len() && text[i..].chars().next().is_some_and(command_letter) {
                i += text[i..].chars().next().unwrap().len_utf8();
            }
            let name = &text[start..i];
            if name == "fi" {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            } else if matches!(
                name,
                "if" | "ifcat"
                    | "ifnum"
                    | "ifdim"
                    | "ifodd"
                    | "ifvmode"
                    | "ifhmode"
                    | "ifmmode"
                    | "ifinner"
                    | "ifvoid"
                    | "ifhbox"
                    | "ifvbox"
                    | "ifx"
                    | "ifeof"
                    | "iftrue"
                    | "iffalse"
                    | "ifcase"
                    | "ifdefined"
                    | "ifcsname"
                    | "iffontchar"
                    | "ifincsname"
            ) {
                depth += 1;
            } else if name.is_empty() && i < text.len() {
                i += text[i..].chars().next().unwrap().len_utf8();
            }
        }
    }
    i
}
#[derive(Debug, Clone)]
pub struct OutlineEntry {
    pub title: String,
    pub line: usize,
    pub level: usize,
}
pub(crate) fn braced(text: &str, mut i: usize) -> Option<String> {
    while text[i..].starts_with(char::is_whitespace) {
        i += text[i..].chars().next()?.len_utf8();
    }
    // Section short titles and bibitem display labels precede the required group.
    if text[i..].starts_with('[') {
        i += 1;
        let mut braces = 0usize;
        while i < text.len() {
            let ch = text[i..].chars().next()?;
            i += ch.len_utf8();
            match ch {
                '\\' => {
                    if let Some(c) = text[i..].chars().next() {
                        i += c.len_utf8();
                    }
                }
                '{' => braces += 1,
                '}' => braces = braces.saturating_sub(1),
                ']' if braces == 0 => break,
                _ => {}
            }
        }
        while text[i..].starts_with(char::is_whitespace) {
            i += text[i..].chars().next()?.len_utf8();
        }
    }
    if !text[i..].starts_with('{') {
        return None;
    }
    i += 1;
    let start = i;
    let mut depth = 1;
    while i < text.len() {
        let ch = text[i..].chars().next()?;
        if ch == '\\' {
            i += 1;
            if let Some(c) = text[i..].chars().next() {
                i += c.len_utf8();
            }
            continue;
        }
        if ch == '{' {
            depth += 1;
        }
        if ch == '}' {
            depth -= 1;
            if depth == 0 {
                return Some(text[start..i].to_string());
            }
        }
        i += ch.len_utf8();
    }
    None
}
pub fn outline(text: &str) -> Vec<OutlineEntry> {
    let mut entries = vec![];
    for t in tokenize(text)
        .iter()
        .filter(|t| t.kind == TokenKind::Command)
    {
        let name = text[t.range.clone()].trim_end_matches('*');
        if let Some(level) = [
            "\\part",
            "\\chapter",
            "\\section",
            "\\subsection",
            "\\subsubsection",
            "\\paragraph",
        ]
        .iter()
        .position(|n| *n == name)
            && let Some(title) = braced(text, t.range.end)
        {
            entries.push(OutlineEntry {
                title,
                line: text[..t.range.start]
                    .bytes()
                    .filter(|b| *b == b'\n')
                    .count()
                    + 1,
                level,
            });
        }
    }
    entries
}
pub const COMMANDS: &[&str] = &[
    "\\begin",
    "\\end",
    "\\documentclass",
    "\\usepackage",
    "\\newcommand",
    "\\renewcommand",
    "\\section",
    "\\subsection",
    "\\subsubsection",
    "\\chapter",
    "\\paragraph",
    "\\label",
    "\\ref",
    "\\eqref",
    "\\pageref",
    "\\cite",
    "\\autocite",
    "\\textbf",
    "\\textit",
    "\\emph",
    "\\footnote",
    "\\item",
    "\\include",
    "\\input",
    "\\includegraphics",
    "\\caption",
    "\\centering",
    "\\frac",
    "\\sqrt",
    "\\sum",
    "\\int",
    "\\alpha",
    "\\beta",
    "\\gamma",
    "\\delta",
    "\\lambda",
    "\\theta",
    "\\pi",
    "\\sigma",
    "\\infty",
    "\\left",
    "\\right",
    "\\mathbf",
    "\\mathrm",
    "\\mathbb",
    "\\bibliography",
    "\\bibliographystyle",
    "\\addbibresource",
    "\\printbibliography",
    "\\title",
    "\\author",
    "\\date",
    "\\maketitle",
    "\\tableofcontents",
];
pub fn completions(text: &str, prefix: &str) -> Vec<String> {
    let mut candidates: BTreeSet<String> = COMMANDS.iter().map(|s| s.to_string()).collect();
    for t in tokenize(text)
        .iter()
        .filter(|t| t.kind == TokenKind::Command)
    {
        let name = &text[t.range.clone()];
        candidates.insert(name.into());
        if ["\\label", "\\bibitem"].contains(&name)
            && let Some(arg) = braced(text, t.range.end)
        {
            candidates.insert(arg);
        }
    }
    candidates
        .into_iter()
        .filter(|c| c.starts_with(prefix))
        .take(40)
        .collect()
}

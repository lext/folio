//! Lossless highlighting for project sources, including incomplete edits.
use std::{ops::Range, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Latex,
    Toml,
    Json,
    Plain,
}

impl Kind {
    pub fn for_path(path: &Path) -> Self {
        match path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "tex" | "bib" | "sty" | "cls" | "bst" | "clo" | "def" | "cfg" | "fd" => Self::Latex,
            "toml" => Self::Toml,
            "json" => Self::Json,
            _ => Self::Plain,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    Text,
    Command,
    Key,
    String,
    Number,
    Keyword,
    Comment,
    Math,
    Delimiter,
    Verbatim,
}

pub struct Token {
    pub range: Range<usize>,
    pub style: Style,
}

pub fn tokenize(kind: Kind, source: &str) -> Vec<Token> {
    match kind {
        Kind::Latex => folio_core::syntax::tokenize(source)
            .into_iter()
            .map(|t| {
                use folio_core::syntax::TokenKind;
                Token {
                    range: t.range,
                    style: match t.kind {
                        TokenKind::Text => Style::Text,
                        TokenKind::Command => Style::Command,
                        TokenKind::Comment => Style::Comment,
                        TokenKind::Math => Style::Math,
                        TokenKind::Delimiter => Style::Delimiter,
                        TokenKind::Verbatim => Style::Verbatim,
                    },
                }
            })
            .collect(),
        Kind::Json | Kind::Toml => configuration(kind, source),
        Kind::Plain => {
            if source.is_empty() {
                vec![]
            } else {
                vec![Token {
                    range: 0..source.len(),
                    style: Style::Text,
                }]
            }
        }
    }
}

/// No validating parse is required: partial strings/values stay editable and
/// single-line strings recover at the next line instead of coloring to EOF.
fn string_end(source: &str, start: usize, multiline: bool) -> usize {
    let bytes = source.as_bytes();
    let quote = bytes[start];
    let triple = multiline && bytes[start..].starts_with(&[quote; 3]);
    let mut i = start + if triple { 3 } else { 1 };
    while i < bytes.len() {
        if bytes[i] == quote {
            if !triple {
                return i + 1;
            }
            let run = bytes[i..].iter().take_while(|&&b| b == quote).count();
            if run >= 3 {
                // TOML allows one or two content quotes before the closing three.
                return i + run.min(5);
            }
            i += run;
        } else if matches!(bytes[i], b'\n' | b'\r') && !triple {
            break;
        } else if bytes[i] == b'\\' && quote == b'"' {
            i += 1;
            if i < bytes.len() && (triple || !matches!(bytes[i], b'\n' | b'\r')) {
                i += source[i..].chars().next().unwrap().len_utf8();
            }
        } else {
            i += source[i..].chars().next().unwrap().len_utf8();
        }
    }
    i
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Context {
    Array,
    Table,
    Header,
}

fn configuration(kind: Kind, source: &str) -> Vec<Token> {
    let toml = kind == Kind::Toml;
    let mut tokens = Vec::new();
    let mut contexts = Vec::new();
    let mut key = true;
    let mut i = 0;
    while i < source.len() {
        let start = i;
        let ch = source[i..].chars().next().unwrap();
        i += ch.len_utf8();
        let style = match ch {
            '\n' | '\r' => {
                // An unfinished table heading must not swallow later assignments.
                if contexts.last() == Some(&Context::Header) {
                    contexts.clear();
                }
                if contexts.is_empty() {
                    key = true;
                }
                Style::Text
            }
            c if c.is_whitespace() => {
                while let Some(c) = source[i..]
                    .chars()
                    .next()
                    .filter(|c| c.is_whitespace() && !matches!(c, '\r' | '\n'))
                {
                    i += c.len_utf8();
                }
                Style::Text
            }
            '#' if toml => {
                i = source[i..]
                    .find(['\r', '\n'])
                    .map_or(source.len(), |n| i + n);
                Style::Comment
            }
            '"' | '\'' if toml || ch == '"' => {
                i = string_end(source, start, toml);
                let is_key = if toml {
                    key
                } else {
                    source[i..].trim_start().starts_with(':')
                };
                if is_key { Style::Key } else { Style::String }
            }
            '[' => {
                let header = toml
                    && key
                    && (contexts.is_empty() || contexts.last() == Some(&Context::Header));
                contexts.push(if header {
                    Context::Header
                } else {
                    Context::Array
                });
                key = header;
                Style::Delimiter
            }
            '{' => {
                contexts.push(Context::Table);
                key = true;
                Style::Delimiter
            }
            ']' | '}' => {
                contexts.pop();
                key = contexts.last() == Some(&Context::Header);
                Style::Delimiter
            }
            '=' if toml => {
                key = false;
                Style::Delimiter
            }
            ':' if !toml => {
                key = false;
                Style::Delimiter
            }
            ',' => {
                key = contexts.last() == Some(&Context::Table);
                Style::Delimiter
            }
            '.' if toml && key => Style::Delimiter,
            _ => {
                while let Some(c) = source[i..].chars().next() {
                    if c.is_whitespace()
                        || "[]{}\",=".contains(c)
                        || (toml && (c == '#' || c == '\'' || (key && c == '.')))
                        || (!toml && c == ':')
                    {
                        break;
                    }
                    i += c.len_utf8();
                }
                let value = &source[start..i];
                if toml && key {
                    Style::Key
                } else if matches!(value, "true" | "false") || (!toml && value == "null") {
                    Style::Keyword
                } else if ch.is_ascii_digit()
                    || matches!(ch, '-' | '+')
                    || (toml && matches!(value, "inf" | "nan"))
                {
                    Style::Number
                } else {
                    Style::Text
                }
            }
        };
        tokens.push(Token {
            range: start..i,
            style,
        });
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    fn styled(kind: Kind, source: &str, text: &str, expected: Style) {
        let start = source.find(text).unwrap();
        let tokens = tokenize(kind, source);
        let token = tokens.iter().find(|t| t.range.contains(&start)).unwrap();
        assert_eq!(token.style, expected, "{text:?} in {source:?}");
        assert!(token.range.end >= start + text.len());
    }

    #[test]
    fn file_types_are_detected_without_affecting_tex_packages() {
        for (path, expected) in [
            ("folio.project.toml", Kind::Toml),
            ("data/RESULTS.JSON", Kind::Json),
            ("main.tex", Kind::Latex),
            ("styles/custom.sty", Kind::Latex),
            ("references.bib", Kind::Latex),
            ("README.md", Kind::Plain),
        ] {
            assert_eq!(Kind::for_path(Path::new(path)), expected, "{path}");
        }
    }

    #[test]
    fn json_distinguishes_keys_escaped_strings_numbers_and_literals() {
        let source =
            r#"{"α": "escaped \" [text] \\ path", "values": [-2.5e+3, true, false, null]}"#;
        for (text, style) in [
            ("\"α\"", Style::Key),
            ("\"values\"", Style::Key),
            (r#""escaped \" [text] \\ path""#, Style::String),
            ("-2.5e+3", Style::Number),
            ("true", Style::Keyword),
            ("false", Style::Keyword),
            ("null", Style::Keyword),
            ("{", Style::Delimiter),
        ] {
            styled(Kind::Json, source, text, style);
        }
        styled(
            Kind::Json,
            "\"top-level string\"",
            "top-level",
            Style::String,
        );
        styled(Kind::Json, "true", "true", Style::Keyword);
        styled(Kind::Json, "[1]", "[", Style::Delimiter);
    }

    #[test]
    fn toml_highlights_tables_dotted_keys_inline_tables_and_multiline_values() {
        let source = concat!(
            "# Project [comment]\n[[build.targets]]\n",
            "site.\"display name\" = 'Folio #1'\n",
            "options = { enabled = true, count = 0xDEAD_beef, scales = [1.5, -2e3] }\n",
            "date = 2026-10-11T12:30:00Z\n",
            "text = \"\"\"multiline\n# still a string [ ]\n\"\"\"\n",
            "literal = '''C:\\[raw]\n# literal'''\nnext = false\n",
        );
        for (text, style) in [
            ("# Project [comment]", Style::Comment),
            ("build", Style::Key),
            ("targets", Style::Key),
            ("site", Style::Key),
            ("\"display name\"", Style::Key),
            ("'Folio #1'", Style::String),
            ("enabled", Style::Key),
            ("count", Style::Key),
            ("scales", Style::Key),
            ("0xDEAD_beef", Style::Number),
            ("1.5", Style::Number),
            ("-2e3", Style::Number),
            ("true", Style::Keyword),
            ("2026-10-11T12:30:00Z", Style::Number),
            ("# still a string [ ]", Style::String),
            ("# literal", Style::String),
            ("next", Style::Key),
            ("false", Style::Keyword),
        ] {
            styled(Kind::Toml, source, text, style);
        }
    }

    #[test]
    fn incomplete_strings_recover_without_swallowing_following_lines() {
        styled(
            Kind::Json,
            "{\"bad\": \"unfinished\n\"next\": true}",
            "\"next\"",
            Style::Key,
        );
        styled(
            Kind::Toml,
            "broken = \"unfinished\nnext = true",
            "next",
            Style::Key,
        );
        styled(
            Kind::Toml,
            "broken = '''unfinished\nnext = true",
            "next",
            Style::String,
        );
        styled(
            Kind::Toml,
            "value = \"\"\"a \"\" quote\"\"\"\nnext = true",
            "next",
            Style::Key,
        );
        styled(
            Kind::Toml,
            "value = \"\"\"one closing quote\"\"\"\"\nnext = true",
            "next",
            Style::Key,
        );
    }

    #[test]
    fn tokenization_is_lossless_at_every_utf8_edit_boundary() {
        for kind in [Kind::Latex, Kind::Toml, Kind::Json, Kind::Plain] {
            for source in [
                "[α.\"β\"]\nvalue = \"\"\"escaped \\\" λ\n#comment\"\"\"\nnext = [true, {id=2}]",
                r#"{"λ": ["\\", "\"", -3.2E+4, null]}"#,
                "\\section{α}\n$x_{i}$ % []\n\\verb|{|",
            ] {
                for end in source.char_indices().map(|(i, _)| i).chain([source.len()]) {
                    let input = &source[..end];
                    let mut offset = 0;
                    for token in tokenize(kind, input) {
                        assert_eq!(token.range.start, offset);
                        assert!(!token.range.is_empty());
                        assert!(input.is_char_boundary(token.range.start));
                        assert!(input.is_char_boundary(token.range.end));
                        offset = token.range.end;
                    }
                    assert_eq!(offset, input.len());
                }
            }
        }
    }
}

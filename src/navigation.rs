//! Word-level refinement of SyncTeX's source-line anchors.
use std::ops::Range;

const CONTEXT: usize = 8;
#[derive(Debug)]
struct Word {
    range: Range<usize>,
    text: String,
}

#[derive(Debug, Clone)]
pub struct WordAnchor {
    pub range: Range<usize>,
    word: String,
    before: Vec<String>,
    after: Vec<String>,
}
impl WordAnchor {
    /// Offsets refer to Unicode scalars, as do PDF glyphs and the editor cursor.
    pub fn at(text: &str, character: usize) -> Option<Self> {
        let chars: Vec<_> = text.chars().collect();
        if !word_character(*chars.get(character)?) {
            return None;
        }
        let words = words(&chars);
        let i = words.iter().position(|w| w.range.contains(&character))?;
        Some(Self {
            range: words[i].range.clone(),
            word: words[i].text.clone(),
            before: words[..i]
                .iter()
                .rev()
                .take(CONTEXT)
                .map(|w| w.text.clone())
                .collect(),
            after: words[i + 1..]
                .iter()
                .take(CONTEXT)
                .map(|w| w.text.clone())
                .collect(),
        })
    }
}

/// Refine within eight lines of SyncTeX, never by a global first-word search.
/// Tied matches are deliberately left unresolved (e.g. generated macro output).
pub fn source_word(source: &str, line: usize, hint: &WordAnchor) -> Option<Range<usize>> {
    let first = line.saturating_sub(CONTEXT).max(1);
    let last = line.saturating_add(CONTEXT);
    let (mut start, mut end, mut base) = (0, 0, 0);
    for (i, row) in source.split_inclusive('\n').enumerate() {
        if i + 1 < first {
            start += row.len();
            base += row.chars().count();
        }
        if i >= last {
            break;
        }
        end += row.len();
    }
    if start >= end {
        return None;
    }
    let chars = source_characters(source, start..end);
    let words = words(&chars);
    let mut row = first;
    let lines: Vec<_> = chars
        .iter()
        .map(|&c| {
            let current = row;
            row += usize::from(c == '\n');
            current
        })
        .collect();
    let mut best = None;
    let mut tied = false;
    for (i, word) in words
        .iter()
        .enumerate()
        .filter(|(_, w)| w.text == hint.word)
    {
        let before = hint
            .before
            .iter()
            .zip(words[..i].iter().rev())
            .take_while(|(expected, actual)| **expected == actual.text)
            .count();
        let after = hint
            .after
            .iter()
            .zip(&words[i + 1..])
            .take_while(|(expected, actual)| **expected == actual.text)
            .count();
        let score = (
            before + after,
            std::cmp::Reverse(lines[word.range.start].abs_diff(line)),
        );
        match best.as_ref().map(|(s, _)| score.cmp(s)) {
            None | Some(std::cmp::Ordering::Greater) => {
                best = Some((score, word.range.clone()));
                tied = false;
            }
            Some(std::cmp::Ordering::Equal) => tied = true,
            _ => {}
        }
    }
    let (_, range) = best.filter(|_| !tied)?;
    Some(base + range.start..base + range.end)
}

fn word_character(c: char) -> bool {
    c.is_alphanumeric() || ('\u{300}'..='\u{36f}').contains(&c)
}
fn words(chars: &[char]) -> Vec<Word> {
    let (mut result, mut i) = (Vec::new(), 0);
    while i < chars.len() {
        if !word_character(chars[i]) {
            i += 1;
            continue;
        }
        let start = i;
        let mut text = String::new();
        while i < chars.len() {
            let c = chars[i];
            if word_character(c) {
                // PDF fonts commonly expose a single glyph for these sequences.
                if let Some(ligature) = match c {
                    'ﬀ' => Some("ff"),
                    'ﬁ' => Some("fi"),
                    'ﬂ' => Some("fl"),
                    'ﬃ' => Some("ffi"),
                    'ﬄ' => Some("ffl"),
                    'ﬅ' | 'ﬆ' => Some("st"),
                    _ => None,
                } {
                    text.push_str(ligature);
                } else {
                    text.extend(c.to_lowercase());
                }
                i += 1;
            } else if matches!(c, '-' | '\u{ad}') {
                let mut next = i + 1;
                while chars.get(next).is_some_and(|c| c.is_whitespace()) {
                    next += 1;
                }
                if chars.get(next).is_some_and(|c| word_character(*c))
                    && (c == '\u{ad}'
                        || chars[i + 1..next].iter().any(|c| matches!(c, '\r' | '\n')))
                {
                    i = next;
                } else if next == i + 1 && chars.get(next).is_some_and(|c| word_character(*c)) {
                    text.push('-');
                    i += 1;
                } else {
                    break;
                }
            } else if matches!(c, '\'' | '’')
                && chars.get(i + 1).is_some_and(|c| word_character(*c))
            {
                text.push('\'');
                i += 1;
            } else {
                break;
            }
        }
        result.push(Word {
            range: start..i,
            text,
        });
    }
    result
}

/// Mask non-rendered syntax without moving a single source character offset.
fn source_characters(text: &str, window: Range<usize>) -> Vec<char> {
    use crate::syntax::{self, TokenKind};
    let mut chars: Vec<_> = text[window.clone()].chars().collect();
    let (mut offset, mut hidden_until) = (0, 0);
    for token in syntax::tokenize(text) {
        let spelling = &text[token.range.clone()];
        if token.kind == TokenKind::Command {
            let name = spelling.trim_start_matches('\\').trim_end_matches('*');
            if matches!(
                name,
                "label"
                    | "ref"
                    | "eqref"
                    | "pageref"
                    | "autoref"
                    | "begin"
                    | "end"
                    | "input"
                    | "include"
                    | "includegraphics"
                    | "bibliography"
                    | "bibliographystyle"
                    | "addbibresource"
                    | "documentclass"
                    | "usepackage"
                    | "bibitem"
                    | "href"
            ) || name.starts_with("cite")
                || name.ends_with("cite")
            {
                hidden_until = hidden_until.max(argument_end(text, token.range.end));
            }
        }
        let start = token.range.start.max(window.start);
        let end = token.range.end.min(window.end);
        if start >= end {
            continue;
        }
        for (byte, c) in text[start..end].char_indices() {
            if c != '\n'
                && (start + byte < hidden_until
                    || matches!(token.kind, TokenKind::Command | TokenKind::Comment)
                    || spelling.starts_with("\\begin{comment}"))
            {
                chars[offset] = ' ';
            }
            offset += 1;
        }
    }
    chars
}
fn argument_end(text: &str, mut i: usize) -> usize {
    while i < text.len() {
        let c = text[i..].chars().next().unwrap();
        if c.is_whitespace() {
            i += c.len_utf8();
            continue;
        }
        let close = match c {
            '[' => ']',
            '{' => '}',
            _ => return i,
        };
        let mut depth = 1;
        i += 1;
        while i < text.len() && depth > 0 {
            let ch = text[i..].chars().next().unwrap();
            i += ch.len_utf8();
            if ch == '\\' {
                if let Some(escaped) = text[i..].chars().next() {
                    i += escaped.len_utf8();
                }
            } else if ch == c {
                depth += 1;
            } else if ch == close {
                depth -= 1;
            }
        }
        if close == '}' {
            break;
        }
    }
    i
}

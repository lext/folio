//! Bracket positions use the same character offsets as egui's text cursor.
use super::highlight::{Kind, Style, Token};
use std::ops::Range;

#[derive(Debug, PartialEq, Eq)]
pub struct Match {
    pub active: Range<usize>,
    pub partner: Option<Range<usize>>,
}

struct Entry {
    range: Range<usize>,
    partner: Option<usize>,
}

#[derive(Default)]
pub struct Brackets {
    entries: Vec<Entry>,
}

impl Brackets {
    pub fn update(&mut self, kind: Kind, source: &str, tokens: &[Token]) {
        self.entries.clear();
        let mut stack = Vec::new();
        let mut token = 0;
        let mut chars = source.char_indices().enumerate().peekable();
        while let Some((position, (byte, mut ch))) = chars.next() {
            while token < tokens.len() && tokens[token].range.end <= byte {
                token += 1;
            }
            let Some(style) = tokens.get(token).map(|t| t.style) else {
                break;
            };
            if matches!(
                style,
                Style::Comment | Style::String | Style::Key | Style::Verbatim | Style::Command
            ) {
                continue;
            }
            let mut escaped = false;
            let mut end = position + 1;
            if kind == Kind::Latex && style == Style::Math {
                if ch == '%' {
                    while chars.peek().is_some_and(|(_, (_, c))| *c != '\n') {
                        chars.next();
                    }
                    continue;
                }
                if ch == '\\' {
                    let Some((_, (_, next))) = chars.next() else {
                        continue;
                    };
                    // \(…\) and \[…\] are structural math delimiters. \{ and
                    // \} are printed characters and must not match TeX groups.
                    if !matches!(next, '(' | ')' | '[' | ']') {
                        continue;
                    }
                    ch = next;
                    escaped = true;
                    end += 1;
                }
            }
            let (opening, closer) = match ch {
                '(' => (true, ')'),
                '[' => (true, ']'),
                '{' => (true, '}'),
                ')' | ']' | '}' => (false, ch),
                _ => continue,
            };
            let index = self.entries.len();
            self.entries.push(Entry {
                range: position..end,
                partner: None,
            });
            if opening {
                stack.push((closer, escaped, index));
            } else if let Some(&(expected, math, other)) = stack.last() {
                if expected == closer && escaped == math {
                    self.entries[index].partner = Some(other);
                    self.entries[other].partner = Some(index);
                    stack.pop();
                } else {
                    // Recover after malformed input without inventing crossing
                    // pairs, such as the braces and brackets in `{[}]`.
                    stack.clear();
                }
            }
        }
    }
    pub fn at(&self, caret: usize) -> Option<Match> {
        let index = self
            .entries
            .partition_point(|e| e.range.start <= caret)
            .checked_sub(1)?;
        let entry = &self.entries[index];
        (caret <= entry.range.end).then(|| Match {
            active: entry.range.clone(),
            partner: entry.partner.map(|i| self.entries[i].range.clone()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::highlight::tokenize;
    use super::*;

    fn index(kind: Kind, source: &str) -> Brackets {
        let mut brackets = Brackets::default();
        brackets.update(kind, source, &tokenize(kind, source));
        brackets
    }

    fn chars_before(source: &str, needle: &str) -> usize {
        source[..source.find(needle).unwrap()].chars().count()
    }

    #[test]
    fn matches_nested_unicode_brackets_on_either_side_of_the_caret() {
        let brackets = index(Kind::Latex, "α {β [γ (δ)]}");
        for (caret, active, other) in [(2, 2, 12), (3, 2, 12), (6, 5, 11), (9, 8, 10), (13, 12, 2)]
        {
            assert_eq!(
                brackets.at(caret),
                Some(Match {
                    active: active..active + 1,
                    partner: Some(other..other + 1)
                })
            );
        }
        assert_eq!(brackets.at(4), None);
        assert_eq!(brackets.at(100), None);
    }

    #[test]
    fn strings_comments_and_raw_tex_do_not_introduce_bracket_pairs() {
        for (kind, source) in [
            (Kind::Json, r#"{"text": "[ \" }", "array": [true]}"#),
            (Kind::Toml, "# }\narray = [\"{\", '''[raw]''']\n"),
            (
                Kind::Latex,
                "α \\{ % }\n\\verb|[| \\begin{verbatim})\\end{verbatim}\\iffalse { \\fi [ok]",
            ),
        ] {
            let brackets = index(kind, source);
            let open = chars_before(
                source,
                if kind == Kind::Json {
                    "[true]"
                } else if kind == Kind::Toml {
                    "[\""
                } else {
                    "[ok]"
                },
            );
            let close = source
                .chars()
                .collect::<Vec<_>>()
                .iter()
                .rposition(|c| *c == ']')
                .unwrap();
            assert_eq!(
                brackets.at(open),
                Some(Match {
                    active: open..open + 1,
                    partner: Some(close..close + 1)
                })
            );
            let ignored = chars_before(
                source,
                if kind == Kind::Json {
                    "[ "
                } else if kind == Kind::Toml {
                    "[raw]"
                } else {
                    "[|"
                },
            );
            assert_eq!(brackets.at(ignored), None, "{source}");
        }
    }

    #[test]
    fn math_groups_and_escaped_math_delimiters_match_without_literal_braces() {
        let source = "α \\(x_{i} + \\{y\\} % }\n\\)";
        let brackets = index(Kind::Latex, source);
        let close = chars_before(source, "\\)");
        assert_eq!(
            brackets.at(2),
            Some(Match {
                active: 2..4,
                partner: Some(close..close + 2)
            })
        );
        let group = chars_before(source, "{i}");
        assert_eq!(
            brackets.at(group),
            Some(Match {
                active: group..group + 1,
                partner: Some(group + 2..group + 3)
            })
        );
        assert_eq!(brackets.at(chars_before(source, "{y")), None);
        assert_eq!(brackets.at(chars_before(source, "}\n")), None);
    }

    #[test]
    fn mismatches_never_cross_and_edits_discard_old_positions() {
        let mut brackets = index(Kind::Json, "{[}]");
        for caret in 0..4 {
            assert_eq!(brackets.at(caret).unwrap().partner, None);
        }
        let source = "\"α\" [2]";
        brackets.update(Kind::Json, source, &tokenize(Kind::Json, source));
        assert_eq!(
            brackets.at(4),
            Some(Match {
                active: 4..5,
                partner: Some(6..7)
            })
        );
        brackets.update(Kind::Json, "", &[]);
        assert_eq!(brackets.at(4), None);
    }
}

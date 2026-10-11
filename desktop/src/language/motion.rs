//! Source motions use cached Unicode character offsets, like egui's caret.
use super::highlight::{Kind, Style, Token};
use std::ops::Range;

#[derive(Clone, Copy, Debug)]
pub enum Motion {
    ParagraphEnd,
    ParagraphStart,
    SentenceEnd,
    SentenceStart,
}

#[derive(Default)]
pub struct Index {
    paragraphs: Vec<Range<usize>>,
    sentences: Vec<Range<usize>>,
    length: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Syntax {
    Text,
    Opaque,
    Comment,
    ParagraphBreak,
}
struct Character {
    ch: char,
    syntax: Syntax,
}
impl Character {
    fn sentence_space(&self) -> bool {
        self.ch.is_whitespace() || self.syntax == Syntax::Comment
    }
    fn terminator(&self) -> bool {
        self.syntax == Syntax::Text && matches!(self.ch, '.' | '!' | '?' | '…' | '。' | '！' | '？')
    }
    fn closing(&self) -> bool {
        self.syntax == Syntax::Text
            && matches!(
                self.ch,
                '"' | '\'' | '”' | '’' | '»' | ')' | ']' | '}' | '」' | '』'
            )
    }
}

impl Index {
    pub fn update(&mut self, source: &str, kind: Kind, tokens: &[Token]) {
        // Reuse the editor's lossless tokens. Punctuation in a comment, macro,
        // formula or verbatim block is not a prose sentence boundary.
        let chars: Vec<_> = tokens
            .iter()
            .flat_map(|token| {
                let text = &source[token.range.clone()];
                let syntax = match token.style {
                    Style::Command if kind == Kind::Latex && text == "\\par" => {
                        Syntax::ParagraphBreak
                    }
                    Style::Comment => Syntax::Comment,
                    Style::Command | Style::Math | Style::Verbatim => Syntax::Opaque,
                    _ => Syntax::Text,
                };
                text.chars().map(move |ch| Character { ch, syntax })
            })
            .collect();
        self.length = chars.len();
        self.paragraphs.clear();
        self.sentences.clear();
        let mut start = None;
        let mut end = 0;
        let mut line_has_content = false;
        for (i, c) in chars.iter().enumerate() {
            if c.syntax == Syntax::ParagraphBreak || (c.ch == '\n' && !line_has_content) {
                if let Some(start) = start.take() {
                    self.paragraphs.push(start..end);
                }
                line_has_content = false;
            } else if c.ch == '\n' {
                line_has_content = false;
            } else if !c.ch.is_whitespace() {
                line_has_content = true;
                start.get_or_insert(i);
                end = i + 1;
            }
        }
        if let Some(start) = start {
            self.paragraphs.push(start..end);
        }
        for paragraph in &self.paragraphs {
            sentences(&chars, paragraph.clone(), &mut self.sentences);
        }
    }
    pub fn destination(&self, caret: usize, motion: Motion) -> usize {
        let caret = caret.min(self.length);
        let ranges = match motion {
            Motion::ParagraphEnd | Motion::ParagraphStart => &self.paragraphs,
            Motion::SentenceEnd | Motion::SentenceStart => &self.sentences,
        };
        match motion {
            Motion::ParagraphEnd | Motion::SentenceEnd => ranges
                .get(ranges.partition_point(|r| r.end <= caret))
                .map_or(self.length, |r| r.end),
            Motion::ParagraphStart | Motion::SentenceStart => ranges
                .partition_point(|r| r.start < caret)
                .checked_sub(1)
                .map_or(0, |i| ranges[i].start),
        }
    }
}

fn sentences(chars: &[Character], paragraph: Range<usize>, output: &mut Vec<Range<usize>>) {
    let mut start = paragraph.start;
    let mut end = paragraph.end;
    while end > start && chars[end - 1].sentence_space() {
        end -= 1;
    }
    while start < end && chars[start].sentence_space() {
        start += 1;
    }
    let mut i = start;
    while i < end {
        if !chars[i].terminator() {
            i += 1;
            continue;
        }
        let mut punctuation_end = i + 1;
        while punctuation_end < end && chars[punctuation_end].terminator() {
            punctuation_end += 1;
        }
        let mut stop = punctuation_end;
        while stop < end && chars[stop].closing() {
            stop += 1;
        }
        let boundary = stop == end
            || chars[stop].sentence_space()
            || chars[i..punctuation_end]
                .iter()
                .any(|c| matches!(c.ch, '。' | '！' | '？'));
        let abbreviation = boundary
            && stop < end
            && chars[i].ch == '.'
            && punctuation_end == i + 1
            && is_abbreviation(chars, start, i, stop);
        if boundary && !abbreviation {
            output.push(start..stop);
            start = stop;
            while start < end && chars[start].sentence_space() {
                start += 1;
            }
            i = start;
        } else {
            i = punctuation_end;
        }
    }
    if start < end {
        output.push(start..end);
    }
}

// Deliberately small prose heuristics, not an NLP parser. Decimal points and
// punctuation within a word already fail the whitespace boundary check above.
fn is_abbreviation(chars: &[Character], sentence_start: usize, dot: usize, next: usize) -> bool {
    let mut start = dot;
    while start > sentence_start
        && chars[start - 1].syntax == Syntax::Text
        && (chars[start - 1].ch.is_alphabetic() || chars[start - 1].ch == '.')
    {
        start -= 1;
    }
    let word: String = chars[start..dot].iter().map(|c| c.ch).collect();
    if matches!(
        word.to_ascii_lowercase().as_str(),
        "dr" | "mr"
            | "mrs"
            | "ms"
            | "prof"
            | "fig"
            | "figs"
            | "eq"
            | "eqs"
            | "sec"
            | "secs"
            | "ch"
            | "no"
            | "vol"
            | "pp"
            | "p"
            | "e.g"
            | "i.e"
            | "vs"
            | "cf"
    ) {
        return true;
    }
    // Dotted initials (U.S.) and a personal initial before a capitalized name.
    if word.contains('.')
        && word
            .split('.')
            .all(|part| part.chars().count() == 1 && part.chars().all(char::is_alphabetic))
    {
        return true;
    }
    word.chars().count() == 1
        && word.chars().all(char::is_uppercase)
        && chars[next..]
            .iter()
            .find(|c| !c.sentence_space())
            .is_some_and(|c| c.ch.is_uppercase())
}

#[cfg(test)]
mod tests {
    use super::super::highlight::tokenize;
    use super::*;

    fn index(source: &str, kind: Kind) -> Index {
        let mut index = Index::default();
        index.update(source, kind, &tokenize(kind, source));
        index
    }
    fn before(source: &str, text: &str) -> usize {
        source[..source.find(text).unwrap()].chars().count()
    }
    fn after(source: &str, text: &str) -> usize {
        before(source, text) + text.chars().count()
    }

    #[test]
    fn paragraph_motions_skip_blank_runs_but_keep_soft_lines_and_crlf() {
        let text = "  α one\r\nsoft line.  \r\n \t\r\n\r\n  β two.\n\nLast";
        let idx = index(text, Kind::Latex);
        let first = after(text, "soft line.");
        let second = after(text, "β two.");
        assert_eq!(idx.destination(4, Motion::ParagraphEnd), first);
        assert_eq!(idx.destination(first, Motion::ParagraphEnd), second);
        assert_eq!(
            idx.destination(second, Motion::ParagraphEnd),
            text.chars().count()
        );
        assert_eq!(
            idx.destination(before(text, "two"), Motion::ParagraphStart),
            before(text, "β")
        );
        assert_eq!(
            idx.destination(before(text, "β"), Motion::ParagraphStart),
            before(text, "α")
        );
        assert_eq!(
            idx.destination(first + 3, Motion::ParagraphStart),
            before(text, "α")
        );
        assert_eq!(idx.destination(0, Motion::ParagraphStart), 0);
    }

    #[test]
    fn explicit_tex_paragraph_breaks_exclude_comments_math_and_verbatim() {
        let text = "First \\verb|\\par| $\\par$ % \\par\nstill first.\\par Second.\\par\nThird.";
        let idx = index(text, Kind::Latex);
        assert_eq!(
            idx.destination(0, Motion::ParagraphEnd),
            after(text, "still first.")
        );
        assert_eq!(
            idx.destination(after(text, "still first."), Motion::ParagraphEnd),
            after(text, "Second.")
        );
        assert_eq!(
            idx.destination(after(text, "Second."), Motion::ParagraphStart),
            before(text, "Second.")
        );
        let plain = index(text, Kind::Plain);
        assert_eq!(
            plain.destination(0, Motion::ParagraphEnd),
            text.chars().count()
        );
    }

    #[test]
    fn sentence_ends_include_quotes_and_repeated_moves_progress() {
        let text = "  α first sentence.\nSecond? \"Third!\"\n\nLast unfinished";
        let idx = index(text, Kind::Latex);
        let mut caret = 0;
        for ending in ["sentence.", "Second?", "\"Third!\"", "unfinished"] {
            caret = idx.destination(caret, Motion::SentenceEnd);
            assert_eq!(caret, after(text, ending));
        }
        let mut caret = idx.destination(before(text, "Third") + 2, Motion::SentenceStart);
        assert_eq!(caret, before(text, "\"Third"));
        caret = idx.destination(caret, Motion::SentenceStart);
        assert_eq!(caret, before(text, "Second"));
        assert_eq!(
            idx.destination(caret, Motion::SentenceStart),
            before(text, "α")
        );
    }

    #[test]
    fn sentence_motions_avoid_decimal_abbreviation_and_tex_punctuation_traps() {
        let text = "Dr. Ada uses e.g. Fig. 3.14 with $x. y?$ and \\verb|a. b?|. % Ignore. This!\nNext \\emph{sentence.}\n\nNo punctuation";
        let idx = index(text, Kind::Latex);
        let end = after(text, "b?|.");
        assert_eq!(idx.destination(0, Motion::SentenceEnd), end);
        assert_eq!(
            idx.destination(end, Motion::SentenceEnd),
            after(text, "sentence.}")
        );
        assert_eq!(
            idx.destination(before(text, "sentence"), Motion::SentenceStart),
            before(text, "Next")
        );
        assert_eq!(
            idx.destination(after(text, "sentence.}"), Motion::SentenceEnd),
            text.chars().count()
        );
    }

    #[test]
    fn multilingual_punctuation_and_unterminated_input_stay_navigable() {
        let text = "你好。再见！\n\nWait... \"Really?!\" Final fragment";
        let idx = index(text, Kind::Plain);
        let mut caret = 0;
        for end in [
            "你好。",
            "再见！",
            "Wait...",
            "\"Really?!\"",
            "Final fragment",
        ] {
            caret = idx.destination(caret, Motion::SentenceEnd);
            assert_eq!(caret, after(text, end));
        }
        for text in [
            "",
            " \n\t\r\n",
            "α $unfinished math\n\nNext.",
            "\\verb|unfinished\nNext.",
        ] {
            let idx = index(text, Kind::Latex);
            let length = text.chars().count();
            for caret in 0..=length + 1 {
                for motion in [
                    Motion::ParagraphEnd,
                    Motion::ParagraphStart,
                    Motion::SentenceEnd,
                    Motion::SentenceStart,
                ] {
                    let target = idx.destination(caret, motion);
                    assert!(target <= length, "{text:?}, {caret}, {motion:?}");
                    if matches!(motion, Motion::ParagraphEnd | Motion::SentenceEnd) {
                        assert!(target >= caret.min(length));
                    } else {
                        assert!(target <= caret.min(length));
                    }
                }
            }
        }
    }
}

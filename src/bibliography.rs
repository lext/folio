//! Small, tolerant BibTeX/BibLaTeX index. TeX remains the bibliography formatter.
use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub struct Entry {
    pub key: String,
    pub kind: String,
    pub title: String,
    pub author: String,
    pub year: String,
}
fn skip(s: &[u8], i: &mut usize) {
    loop {
        while *i < s.len() && s[*i].is_ascii_whitespace() {
            *i += 1;
        }
        if s.get(*i) != Some(&b'%') {
            break;
        }
        while *i < s.len() && s[*i] != b'\n' {
            *i += 1;
        }
    }
}
fn group(s: &str, i: &mut usize, close: u8) -> String {
    let bytes = s.as_bytes();
    *i += 1;
    let start = *i;
    let mut depth = 0usize;
    while *i < bytes.len() {
        let ch = bytes[*i];
        if ch == b'\\' {
            *i = (*i + 2).min(bytes.len());
            continue;
        }
        if ch == close && depth == 0 {
            let result = s[start..*i].to_owned();
            *i += 1;
            return result;
        }
        if ch == b'{' {
            depth += 1;
        }
        if ch == b'}' {
            depth = depth.saturating_sub(1);
        }
        *i += 1;
    }
    s[start..].to_owned()
}
fn value(s: &str, i: &mut usize, close: u8, strings: &HashMap<String, String>) -> String {
    let bytes = s.as_bytes();
    let mut result = String::new();
    loop {
        skip(bytes, i);
        match bytes.get(*i).copied() {
            Some(b'{') => result.push_str(&group(s, i, b'}')),
            Some(b'"') => result.push_str(&group(s, i, b'"')),
            Some(_) => {
                let start = *i;
                while *i < bytes.len() && ![b',', b'#', close, b'\n', b'\r'].contains(&bytes[*i]) {
                    *i += 1;
                }
                let raw = s[start..*i].trim();
                result.push_str(
                    strings
                        .get(&raw.to_ascii_lowercase())
                        .map_or(raw, String::as_str),
                );
            }
            None => break,
        }
        skip(bytes, i);
        if bytes.get(*i) != Some(&b'#') {
            break;
        }
        *i += 1;
    }
    result
}
/// TeX accent commands and special letters as Unicode, so `M{\"u}ller`
/// displays and searches as `Müller`.
fn decode_accents(s: &str) -> String {
    const ACCENTED: &[(char, &str, &str)] = &[
        ('"', "aeiouyAEIOU", "äëïöüÿÄËÏÖÜ"),
        ('\'', "aeiouycnszAEIOUYCNSZ", "áéíóúýćńśźÁÉÍÓÚÝĆŃŚŹ"),
        ('`', "aeiouAEIOU", "àèìòùÀÈÌÒÙ"),
        ('^', "aeiouAEIOU", "âêîôûÂÊÎÔÛ"),
        ('~', "anoANO", "ãñõÃÑÕ"),
        ('=', "aeiouAEIOU", "āēīōūĀĒĪŌŪ"),
        ('.', "zZI", "żŻİ"),
        ('v', "cszrenCSZRENdt", "čšžřěňČŠŽŘĚŇďť"),
        ('c', "csCS", "çşÇŞ"),
        ('H', "ouOU", "őűŐŰ"),
        ('r', "auAU", "åůÅŮ"),
        ('k', "aeAE", "ąęĄĘ"),
        ('u', "agAG", "ăğĂĞ"),
    ];
    const LETTERS: &[(&str, &str)] = &[
        ("ss", "ß"),
        ("ae", "æ"),
        ("AE", "Æ"),
        ("oe", "œ"),
        ("OE", "Œ"),
        ("aa", "å"),
        ("AA", "Å"),
        ("o", "ø"),
        ("O", "Ø"),
        ("l", "ł"),
        ("L", "Ł"),
        ("i", "ı"),
        ("j", "ȷ"),
    ];
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '\\' || i + 1 >= chars.len() {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let accent = chars[i + 1];
        if let Some((_, bases, marked)) = ACCENTED.iter().find(|(a, ..)| *a == accent) {
            // The accented letter: `\"u`, `\"{u}`, `\v c`, `\'{\i}`.
            let mut j = i + 2;
            if accent.is_alphabetic() {
                if chars.get(j) == Some(&' ') {
                    j += 1;
                } else if chars.get(j) != Some(&'{') {
                    out.push('\\');
                    i += 1;
                    continue;
                }
            }
            let braced = chars.get(j) == Some(&'{');
            if braced {
                j += 1;
            }
            if chars.get(j) == Some(&'\\') && matches!(chars.get(j + 1), Some('i' | 'j')) {
                j += 1;
            }
            if let Some(&letter) = chars.get(j)
                && let Some(n) = bases.chars().position(|b| b == letter)
                && (!braced || chars.get(j + 1) == Some(&'}'))
            {
                out.push(marked.chars().nth(n).unwrap());
                i = j + 1 + usize::from(braced);
                continue;
            }
        } else {
            let name: String = chars[i + 1..]
                .iter()
                .take_while(|c| c.is_ascii_alphabetic())
                .collect();
            if let Some((_, letter)) = LETTERS.iter().find(|(n, _)| *n == name) {
                out.push_str(letter);
                i += 1 + name.len();
                if chars.get(i) == Some(&' ') {
                    i += 1;
                }
                continue;
            }
        }
        out.push('\\');
        i += 1;
    }
    out
}
fn display(s: &str) -> String {
    decode_accents(s)
        .replace(['{', '}'], "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
pub fn parse(source: &str) -> Vec<Entry> {
    let s = source.as_bytes();
    let mut i = 0;
    let mut entries = Vec::new();
    let mut strings = HashMap::new();
    while i < s.len() {
        skip(s, &mut i);
        if s.get(i) != Some(&b'@') {
            i += 1;
            continue;
        }
        i += 1;
        let start = i;
        while i < s.len() && s[i].is_ascii_alphabetic() {
            i += 1;
        }
        let kind = source[start..i].to_ascii_lowercase();
        skip(s, &mut i);
        let close = match s.get(i) {
            Some(b'{') => b'}',
            Some(b'(') => b')',
            _ => continue,
        };
        if kind == "comment" || kind == "preamble" {
            group(source, &mut i, close);
            continue;
        }
        i += 1;
        skip(s, &mut i);
        let start = i;
        while i < s.len() && ![b',', b'=', close, b'\n'].contains(&s[i]) {
            i += 1;
        }
        let key = source[start..i].trim().to_owned();
        if kind == "string" {
            if s.get(i) == Some(&b'=') {
                i += 1;
                let text = value(source, &mut i, close, &strings);
                strings.insert(key.to_ascii_lowercase(), text);
            }
            continue;
        }
        if key.is_empty() {
            continue;
        }
        let mut entry = Entry {
            key,
            kind,
            ..Entry::default()
        };
        while i < s.len() && s[i] != close && s[i] != b'@' {
            if s[i] == b',' {
                i += 1;
            }
            skip(s, &mut i);
            if i == s.len() || s[i] == close || s[i] == b'@' {
                break;
            }
            let start = i;
            while i < s.len() && (s[i].is_ascii_alphanumeric() || s[i] == b'_') {
                i += 1;
            }
            let field = source[start..i].to_ascii_lowercase();
            skip(s, &mut i);
            if s.get(i) != Some(&b'=') {
                if i == start {
                    i += source[i..].chars().next().unwrap().len_utf8();
                }
                continue;
            }
            i += 1;
            let val = display(&value(source, &mut i, close, &strings));
            match field.as_str() {
                "title" => entry.title = val,
                "author" => entry.author = val,
                "editor" if entry.author.is_empty() => entry.author = val,
                "year" => entry.year = val,
                "date" if entry.year.is_empty() => entry.year = val.chars().take(4).collect(),
                _ => {}
            }
        }
        entries.push(entry);
    }
    entries
}
pub fn resources(source: &str) -> Vec<String> {
    crate::syntax::tokenize(source)
        .into_iter()
        .filter(|t| t.kind == crate::syntax::TokenKind::Command)
        .filter_map(|t| {
            matches!(
                &source[t.range.clone()],
                "\\bibliography" | "\\addbibresource"
            )
            .then(|| crate::syntax::braced(source, t.range.end))
            .flatten()
        })
        .flat_map(|s| {
            s.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|p| {
                    if p.ends_with(".bib") {
                        p.to_owned()
                    } else {
                        format!("{p}.bib")
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

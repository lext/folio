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
fn display(s: &str) -> String {
    s.replace(['{', '}'], "")
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

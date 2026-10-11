use std::{collections::VecDeque, ops::Range};
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
impl Rect {
    pub fn is_valid(self) -> bool {
        [self.x, self.y, self.width, self.height]
            .iter()
            .all(|v| v.is_finite())
            && self.width > 0.
            && self.height > 0.
    }
    pub fn contains(self, x: f32, y: f32) -> bool {
        self.is_valid()
            && x >= self.x
            && y >= self.y
            && x <= self.x + self.width
            && y <= self.y + self.height
    }
}
#[derive(Debug, Clone)]
pub struct Glyph {
    pub ch: char,
    pub rect: Rect,
}
#[derive(Debug, Clone, Default)]
pub struct PageText {
    pub width: f32,
    pub height: f32,
    pub glyphs: Vec<Glyph>,
}
impl PageText {
    pub fn text(&self) -> String {
        self.glyphs.iter().map(|g| g.ch).collect()
    }
    pub fn selected_text(&self, range: Range<usize>) -> String {
        self.glyphs
            .iter()
            .skip(range.start)
            .take(range.end.saturating_sub(range.start))
            .map(|g| g.ch)
            .collect()
    }
    pub fn hit_test(&self, x: f32, y: f32) -> Option<usize> {
        self.glyphs.iter().position(|g| g.rect.contains(x, y))
    }
    pub fn nearest(&self, x: f32, y: f32) -> Option<usize> {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        self.glyphs
            .iter()
            .enumerate()
            .filter(|(_, g)| g.rect.is_valid())
            .min_by(|(_, a), (_, b)| {
                let d =
                    |r: Rect| (x - r.x - r.width / 2.).powi(2) + (y - r.y - r.height / 2.).powi(2);
                d(a.rect).total_cmp(&d(b.rect))
            })
            .map(|(i, _)| i)
    }
    /// Case-insensitive Unicode search with a mapping back to original characters.
    pub fn find(&self, query: &str) -> Vec<Range<usize>> {
        let (needle, _) = folded(&query.trim().chars().collect::<Vec<_>>(), false);
        if needle.is_empty() {
            return vec![];
        }
        let chars: Vec<_> = self.glyphs.iter().map(|g| g.ch).collect();
        let mut found = Vec::new();
        // Both interpretations preserve real hyphenated words at line wraps.
        for join in [false, true] {
            let (stream, map) = folded(&chars, join);
            for (i, w) in stream.windows(needle.len()).enumerate() {
                if w == needle {
                    found.push(map[i].start..map[i + needle.len() - 1].end);
                }
            }
        }
        found.sort_by_key(|r| (r.start, r.end));
        found.dedup();
        found
    }
}
fn folded(chars: &[char], join: bool) -> (Vec<char>, Vec<Range<usize>>) {
    let (mut stream, mut map, mut i) = (Vec::new(), Vec::new(), 0);
    while i < chars.len() {
        let start = i;
        let ch = chars[i];
        i += 1;
        if matches!(ch, '-' | '\u{ad}') && start > 0 && chars[start - 1].is_alphabetic() {
            let mut next = i;
            while next < chars.len() && chars[next].is_whitespace() {
                next += 1;
            }
            if chars[i..next].iter().any(|c| matches!(c, '\n' | '\r'))
                && chars.get(next).is_some_and(|c| c.is_alphabetic())
            {
                i = next;
                if join || ch == '\u{ad}' {
                    continue;
                }
            }
        }
        if ch.is_whitespace() {
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            stream.push(' ');
            map.push(start..i);
        } else {
            for c in ch.to_lowercase() {
                stream.push(c);
                map.push(start..start + 1);
            }
        }
    }
    (stream, map)
}
#[derive(Debug, Clone, Copy)]
pub struct PageTransform {
    pub origin_x: f32,
    pub origin_y: f32,
    pub display_width: f32,
    pub page_width: f32,
    pub page_height: f32,
}
impl PageTransform {
    pub fn scale(self) -> Option<f32> {
        let scale = self.display_width / self.page_width;
        (self.display_width.is_finite()
            && self.page_width.is_finite()
            && self.page_height.is_finite()
            && self.page_height > 0.
            && self.display_width > 0.
            && self.page_width > 0.
            && scale.is_finite()
            && scale > 0.)
            .then_some(scale)
    }
    pub fn to_page(self, x: f32, y: f32) -> Option<(f32, f32)> {
        let scale = 1. / self.scale()?;
        let (x, y) = ((x - self.origin_x) * scale, (y - self.origin_y) * scale);
        (x.is_finite()
            && y.is_finite()
            && x >= 0.
            && y >= 0.
            && x <= self.page_width
            && y <= self.page_height)
            .then_some((x, y))
    }
    pub fn to_screen(self, x: f32, y: f32) -> Option<(f32, f32)> {
        let scale = self.scale()?;
        let (x, y) = (self.origin_x + x * scale, self.origin_y + y * scale);
        (x.is_finite() && y.is_finite()).then_some((x, y))
    }
}
/// LRU accounting independent of GPU handles. Oversized entries evict themselves.
pub struct CacheBudget {
    limit: usize,
    used: usize,
    entries: VecDeque<(u64, usize)>,
}
impl CacheBudget {
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            used: 0,
            entries: VecDeque::new(),
        }
    }
    pub fn used(&self) -> usize {
        self.used
    }
    pub fn touch(&mut self, key: u64) {
        if let Some(i) = self.entries.iter().position(|e| e.0 == key) {
            let e = self.entries.remove(i).unwrap();
            self.entries.push_back(e);
        }
    }
    pub fn insert(&mut self, key: u64, bytes: usize) -> Vec<u64> {
        self.remove(key);
        self.entries.push_back((key, bytes));
        self.used += bytes;
        let mut evicted = vec![];
        while self.used > self.limit {
            if let Some((k, b)) = self.entries.pop_front() {
                self.used -= b;
                evicted.push(k);
            }
        }
        evicted
    }
    pub fn remove(&mut self, key: u64) {
        if let Some(i) = self.entries.iter().position(|e| e.0 == key) {
            self.used -= self.entries.remove(i).unwrap().1;
        }
    }
}

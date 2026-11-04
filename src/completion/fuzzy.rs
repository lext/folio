//! Matching for completion: exact prefixes rank first, then case-insensitive
//! prefixes, word-boundary abbreviations, substrings and finally in-order
//! subsequences. Accents fold to their base letters on both sides.

/// Lowercase and strip common diacritics, so `Müller`, `muller` and the TeX
/// spelling decoded by the bibliography parser all compare equal.
pub fn fold(text: &str) -> String {
    let mut folded = String::with_capacity(text.len());
    for ch in text.chars() {
        match base(ch) {
            Some(base) => folded.push_str(base),
            None => folded.extend(ch.to_lowercase()),
        }
    }
    folded
}

fn base(ch: char) -> Option<&'static str> {
    Some(match ch {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' | 'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å'
        | 'Ā' | 'Ă' | 'Ą' => "a",
        'æ' | 'Æ' => "ae",
        'ç' | 'ć' | 'č' | 'Ç' | 'Ć' | 'Č' => "c",
        'ď' | 'đ' | 'Ď' | 'Đ' => "d",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ę' | 'ě' | 'È' | 'É' | 'Ê' | 'Ë' | 'Ē' | 'Ę' | 'Ě' => {
            "e"
        }
        'ğ' | 'Ğ' => "g",
        'ì' | 'í' | 'î' | 'ï' | 'ī' | 'ı' | 'Ì' | 'Í' | 'Î' | 'Ï' | 'Ī' | 'İ' => "i",
        'ł' | 'Ł' => "l",
        'ñ' | 'ń' | 'ň' | 'Ñ' | 'Ń' | 'Ň' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' | 'Ō'
        | 'Ő' => "o",
        'œ' | 'Œ' => "oe",
        'ř' | 'Ř' => "r",
        'ś' | 'š' | 'ş' | 'Ś' | 'Š' | 'Ş' => "s",
        'ß' => "ss",
        'ť' | 'ţ' | 'Ť' | 'Ţ' => "t",
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' | 'Ù' | 'Ú' | 'Û' | 'Ü' | 'Ū' | 'Ů' | 'Ű' => {
            "u"
        }
        'ý' | 'ÿ' | 'Ý' => "y",
        'ź' | 'ż' | 'ž' | 'Ź' | 'Ż' | 'Ž' => "z",
        _ => return None,
    })
}

fn boundary(previous: Option<char>, ch: char) -> bool {
    match previous {
        None => true,
        Some(p) => {
            !p.is_alphanumeric()
                || (p.is_lowercase() && ch.is_uppercase())
                || (p.is_alphabetic() && ch.is_numeric())
        }
    }
}

/// Score `candidate` against `query`; `None` means no match. Higher is better.
pub fn score(candidate: &str, query: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let length_penalty = candidate.chars().count().min(60) as i32;
    if candidate.starts_with(query) {
        return Some(1000 - length_penalty);
    }
    if candidate.is_ascii() && query.is_ascii() {
        // Most keys and labels are ASCII: decide the common cases without
        // allocating.
        let (c, q) = (candidate.as_bytes(), query.as_bytes());
        if c.len() >= q.len() && c[..q.len()].eq_ignore_ascii_case(q) {
            return Some(900 - length_penalty);
        }
        let mut rest = c.iter();
        if !q
            .iter()
            .all(|q| rest.by_ref().any(|c| c.eq_ignore_ascii_case(q)))
        {
            return None;
        }
    }
    let folded = fold(candidate);
    let query_folded = fold(query);
    if folded.starts_with(&query_folded) {
        return Some(900 - length_penalty);
    }
    // Cheap rejection before any per-character bookkeeping: every query
    // letter must appear in order.
    let mut rest = folded.chars();
    if !query_folded.chars().all(|q| rest.by_ref().any(|c| c == q)) {
        return None;
    }
    // A substring that starts on a word boundary reads like a prefix of a word.
    if let Some(at) = folded.find(&query_folded) {
        let index = folded[..at].chars().count();
        let previous = folded[..at].chars().next_back();
        let current = folded[at..].chars().next().unwrap_or(' ');
        // Case changes mark boundaries too (`camelCase`); read them from the
        // original when folding kept positions.
        let original = (candidate.chars().count() == folded.chars().count())
            .then(|| {
                let mut chars = candidate.chars().skip(index.saturating_sub(1));
                let previous = (index > 0).then(|| chars.next()).flatten();
                chars.next().map(|current| boundary(previous, current))
            })
            .flatten();
        let bonus = if original.unwrap_or_else(|| boundary(previous, current)) {
            150
        } else {
            0
        };
        return Some(600 + bonus - length_penalty - index.min(40) as i32);
    }
    let query: Vec<char> = query_folded.chars().collect();
    if query.len() < 2 {
        return None;
    }
    // In-order subsequence, streamed without allocating; the first letter
    // must start a word. Boundaries come from the original spelling when
    // folding kept one character per character (`ß` → `ss` does not).
    let same = candidate.chars().count() == folded.chars().count();
    let originals: Box<dyn Iterator<Item = char>> = if same {
        Box::new(candidate.chars())
    } else {
        Box::new(folded.chars())
    };
    let mut total = 0;
    let mut wanted = query.iter().peekable();
    let mut previous_original = None;
    let mut last_match: Option<usize> = None;
    let mut gap = 0;
    for (i, (original, lower)) in originals.zip(folded.chars()).enumerate() {
        let Some(&&q) = wanted.peek() else {
            break;
        };
        let at_boundary = boundary(previous_original, original);
        previous_original = Some(original);
        if lower == q && (last_match.is_some() || at_boundary) {
            total += 10 - gap.min(10);
            if at_boundary {
                total += 12;
            }
            if last_match.is_some_and(|p| p + 1 == i) {
                total += 8;
            }
            last_match = Some(i);
            gap = 0;
            wanted.next();
        } else if last_match.is_some() {
            gap += 1;
        }
    }
    if wanted.peek().is_some() {
        return None;
    }
    Some(200 + total - length_penalty)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prefixes_beat_abbreviations_and_accents_fold() {
        assert!(score("section", "sec") > score("subsection", "sec"));
        assert!(score("subsection", "sbsec").is_some());
        assert!(score("fig:results-main", "results").is_some());
        assert!(score("fig:results-main", "rm").is_some());
        assert!(score("Section", "sec").unwrap() < score("section", "sec").unwrap());
        assert_eq!(fold("Müller Ærø"), "muller aero");
        assert!(score("Müller", "mul").is_some());
        assert!(score("alpha", "x").is_none());
        assert!(score("delta", "dl").is_some());
        assert!(
            score("delta", "tl").is_none(),
            "a subsequence must start a word"
        );
        assert!(
            score("varepsilon", "eps").is_some(),
            "substrings match inside words"
        );
    }
}

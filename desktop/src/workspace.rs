#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Source(usize),
    Preview,
}
impl Surface {
    pub fn cycle(self, sources: usize, backwards: bool) -> Option<Self> {
        if sources == 0 {
            return None;
        }
        let index = match self {
            Self::Source(i) => i.min(sources - 1),
            Self::Preview => sources,
        };
        let next = if backwards {
            (index + sources) % (sources + 1)
        } else {
            (index + 1) % (sources + 1)
        };
        Some(if next == sources {
            Self::Preview
        } else {
            Self::Source(next)
        })
    }
}

/// Whitespace-separated terms may match either a title or its path/description.
pub fn matches(query: &str, title: &str, detail: &str) -> bool {
    let haystack = format!("{title} {detail}").to_lowercase();
    query
        .split_whitespace()
        .all(|term| haystack.contains(&term.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cycling_visits_open_buffers_and_pdf_in_both_directions() {
        let mut surface = Surface::Source(0);
        for expected in [Surface::Source(1), Surface::Preview, Surface::Source(0)] {
            surface = surface.cycle(2, false).unwrap();
            assert_eq!(surface, expected);
        }
        for expected in [Surface::Preview, Surface::Source(1), Surface::Source(0)] {
            surface = surface.cycle(2, true).unwrap();
            assert_eq!(surface, expected);
        }
        assert_eq!(Surface::Source(0).cycle(0, false), None);
        assert_eq!(Surface::Source(0).cycle(1, false), Some(Surface::Preview));
        assert_eq!(Surface::Preview.cycle(1, false), Some(Surface::Source(0)));
    }
    #[test]
    fn launcher_matches_all_terms_across_names_and_paths() {
        assert!(matches("", "chapter.tex", "Open · chapters/chapter.tex"));
        assert!(matches(
            "CHAPTER tex",
            "chapter.tex",
            "Open · chapters/chapter.tex"
        ));
        assert!(matches(
            "résumé open",
            "résumé.tex",
            "Open · chapters/résumé.tex"
        ));
        assert!(!matches(
            "chapter bib",
            "chapter.tex",
            "Open · chapters/chapter.tex"
        ));
        assert!(matches("pdf", "Preview", "main.pdf"));
    }
}

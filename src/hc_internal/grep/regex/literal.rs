use memchr::memmem::Finder;

use super::super::matcher::Match;

#[derive(Clone, Debug)]
pub(super) struct FastLine {
    finder: Finder<'static>,
}

impl FastLine {
    pub(super) fn from_literals(literals: &[Vec<u8>]) -> Option<FastLine> {
        match literals {
            [one] if !one.is_empty() => Some(FastLine {
                finder: Finder::new(one).into_owned(),
            }),
            _ => None,
        }
    }

    pub(super) fn needle(&self) -> &[u8] {
        self.finder.needle()
    }

    #[inline]
    pub(super) fn find_at(&self, haystack: &[u8], at: usize) -> Option<Match> {
        let start = at + self.finder.find(haystack.get(at..)?)?;
        Some(Match::new(start, start + self.finder.needle().len()))
    }
}

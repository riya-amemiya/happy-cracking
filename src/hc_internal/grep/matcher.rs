use std::fmt;

use memchr::memchr;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Match {
    start: usize,
    end: usize,
}

impl Match {
    #[inline]
    #[must_use]
    pub fn new(start: usize, end: usize) -> Match {
        assert!(start <= end);
        Match { start, end }
    }

    #[inline]
    #[must_use]
    pub fn zero(offset: usize) -> Match {
        Match {
            start: offset,
            end: offset,
        }
    }

    #[inline]
    #[must_use]
    pub fn start(&self) -> usize {
        self.start
    }

    #[inline]
    #[must_use]
    pub fn end(&self) -> usize {
        self.end
    }

    #[inline]
    #[must_use]
    pub fn with_start(&self, start: usize) -> Match {
        assert!(start <= self.end, "{} is not <= {}", start, self.end);
        Match { start, ..*self }
    }

    #[inline]
    #[must_use]
    pub fn with_end(&self, end: usize) -> Match {
        assert!(self.start <= end, "{} is not <= {}", self.start, end);
        Match { end, ..*self }
    }

    #[inline]
    #[must_use]
    pub fn offset(&self, amount: usize) -> Match {
        Match {
            start: self.start.checked_add(amount).unwrap(),
            end: self.end.checked_add(amount).unwrap(),
        }
    }

    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    #[must_use]
    pub fn range(&self) -> std::ops::Range<usize> {
        self.start..self.end
    }
}

impl std::ops::Index<Match> for [u8] {
    type Output = [u8];

    #[inline]
    fn index(&self, index: Match) -> &[u8] {
        &self[index.start..index.end]
    }
}

impl std::ops::IndexMut<Match> for [u8] {
    #[inline]
    fn index_mut(&mut self, index: Match) -> &mut [u8] {
        &mut self[index.start..index.end]
    }
}

impl std::ops::Index<Match> for str {
    type Output = str;

    #[inline]
    fn index(&self, index: Match) -> &str {
        &self[index.start..index.end]
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LineTerminator(LineTerminatorImp);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum LineTerminatorImp {
    Byte(u8),
    Crlf,
}

impl LineTerminator {
    #[inline]
    #[must_use]
    pub fn byte(byte: u8) -> LineTerminator {
        LineTerminator(LineTerminatorImp::Byte(byte))
    }

    #[inline]
    #[must_use]
    pub fn crlf() -> LineTerminator {
        LineTerminator(LineTerminatorImp::Crlf)
    }

    #[inline]
    #[must_use]
    pub fn is_crlf(self) -> bool {
        self.0 == LineTerminatorImp::Crlf
    }

    #[inline]
    #[must_use]
    pub fn as_byte(self) -> u8 {
        match self.0 {
            LineTerminatorImp::Byte(byte) => byte,
            LineTerminatorImp::Crlf => b'\n',
        }
    }

    #[inline]
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        match self.0 {
            LineTerminatorImp::Byte(ref byte) => std::slice::from_ref(byte),
            LineTerminatorImp::Crlf => b"\r\n",
        }
    }

    #[inline]
    #[must_use]
    pub fn is_suffix(self, slice: &[u8]) -> bool {
        slice.last() == Some(&self.as_byte())
    }
}

impl Default for LineTerminator {
    #[inline]
    fn default() -> LineTerminator {
        LineTerminator::byte(b'\n')
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ByteSet([u64; 4]);

impl fmt::Debug for ByteSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set()
            .entries((0..=255u8).filter(|&b| self.contains(b)))
            .finish()
    }
}

impl ByteSet {
    #[cfg(test)]
    #[inline]
    #[must_use]
    pub fn empty() -> ByteSet {
        ByteSet([0; 4])
    }

    #[inline]
    #[must_use]
    pub fn full() -> ByteSet {
        ByteSet([u64::MAX; 4])
    }

    #[cfg(test)]
    #[inline]
    pub fn add(&mut self, byte: u8) {
        self.0[usize::from(byte / 64)] |= 1 << (byte % 64);
    }

    #[cfg(test)]
    #[inline]
    pub fn add_all(&mut self, start: u8, end: u8) {
        for b in start..=end {
            self.add(b);
        }
    }

    #[inline]
    pub fn remove(&mut self, byte: u8) {
        self.0[usize::from(byte / 64)] &= !(1 << (byte % 64));
    }

    #[inline]
    pub fn remove_all(&mut self, start: u8, end: u8) {
        for b in start..=end {
            self.remove(b);
        }
    }

    #[inline]
    #[must_use]
    pub fn contains(&self, byte: u8) -> bool {
        self.0[usize::from(byte / 64)] & (1 << (byte % 64)) != 0
    }
}

pub trait Captures {
    fn get(&self, i: usize) -> Option<Match>;

    #[inline]
    fn interpolate<F>(
        &self,
        name_to_index: F,
        haystack: &[u8],
        replacement: &[u8],
        dst: &mut Vec<u8>,
    ) where
        F: FnMut(&str) -> Option<usize>,
    {
        interpolate(
            replacement,
            |i, dst| {
                if let Some(range) = self.get(i) {
                    dst.extend_from_slice(&haystack[range]);
                }
            },
            name_to_index,
            dst,
        );
    }
}

#[derive(Clone, Debug, Default)]
pub struct NoCaptures(());

impl NoCaptures {
    #[inline]
    #[must_use]
    pub fn new() -> NoCaptures {
        NoCaptures(())
    }
}

impl Captures for NoCaptures {
    #[inline]
    fn get(&self, _: usize) -> Option<Match> {
        None
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct NoError(());

impl std::error::Error for NoError {}

impl fmt::Display for NoError {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        panic!("BUG for NoError: an impossible error occurred")
    }
}

impl From<NoError> for std::io::Error {
    fn from(_: NoError) -> std::io::Error {
        panic!("BUG for NoError: an impossible error occurred")
    }
}

#[derive(Clone, Copy, Debug)]
pub enum LineMatchKind {
    Confirmed(usize),
    #[cfg(test)]
    Candidate(usize),
}

pub trait ParallelMatcher: Sync {
    fn par_candidate_line(&self, haystack: &[u8]) -> Result<Option<LineMatchKind>, String>;

    fn par_is_match(&self, haystack: &[u8]) -> Result<bool, String>;

    fn par_find_at(&self, haystack: &[u8], at: usize) -> Result<Option<Match>, String>;

    fn par_fork(&self) -> Option<Box<dyn ParallelMatcher + '_>> {
        None
    }

    fn par_literal(&self) -> Option<&[u8]> {
        None
    }
}

pub trait Matcher {
    type Captures: Captures;
    type Error: fmt::Display;

    fn find_at(&self, haystack: &[u8], at: usize) -> Result<Option<Match>, Self::Error>;

    fn new_captures(&self) -> Result<Self::Captures, Self::Error>;

    #[cfg(test)]
    #[inline]
    fn capture_count(&self) -> usize {
        0
    }

    #[inline]
    fn capture_index(&self, _name: &str) -> Option<usize> {
        None
    }

    #[inline]
    fn find(&self, haystack: &[u8]) -> Result<Option<Match>, Self::Error> {
        self.find_at(haystack, 0)
    }

    #[cfg(test)]
    #[inline]
    fn find_iter<F>(&self, haystack: &[u8], matched: F) -> Result<(), Self::Error>
    where
        F: FnMut(Match) -> bool,
    {
        self.find_iter_at(haystack, 0, matched)
    }

    #[inline]
    fn find_iter_at<F>(&self, haystack: &[u8], at: usize, mut matched: F) -> Result<(), Self::Error>
    where
        F: FnMut(Match) -> bool,
    {
        self.try_find_iter_at(haystack, at, |m| Ok::<bool, ()>(matched(m)))
            .map(|r| r.unwrap_or(()))
    }

    #[cfg(test)]
    #[inline]
    fn try_find_iter<F, E>(&self, haystack: &[u8], matched: F) -> Result<Result<(), E>, Self::Error>
    where
        F: FnMut(Match) -> Result<bool, E>,
    {
        self.try_find_iter_at(haystack, 0, matched)
    }

    #[inline]
    fn try_find_iter_at<F, E>(
        &self,
        haystack: &[u8],
        at: usize,
        mut matched: F,
    ) -> Result<Result<(), E>, Self::Error>
    where
        F: FnMut(Match) -> Result<bool, E>,
    {
        let mut last_end = at;
        let mut last_match = None;
        loop {
            if last_end > haystack.len() {
                return Ok(Ok(()));
            }
            let Some(m) = self.find_at(haystack, last_end)? else {
                return Ok(Ok(()));
            };
            if m.is_empty() {
                last_end = m.end + 1;
                if Some(m.end) == last_match {
                    continue;
                }
            } else {
                last_end = m.end;
            }
            last_match = Some(m.end);
            match matched(m) {
                Ok(true) => {}
                Ok(false) => return Ok(Ok(())),
                Err(err) => return Ok(Err(err)),
            }
        }
    }

    #[cfg(test)]
    #[inline]
    fn captures(&self, haystack: &[u8], caps: &mut Self::Captures) -> Result<bool, Self::Error> {
        self.captures_at(haystack, 0, caps)
    }

    #[cfg(test)]
    #[inline]
    fn captures_iter<F>(
        &self,
        haystack: &[u8],
        caps: &mut Self::Captures,
        matched: F,
    ) -> Result<(), Self::Error>
    where
        F: FnMut(&Self::Captures) -> bool,
    {
        self.captures_iter_at(haystack, 0, caps, matched)
    }

    #[inline]
    fn captures_iter_at<F>(
        &self,
        haystack: &[u8],
        at: usize,
        caps: &mut Self::Captures,
        mut matched: F,
    ) -> Result<(), Self::Error>
    where
        F: FnMut(&Self::Captures) -> bool,
    {
        self.try_captures_iter_at(haystack, at, caps, |caps| Ok::<bool, ()>(matched(caps)))
            .map(|r| r.unwrap_or(()))
    }

    #[cfg(test)]
    #[inline]
    fn try_captures_iter<F, E>(
        &self,
        haystack: &[u8],
        caps: &mut Self::Captures,
        matched: F,
    ) -> Result<Result<(), E>, Self::Error>
    where
        F: FnMut(&Self::Captures) -> Result<bool, E>,
    {
        self.try_captures_iter_at(haystack, 0, caps, matched)
    }

    #[inline]
    fn try_captures_iter_at<F, E>(
        &self,
        haystack: &[u8],
        at: usize,
        caps: &mut Self::Captures,
        mut matched: F,
    ) -> Result<Result<(), E>, Self::Error>
    where
        F: FnMut(&Self::Captures) -> Result<bool, E>,
    {
        let mut last_end = at;
        let mut last_match = None;
        loop {
            if last_end > haystack.len() {
                return Ok(Ok(()));
            }
            if !self.captures_at(haystack, last_end, caps)? {
                return Ok(Ok(()));
            }
            let m = caps.get(0).unwrap();
            if m.is_empty() {
                last_end = m.end + 1;
                if Some(m.end) == last_match {
                    continue;
                }
            } else {
                last_end = m.end;
            }
            last_match = Some(m.end);
            match matched(caps) {
                Ok(true) => {}
                Ok(false) => return Ok(Ok(())),
                Err(err) => return Ok(Err(err)),
            }
        }
    }

    #[inline]
    fn captures_at(
        &self,
        _haystack: &[u8],
        _at: usize,
        _caps: &mut Self::Captures,
    ) -> Result<bool, Self::Error> {
        Ok(false)
    }

    #[cfg(test)]
    #[inline]
    fn replace<F>(
        &self,
        haystack: &[u8],
        dst: &mut Vec<u8>,
        mut append: F,
    ) -> Result<(), Self::Error>
    where
        F: FnMut(Match, &mut Vec<u8>) -> bool,
    {
        let mut last_match = 0;
        self.find_iter(haystack, |m| {
            dst.extend_from_slice(&haystack[last_match..m.start]);
            last_match = m.end;
            append(m, dst)
        })?;
        dst.extend_from_slice(&haystack[last_match..]);
        Ok(())
    }

    #[cfg(test)]
    #[inline]
    fn replace_with_captures<F>(
        &self,
        haystack: &[u8],
        caps: &mut Self::Captures,
        dst: &mut Vec<u8>,
        append: F,
    ) -> Result<(), Self::Error>
    where
        F: FnMut(&Self::Captures, &mut Vec<u8>) -> bool,
    {
        self.replace_with_captures_at(haystack, 0, caps, dst, append)
    }

    #[cfg(test)]
    #[inline]
    fn replace_with_captures_at<F>(
        &self,
        haystack: &[u8],
        at: usize,
        caps: &mut Self::Captures,
        dst: &mut Vec<u8>,
        mut append: F,
    ) -> Result<(), Self::Error>
    where
        F: FnMut(&Self::Captures, &mut Vec<u8>) -> bool,
    {
        let mut last_match = at;
        self.captures_iter_at(haystack, at, caps, |caps| {
            let m = caps.get(0).unwrap();
            dst.extend_from_slice(&haystack[last_match..m.start]);
            last_match = m.end;
            append(caps, dst)
        })?;
        dst.extend_from_slice(&haystack[last_match..]);
        Ok(())
    }

    #[inline]
    fn is_match(&self, haystack: &[u8]) -> Result<bool, Self::Error> {
        self.is_match_at(haystack, 0)
    }

    #[inline]
    fn is_match_at(&self, haystack: &[u8], at: usize) -> Result<bool, Self::Error> {
        Ok(self.shortest_match_at(haystack, at)?.is_some())
    }

    #[inline]
    fn shortest_match(&self, haystack: &[u8]) -> Result<Option<usize>, Self::Error> {
        self.shortest_match_at(haystack, 0)
    }

    #[inline]
    fn shortest_match_at(&self, haystack: &[u8], at: usize) -> Result<Option<usize>, Self::Error> {
        Ok(self.find_at(haystack, at)?.map(|m| m.end))
    }

    #[inline]
    fn non_matching_bytes(&self) -> Option<&ByteSet> {
        None
    }

    #[inline]
    fn line_terminator(&self) -> Option<LineTerminator> {
        None
    }

    #[inline]
    fn find_candidate_line(&self, haystack: &[u8]) -> Result<Option<LineMatchKind>, Self::Error> {
        Ok(self.shortest_match(haystack)?.map(LineMatchKind::Confirmed))
    }

    #[inline]
    fn parallel(&self) -> Option<&dyn ParallelMatcher> {
        None
    }
}

impl<M: Matcher + ?Sized> Matcher for &M {
    #[inline]
    fn parallel(&self) -> Option<&dyn ParallelMatcher> {
        (**self).parallel()
    }

    type Captures = M::Captures;
    type Error = M::Error;

    #[inline]
    fn find_at(&self, haystack: &[u8], at: usize) -> Result<Option<Match>, Self::Error> {
        (**self).find_at(haystack, at)
    }

    #[inline]
    fn new_captures(&self) -> Result<Self::Captures, Self::Error> {
        (**self).new_captures()
    }

    #[inline]
    fn captures_at(
        &self,
        haystack: &[u8],
        at: usize,
        caps: &mut Self::Captures,
    ) -> Result<bool, Self::Error> {
        (**self).captures_at(haystack, at, caps)
    }

    #[inline]
    fn capture_index(&self, name: &str) -> Option<usize> {
        (**self).capture_index(name)
    }

    #[cfg(test)]
    #[inline]
    fn capture_count(&self) -> usize {
        (**self).capture_count()
    }

    #[inline]
    fn find(&self, haystack: &[u8]) -> Result<Option<Match>, Self::Error> {
        (**self).find(haystack)
    }

    #[cfg(test)]
    #[inline]
    fn find_iter<F>(&self, haystack: &[u8], matched: F) -> Result<(), Self::Error>
    where
        F: FnMut(Match) -> bool,
    {
        (**self).find_iter(haystack, matched)
    }

    #[inline]
    fn find_iter_at<F>(&self, haystack: &[u8], at: usize, matched: F) -> Result<(), Self::Error>
    where
        F: FnMut(Match) -> bool,
    {
        (**self).find_iter_at(haystack, at, matched)
    }

    #[cfg(test)]
    #[inline]
    fn try_find_iter<F, E>(&self, haystack: &[u8], matched: F) -> Result<Result<(), E>, Self::Error>
    where
        F: FnMut(Match) -> Result<bool, E>,
    {
        (**self).try_find_iter(haystack, matched)
    }

    #[inline]
    fn try_find_iter_at<F, E>(
        &self,
        haystack: &[u8],
        at: usize,
        matched: F,
    ) -> Result<Result<(), E>, Self::Error>
    where
        F: FnMut(Match) -> Result<bool, E>,
    {
        (**self).try_find_iter_at(haystack, at, matched)
    }

    #[cfg(test)]
    #[inline]
    fn captures(&self, haystack: &[u8], caps: &mut Self::Captures) -> Result<bool, Self::Error> {
        (**self).captures(haystack, caps)
    }

    #[cfg(test)]
    #[inline]
    fn captures_iter<F>(
        &self,
        haystack: &[u8],
        caps: &mut Self::Captures,
        matched: F,
    ) -> Result<(), Self::Error>
    where
        F: FnMut(&Self::Captures) -> bool,
    {
        (**self).captures_iter(haystack, caps, matched)
    }

    #[inline]
    fn captures_iter_at<F>(
        &self,
        haystack: &[u8],
        at: usize,
        caps: &mut Self::Captures,
        matched: F,
    ) -> Result<(), Self::Error>
    where
        F: FnMut(&Self::Captures) -> bool,
    {
        (**self).captures_iter_at(haystack, at, caps, matched)
    }

    #[cfg(test)]
    #[inline]
    fn try_captures_iter<F, E>(
        &self,
        haystack: &[u8],
        caps: &mut Self::Captures,
        matched: F,
    ) -> Result<Result<(), E>, Self::Error>
    where
        F: FnMut(&Self::Captures) -> Result<bool, E>,
    {
        (**self).try_captures_iter(haystack, caps, matched)
    }

    #[inline]
    fn try_captures_iter_at<F, E>(
        &self,
        haystack: &[u8],
        at: usize,
        caps: &mut Self::Captures,
        matched: F,
    ) -> Result<Result<(), E>, Self::Error>
    where
        F: FnMut(&Self::Captures) -> Result<bool, E>,
    {
        (**self).try_captures_iter_at(haystack, at, caps, matched)
    }

    #[cfg(test)]
    #[inline]
    fn replace<F>(&self, haystack: &[u8], dst: &mut Vec<u8>, append: F) -> Result<(), Self::Error>
    where
        F: FnMut(Match, &mut Vec<u8>) -> bool,
    {
        (**self).replace(haystack, dst, append)
    }

    #[cfg(test)]
    #[inline]
    fn replace_with_captures<F>(
        &self,
        haystack: &[u8],
        caps: &mut Self::Captures,
        dst: &mut Vec<u8>,
        append: F,
    ) -> Result<(), Self::Error>
    where
        F: FnMut(&Self::Captures, &mut Vec<u8>) -> bool,
    {
        (**self).replace_with_captures(haystack, caps, dst, append)
    }

    #[cfg(test)]
    #[inline]
    fn replace_with_captures_at<F>(
        &self,
        haystack: &[u8],
        at: usize,
        caps: &mut Self::Captures,
        dst: &mut Vec<u8>,
        append: F,
    ) -> Result<(), Self::Error>
    where
        F: FnMut(&Self::Captures, &mut Vec<u8>) -> bool,
    {
        (**self).replace_with_captures_at(haystack, at, caps, dst, append)
    }

    #[inline]
    fn is_match(&self, haystack: &[u8]) -> Result<bool, Self::Error> {
        (**self).is_match(haystack)
    }

    #[inline]
    fn is_match_at(&self, haystack: &[u8], at: usize) -> Result<bool, Self::Error> {
        (**self).is_match_at(haystack, at)
    }

    #[inline]
    fn shortest_match(&self, haystack: &[u8]) -> Result<Option<usize>, Self::Error> {
        (**self).shortest_match(haystack)
    }

    #[inline]
    fn shortest_match_at(&self, haystack: &[u8], at: usize) -> Result<Option<usize>, Self::Error> {
        (**self).shortest_match_at(haystack, at)
    }

    #[inline]
    fn non_matching_bytes(&self) -> Option<&ByteSet> {
        (**self).non_matching_bytes()
    }

    #[inline]
    fn line_terminator(&self) -> Option<LineTerminator> {
        (**self).line_terminator()
    }

    #[inline]
    fn find_candidate_line(&self, haystack: &[u8]) -> Result<Option<LineMatchKind>, Self::Error> {
        (**self).find_candidate_line(haystack)
    }
}

#[inline]
pub fn interpolate<A, N>(
    mut replacement: &[u8],
    mut append: A,
    mut name_to_index: N,
    dst: &mut Vec<u8>,
) where
    A: FnMut(usize, &mut Vec<u8>),
    N: FnMut(&str) -> Option<usize>,
{
    while !replacement.is_empty() {
        let Some(i) = memchr(b'$', replacement) else {
            break;
        };
        dst.extend_from_slice(&replacement[..i]);
        replacement = &replacement[i..];
        if replacement.get(1) == Some(&b'$') {
            dst.push(b'$');
            replacement = &replacement[2..];
            continue;
        }
        let Some(cap_ref) = find_cap_ref(replacement) else {
            dst.push(b'$');
            replacement = &replacement[1..];
            continue;
        };
        replacement = &replacement[cap_ref.end..];
        match cap_ref.cap {
            Ref::Number(i) => append(i, dst),
            Ref::Named(name) => {
                if let Some(i) = name_to_index(name) {
                    append(i, dst);
                }
            }
        }
    }
    dst.extend_from_slice(replacement);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CaptureRef<'a> {
    cap: Ref<'a>,
    end: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Ref<'a> {
    Named(&'a str),
    Number(usize),
}

impl<'a> From<&'a str> for Ref<'a> {
    fn from(x: &'a str) -> Ref<'a> {
        Ref::Named(x)
    }
}

impl From<usize> for Ref<'static> {
    fn from(x: usize) -> Ref<'static> {
        Ref::Number(x)
    }
}

fn find_cap_ref(replacement: &[u8]) -> Option<CaptureRef<'_>> {
    if replacement.len() <= 1 || replacement[0] != b'$' {
        return None;
    }
    let brace = replacement[1] == b'{';
    let start = if brace { 2 } else { 1 };
    let name_len = replacement[start..]
        .iter()
        .take_while(|&&b| b.is_ascii_alphanumeric() || b == b'_')
        .count();
    if name_len == 0 {
        return None;
    }
    let cap_end = start + name_len;
    let cap = std::str::from_utf8(&replacement[start..cap_end]).ok()?;
    if brace && replacement.get(cap_end) != Some(&b'}') {
        return None;
    }
    Some(CaptureRef {
        cap: cap
            .parse::<u32>()
            .map_or(Ref::Named(cap), |i| Ref::Number(i as usize)),
        end: if brace { cap_end + 1 } else { cap_end },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::bytes::{CaptureLocations, Regex};
    use std::collections::HashMap;

    macro_rules! find {
        ($name:ident, $text:expr) => {
            #[test]
            fn $name() {
                assert_eq!(None, find_cap_ref($text.as_bytes()));
            }
        };
        ($name:ident, $text:expr, $capref:expr) => {
            #[test]
            fn $name() {
                assert_eq!(Some($capref), find_cap_ref($text.as_bytes()));
            }
        };
    }

    macro_rules! c {
        ($name_or_number:expr, $pos:expr) => {
            CaptureRef {
                cap: $name_or_number.into(),
                end: $pos,
            }
        };
    }

    find!(find_cap_ref1, "$foo", c!("foo", 4));
    find!(find_cap_ref2, "${foo}", c!("foo", 6));
    find!(find_cap_ref3, "$0", c!(0, 2));
    find!(find_cap_ref4, "$5", c!(5, 2));
    find!(find_cap_ref5, "$10", c!(10, 3));
    find!(find_cap_ref6, "$42a", c!("42a", 4));
    find!(find_cap_ref7, "${42}a", c!(42, 5));
    find!(find_cap_ref8, "${42");
    find!(find_cap_ref9, "${42 ");
    find!(find_cap_ref10, " $0 ");
    find!(find_cap_ref11, "$");
    find!(find_cap_ref12, " ");
    find!(find_cap_ref13, "");

    fn interpolate_string(
        mut name_to_index: Vec<(&str, usize)>,
        caps: &[&str],
        replacement: &str,
    ) -> String {
        name_to_index.sort_by_key(|x| x.0);
        let mut dst = vec![];
        interpolate(
            replacement.as_bytes(),
            |i, dst| {
                if let Some(&s) = caps.get(i) {
                    dst.extend_from_slice(s.as_bytes());
                }
            },
            |name| {
                name_to_index
                    .binary_search_by_key(&name, |x| x.0)
                    .ok()
                    .map(|i| name_to_index[i].1)
            },
            &mut dst,
        );
        String::from_utf8(dst).unwrap()
    }

    macro_rules! interp {
        ($name:ident, $map:expr, $caps:expr, $hay:expr, $expected:expr $(,)*) => {
            #[test]
            fn $name() {
                assert_eq!(
                    $expected,
                    interpolate_string($map, &$caps, $hay)
                );
            }
        };
    }

    interp!(
        interp1,
        vec![("foo", 2)],
        ["", "", "xxx"],
        "test $foo test",
        "test xxx test"
    );
    interp!(
        interp2,
        vec![("foo", 2)],
        ["", "", "xxx"],
        "test$footest",
        "test"
    );
    interp!(
        interp3,
        vec![("foo", 2)],
        ["", "", "xxx"],
        "test${foo}test",
        "testxxxtest"
    );
    interp!(
        interp4,
        vec![("foo", 2)],
        ["", "", "xxx"],
        "test$2test",
        "test"
    );
    interp!(
        interp5,
        vec![("foo", 2)],
        ["", "", "xxx"],
        "test${2}test",
        "testxxxtest"
    );
    interp!(
        interp6,
        vec![("foo", 2)],
        ["", "", "xxx"],
        "test $$foo test",
        "test $foo test"
    );
    interp!(
        interp7,
        vec![("foo", 2)],
        ["", "", "xxx"],
        "test $foo",
        "test xxx"
    );
    interp!(
        interp8,
        vec![("foo", 2)],
        ["", "", "xxx"],
        "$foo test",
        "xxx test"
    );
    interp!(
        interp9,
        vec![("bar", 1), ("foo", 2)],
        ["", "yyy", "xxx"],
        "test $bar$foo",
        "test yyyxxx"
    );
    interp!(
        interp10,
        vec![("bar", 1), ("foo", 2)],
        ["", "yyy", "xxx"],
        "test $ test",
        "test $ test"
    );
    interp!(
        interp11,
        vec![("bar", 1), ("foo", 2)],
        ["", "yyy", "xxx"],
        "test ${} test",
        "test ${} test"
    );
    interp!(
        interp12,
        vec![("bar", 1), ("foo", 2)],
        ["", "yyy", "xxx"],
        "test ${ } test",
        "test ${ } test"
    );
    interp!(
        interp13,
        vec![("bar", 1), ("foo", 2)],
        ["", "yyy", "xxx"],
        "test ${a b} test",
        "test ${a b} test"
    );
    interp!(
        interp14,
        vec![("bar", 1), ("foo", 2)],
        ["", "yyy", "xxx"],
        "test ${a} test",
        "test  test"
    );

    #[derive(Debug)]
    struct TestRegex {
        re: Regex,
        names: HashMap<String, usize>,
    }

    impl TestRegex {
        fn new(pattern: &str) -> TestRegex {
            let re = Regex::new(pattern).unwrap();
            let names = re
                .capture_names()
                .enumerate()
                .filter_map(|(i, n)| n.map(|n| (n.to_string(), i)))
                .collect();
            TestRegex { re, names }
        }
    }

    #[derive(Clone, Debug)]
    struct TestCaptures(CaptureLocations);

    impl Captures for TestCaptures {
        fn get(&self, i: usize) -> Option<Match> {
            self.0.get(i).map(|(s, e)| Match::new(s, e))
        }
    }

    impl Matcher for TestRegex {
        type Captures = TestCaptures;
        type Error = NoError;

        fn find_at(&self, haystack: &[u8], at: usize) -> Result<Option<Match>, NoError> {
            Ok(self
                .re
                .find_at(haystack, at)
                .map(|m| Match::new(m.start(), m.end())))
        }

        fn new_captures(&self) -> Result<TestCaptures, NoError> {
            Ok(TestCaptures(self.re.capture_locations()))
        }

        fn captures_at(
            &self,
            haystack: &[u8],
            at: usize,
            caps: &mut TestCaptures,
        ) -> Result<bool, NoError> {
            Ok(self
                .re
                .captures_read_at(&mut caps.0, haystack, at)
                .is_some())
        }

        fn capture_count(&self) -> usize {
            self.re.captures_len()
        }

        fn capture_index(&self, name: &str) -> Option<usize> {
            self.names.get(name).copied()
        }
    }

    #[derive(Debug)]
    struct TestRegexNoCaps(Regex);

    impl Matcher for TestRegexNoCaps {
        type Captures = NoCaptures;
        type Error = NoError;

        fn find_at(&self, haystack: &[u8], at: usize) -> Result<Option<Match>, NoError> {
            Ok(self
                .0
                .find_at(haystack, at)
                .map(|m| Match::new(m.start(), m.end())))
        }

        fn new_captures(&self) -> Result<NoCaptures, NoError> {
            Ok(NoCaptures::new())
        }
    }

    fn m(start: usize, end: usize) -> Match {
        Match::new(start, end)
    }

    #[test]
    fn matcher_find() {
        let matcher = TestRegex::new(r"(\w+)\s+(\w+)");
        assert_eq!(matcher.find(b" homer simpson ").unwrap(), Some(m(1, 14)));
    }

    #[test]
    fn matcher_find_iter() {
        let matcher = TestRegex::new(r"(\w+)\s+(\w+)");
        let mut found = vec![];
        matcher
            .find_iter(b"aa bb cc dd", |m| {
                found.push(m);
                true
            })
            .unwrap();
        assert_eq!(found, vec![m(0, 5), m(6, 11)]);
        found.clear();
        matcher
            .find_iter(b"aa bb cc dd", |m| {
                found.push(m);
                false
            })
            .unwrap();
        assert_eq!(found, vec![m(0, 5)]);
    }

    #[test]
    fn matcher_try_find_iter() {
        #[derive(Clone, Debug, Eq, PartialEq)]
        struct MyError;
        let matcher = TestRegex::new(r"(\w+)\s+(\w+)");
        let mut found = vec![];
        let err = matcher
            .try_find_iter(b"aa bb cc dd", |m| {
                if found.is_empty() {
                    found.push(m);
                    Ok(true)
                } else {
                    Err(MyError)
                }
            })
            .unwrap()
            .unwrap_err();
        assert_eq!(found, vec![m(0, 5)]);
        assert_eq!(err, MyError);
    }

    #[test]
    fn matcher_shortest_match() {
        let matcher = TestRegex::new(r"a+");
        assert_eq!(matcher.shortest_match(b"aaa").unwrap(), Some(3));
        assert_eq!(matcher.re.shortest_match(b"aaa"), Some(1));
    }

    #[test]
    fn matcher_captures() {
        let matcher = TestRegex::new(r"(?P<a>\w+)\s+(?P<b>\w+)");
        assert_eq!(matcher.capture_count(), 3);
        assert_eq!(matcher.capture_index("a"), Some(1));
        assert_eq!(matcher.capture_index("b"), Some(2));
        assert_eq!(matcher.capture_index("nada"), None);
        let mut caps = matcher.new_captures().unwrap();
        assert!(matcher.captures(b" homer simpson ", &mut caps).unwrap());
        assert_eq!(caps.get(0), Some(m(1, 14)));
        assert_eq!(caps.get(1), Some(m(1, 6)));
        assert_eq!(caps.get(2), Some(m(7, 14)));
    }

    #[test]
    fn matcher_captures_iter() {
        let matcher = TestRegex::new(r"(?P<a>\w+)\s+(?P<b>\w+)");
        let mut caps = matcher.new_captures().unwrap();
        let mut found = vec![];
        matcher
            .captures_iter(b"aa bb cc dd", &mut caps, |caps| {
                found.push(caps.get(0).unwrap());
                found.push(caps.get(1).unwrap());
                found.push(caps.get(2).unwrap());
                true
            })
            .unwrap();
        assert_eq!(
            found,
            vec![m(0, 5), m(0, 2), m(3, 5), m(6, 11), m(6, 8), m(9, 11)]
        );
        found.clear();
        matcher
            .captures_iter(b"aa bb cc dd", &mut caps, |caps| {
                found.push(caps.get(0).unwrap());
                found.push(caps.get(1).unwrap());
                found.push(caps.get(2).unwrap());
                false
            })
            .unwrap();
        assert_eq!(found, vec![m(0, 5), m(0, 2), m(3, 5)]);
    }

    #[test]
    fn matcher_try_captures_iter() {
        #[derive(Clone, Debug, Eq, PartialEq)]
        struct MyError;
        let matcher = TestRegex::new(r"(?P<a>\w+)\s+(?P<b>\w+)");
        let mut caps = matcher.new_captures().unwrap();
        let mut found = vec![];
        let err = matcher
            .try_captures_iter(b"aa bb cc dd", &mut caps, |caps| {
                if found.is_empty() {
                    found.push(caps.get(0).unwrap());
                    found.push(caps.get(1).unwrap());
                    found.push(caps.get(2).unwrap());
                    Ok(true)
                } else {
                    Err(MyError)
                }
            })
            .unwrap()
            .unwrap_err();
        assert_eq!(found, vec![m(0, 5), m(0, 2), m(3, 5)]);
        assert_eq!(err, MyError);
    }

    #[test]
    fn matcher_no_captures() {
        let matcher = TestRegexNoCaps(Regex::new(r"(?P<a>\w+)\s+(?P<b>\w+)").unwrap());
        assert_eq!(matcher.capture_count(), 0);
        assert_eq!(matcher.capture_index("a"), None);
        assert_eq!(matcher.capture_index("b"), None);
        assert_eq!(matcher.capture_index("nada"), None);
        let mut caps = matcher.new_captures().unwrap();
        assert!(!matcher.captures(b"homer simpson", &mut caps).unwrap());
        let mut called = false;
        matcher
            .captures_iter(b"homer simpson", &mut caps, |_| {
                called = true;
                true
            })
            .unwrap();
        assert!(!called);
    }

    #[test]
    fn matcher_replace() {
        let matcher = TestRegex::new(r"(\w+)\s+(\w+)");
        let mut dst = vec![];
        matcher
            .replace(b"aa bb cc dd", &mut dst, |_, dst| {
                dst.push(b'z');
                true
            })
            .unwrap();
        assert_eq!(dst, b"z z");
        dst.clear();
        matcher
            .replace(b"aa bb cc dd", &mut dst, |_, dst| {
                dst.push(b'z');
                false
            })
            .unwrap();
        assert_eq!(dst, b"z cc dd");
    }

    #[test]
    fn matcher_replace_with_captures() {
        let matcher = TestRegex::new(r"(\w+)\s+(\w+)");
        let haystack = b"aa bb cc dd";
        let mut caps = matcher.new_captures().unwrap();
        let mut dst = vec![];
        matcher
            .replace_with_captures(haystack, &mut caps, &mut dst, |caps, dst| {
                caps.interpolate(|name| matcher.capture_index(name), haystack, b"$2 $1", dst);
                true
            })
            .unwrap();
        assert_eq!(dst, b"bb aa dd cc");
        dst.clear();
        matcher
            .replace_with_captures(haystack, &mut caps, &mut dst, |caps, dst| {
                caps.interpolate(|name| matcher.capture_index(name), haystack, b"$2 $1", dst);
                false
            })
            .unwrap();
        assert_eq!(dst, b"bb aa cc dd");
    }

    #[test]
    fn byte_set_basics() {
        let mut set = ByteSet::empty();
        set.add(b'a');
        set.add_all(b'x', b'z');
        assert!(set.contains(b'a') && set.contains(b'y') && !set.contains(b'b'));
        set.remove_all(b'x', b'y');
        assert!(!set.contains(b'x') && set.contains(b'z'));
        let full = ByteSet::full();
        assert!((0..=255u8).all(|b| full.contains(b)));
    }
}

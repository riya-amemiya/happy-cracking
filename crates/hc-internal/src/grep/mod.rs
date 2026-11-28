pub mod bytes;
pub mod matcher;
pub mod printer;
pub mod regex;
pub mod searcher;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Flags(pub u32);

impl Flags {
    #[inline]
    #[must_use]
    pub const fn contains(self, bit: u32) -> bool {
        self.0 & bit != 0
    }

    #[inline]
    pub fn set(&mut self, bit: u32, yes: bool) {
        if yes {
            self.0 |= bit;
        } else {
            self.0 &= !bit;
        }
    }
}

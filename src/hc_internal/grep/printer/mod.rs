#[cfg(test)]
macro_rules! assert_eq_printed {
    ($expected:expr, $got:expr) => {{
        let expected = &*$expected;
        let got = &*$got;
        assert!(
            expected == got,
            "printed outputs differ!\n\nexpected:\n{expected}\n\ngot:\n{got}\n"
        );
    }};
}

mod color;
mod counter;
mod hyperlink;
mod json;
mod path;
mod standard;
mod stats;
mod summary;
pub mod termcolor;
mod util;

pub use color::{ColorSpecs, UserColorSpec, default_color_specs};
pub use hyperlink::{HyperlinkConfig, HyperlinkEnvironment, HyperlinkFormat};
pub use json::{JSONBuilder, Json as JSON};
pub use path::PathPrinterBuilder;
pub use standard::{Standard, StandardBuilder};
pub use stats::Stats;
pub use summary::{Summary, SummaryBuilder, SummaryKind};

const MAX_LOOK_AHEAD: usize = 128;

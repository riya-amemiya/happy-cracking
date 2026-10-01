pub(super) const NAME: &str = env!("CARGO_PKG_NAME");
pub(super) const VERSION: &str = env!("CARGO_PKG_VERSION");

pub(super) fn digits() -> String {
    format!("{VERSION} ({NAME})")
}

pub(crate) fn version_short(prog: &str) -> String {
    format!("{prog} {}", digits())
}

pub(crate) fn version_long(prog: &str) -> String {
    format!(
        "{}\n\nfeatures:+pcre2\n\n{}",
        version_short(prog),
        pcre2_version().0
    )
}

pub(crate) fn pcre2_version() -> (String, bool) {
    (
        format!(
            "PCRE2 compatible engine (built into {NAME} {VERSION}) is available (JIT is unavailable)\n"
        ),
        true,
    )
}

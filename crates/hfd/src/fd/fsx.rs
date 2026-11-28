use std::borrow::Cow;
use std::env;
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};

pub(super) fn path_absolute_form(path: &Path) -> io::Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    let path = path.strip_prefix(".").unwrap_or(path);
    env::current_dir().map(|cwd| cwd.join(path))
}

pub(super) fn absolute_path(path: &Path) -> io::Result<PathBuf> {
    let path_buf = path_absolute_form(path)?;
    if cfg!(windows) {
        return Ok(PathBuf::from(
            path_buf.to_string_lossy().trim_start_matches(r"\\?\"),
        ));
    }
    Ok(path_buf)
}

pub(super) fn is_existing_directory(path: &Path) -> bool {
    path.is_dir() && (path.file_name().is_some() || path.canonicalize().is_ok())
}

pub(super) fn strip_current_dir(path: &Path) -> &Path {
    let bytes = path.as_os_str().as_encoded_bytes();
    if bytes.len() > 2 && bytes.starts_with(b"./") && bytes[2] != b'/' && bytes[2] != b'.' {
        let rest = &bytes[2..];
        if !rest.ends_with(b"/") && !rest.windows(2).any(|w| w == b"//" || w == b"/.") {
            return Path::new(unsafe { OsStr::from_encoded_bytes_unchecked(rest) });
        }
    }
    path.strip_prefix(".").unwrap_or(path)
}

pub(super) fn osstr_bytes(input: &OsStr) -> Cow<'_, [u8]> {
    if cfg!(unix) {
        Cow::Borrowed(input.as_encoded_bytes())
    } else {
        match input.to_string_lossy() {
            Cow::Owned(s) => Cow::Owned(s.into_bytes()),
            Cow::Borrowed(s) => Cow::Borrowed(s.as_bytes()),
        }
    }
}

pub(super) fn default_path_separator() -> Option<String> {
    if cfg!(windows) {
        let msystem = env::var("MSYSTEM").ok()?;
        if !msystem.is_empty() {
            return Some("/".to_owned());
        }
    }
    None
}

pub(super) fn config_dir() -> Option<PathBuf> {
    let home = env::home_dir()?;
    let var = if cfg!(windows) {
        "APPDATA"
    } else {
        "XDG_CONFIG_HOME"
    };
    Some(
        env::var_os(var)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| {
                if cfg!(windows) {
                    home.join("AppData").join("Roaming")
                } else {
                    home.join(".config/")
                }
            }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_matches_path_semantics() {
        for p in [
            "./foo",
            "foo",
            "./foo/bar/baz",
            "foo/bar",
            ".//foo",
            "./.hidden",
            "./a/./b",
            "./a/",
            "./",
            ".",
            "../x",
        ] {
            let path = Path::new(p);
            assert_eq!(
                strip_current_dir(path),
                path.strip_prefix(".").unwrap_or(path),
                "{p}"
            );
        }
        assert!(is_existing_directory(Path::new(".")));
        assert!(!is_existing_directory(Path::new("/hfd-no-such-dir")));
        assert!(path_absolute_form(Path::new("./x")).unwrap().ends_with("x"));
        assert_eq!(absolute_path(Path::new("/a")).unwrap(), PathBuf::from("/a"));
        assert_eq!(osstr_bytes(OsStr::new("ab")).len(), 2);
        assert!(default_path_separator().is_none() || cfg!(windows));
        assert!(config_dir().is_some());
    }
}

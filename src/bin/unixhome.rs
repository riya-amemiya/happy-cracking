//! Resolve `~user` home directories without libc `getpwnam_r`.
//!
//! Local accounts come from `/etc/passwd`. NSS/LDAP users fall back to `getent`
//! so gitconfig `~user` expansion stays compatible off the hot path.

use std::ffi::OsString;
use std::fs;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::process::Command;

pub fn home_of(user: &[u8]) -> Option<PathBuf> {
    if user.is_empty() || user.contains(&0) || user.contains(&b'/') || user.contains(&b':') {
        return None;
    }
    fs::read("/etc/passwd")
        .ok()
        .and_then(|data| home_from_passwd_bytes(&data, user))
        .or_else(|| {
            if user.first() == Some(&b'-') {
                None
            } else {
                home_from_getent(user)
            }
        })
}

pub fn home_from_passwd_bytes(data: &[u8], name: &[u8]) -> Option<PathBuf> {
    for line in data.split(|&b| b == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.is_empty() || line.starts_with(b"#") {
            continue;
        }
        let mut fields = line.splitn(7, |&b| b == b':');
        if fields.next()? != name {
            continue;
        }
        let home = fields.nth(4)?;
        if home.is_empty() {
            return None;
        }
        return Some(PathBuf::from(OsString::from_vec(home.to_vec())));
    }
    None
}

fn home_from_getent(user: &[u8]) -> Option<PathBuf> {
    let name = std::str::from_utf8(user).ok()?;
    let out = Command::new("getent")
        .args(["passwd", name])
        .output()
        .ok()?;
    if !out.status.success() || out.stdout.is_empty() {
        return None;
    }
    home_from_passwd_bytes(&out.stdout, user)
}

//! Resolve `~user` home directories without libc `getpwnam_r`.
//!
//! Local accounts come from `/etc/passwd`. NSS/LDAP users fall back to `getent`
//! so gitconfig `~user` expansion stays compatible off the hot path.

use std::ffi::OsString;
use std::fs::File;
use std::io::{self, Read};
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const MAX_PASSWD_FILE_BYTES: usize = 16 * 1024 * 1024;

pub fn read_passwd_with_limit<R: Read>(reader: R, max_bytes: usize) -> io::Result<Vec<u8>> {
    let max_bytes = max_bytes.min(MAX_PASSWD_FILE_BYTES);
    let mut buf = Vec::new();
    reader
        .take((max_bytes as u64).saturating_add(1))
        .read_to_end(&mut buf)?;
    if buf.len() > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "passwd data exceeds maximum size of {max_bytes} bytes to prevent Denial of Service"
            ),
        ));
    }
    Ok(buf)
}

pub fn read_passwd_file_with_limit(path: &Path, max_bytes: usize) -> io::Result<Vec<u8>> {
    read_passwd_with_limit(File::open(path)?, max_bytes)
}

pub fn home_of(user: &[u8]) -> Option<PathBuf> {
    if user.is_empty() || user.contains(&0) || user.contains(&b'/') || user.contains(&b':') {
        return None;
    }
    read_passwd_file_with_limit(Path::new("/etc/passwd"), MAX_PASSWD_FILE_BYTES)
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
    let mut child = Command::new("getent")
        .args(["passwd", name])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let Ok(data) = read_passwd_with_limit(stdout, MAX_PASSWD_FILE_BYTES) else {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    };
    let status = child.wait().ok()?;
    if !status.success() || data.is_empty() {
        return None;
    }
    home_from_passwd_bytes(&data, user)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn read_passwd_file_with_limit_rejects_oversized_file() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "hfind_passwd_oversize_{}_{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("passwd");
        fs::write(&path, vec![b'a'; 32]).unwrap();
        let err = read_passwd_file_with_limit(&path, 16).unwrap_err();
        fs::remove_dir_all(&dir).unwrap();
        assert!(err.to_string().contains("Denial of Service"));
    }

    #[test]
    fn read_passwd_file_with_limit_accepts_file_at_limit() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "hfind_passwd_at_limit_{}_{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("passwd");
        let data = b"alice:x:1000:1000:Alice:/home/alice:/bin/bash\n";
        fs::write(&path, data).unwrap();
        let got = read_passwd_file_with_limit(&path, data.len()).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(got, data);
    }

    #[test]
    fn read_passwd_file_with_limit_accepts_empty() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("hfind_passwd_empty_{}_{nanos}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("passwd");
        fs::write(&path, b"").unwrap();
        let got = read_passwd_file_with_limit(&path, 16).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        assert!(got.is_empty());
    }

    #[test]
    fn read_passwd_with_limit_rejects_oversized_reader() {
        let data = vec![b'a'; 32];
        let err = read_passwd_with_limit(&data[..], 16).unwrap_err();
        assert!(err.to_string().contains("Denial of Service"));
    }

    #[test]
    fn read_passwd_with_limit_accepts_reader_at_limit() {
        let data = b"alice:x:1000:1000:Alice:/home/alice:/bin/bash\n";
        let got = read_passwd_with_limit(&data[..], data.len()).unwrap();
        assert_eq!(got, data);
    }

    #[cfg(unix)]
    #[test]
    fn read_passwd_file_with_limit_bounds_device_without_eof() {
        let err = read_passwd_file_with_limit(Path::new("/dev/zero"), 64).unwrap_err();
        assert!(err.to_string().contains("Denial of Service"));
    }

    #[test]
    fn home_of_current_user_matches_passwd_bytes() {
        let Ok(name) = std::env::var("USER") else {
            return;
        };
        if name.is_empty() {
            return;
        }
        let Ok(data) = read_passwd_file_with_limit(Path::new("/etc/passwd"), MAX_PASSWD_FILE_BYTES)
        else {
            return;
        };
        let expected = home_from_passwd_bytes(&data, name.as_bytes());
        if let Some(home) = expected {
            assert_eq!(home_of(name.as_bytes()).as_deref(), Some(home.as_path()));
        }
    }
}

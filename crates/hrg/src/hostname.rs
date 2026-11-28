use std::io::{self, Read};
use std::path::Path;
use std::process::{Command, Stdio};

use super::process::resolve_binary;

const MAX_HOSTNAME_BIN_BYTES: usize = 16 * 1024 * 1024;

fn read_hostname_output_with_limit<R: Read>(reader: R, max_bytes: usize) -> io::Result<String> {
    let max_bytes = max_bytes.min(MAX_HOSTNAME_BIN_BYTES);
    let mut buf = Vec::new();
    reader
        .take((max_bytes as u64).saturating_add(1))
        .read_to_end(&mut buf)?;
    if buf.len() > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "hostname-bin output exceeds maximum size of {max_bytes} bytes to prevent Denial of Service"
            ),
        ));
    }
    String::from_utf8(buf).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

#[cfg(unix)]
fn platform_hostname() -> Option<String> {
    unsafe extern "C" {
        fn gethostname(name: *mut std::ffi::c_char, len: usize) -> std::ffi::c_int;
    }
    let mut buf = vec![0u8; 256];
    let rc = unsafe { gethostname(buf.as_mut_ptr().cast(), buf.len()) };
    if rc != 0 {
        return None;
    }
    let end = buf.iter().position(|&b| b == 0)?;
    buf.truncate(end);
    String::from_utf8(buf).ok()
}

#[cfg(not(unix))]
fn platform_hostname() -> Option<String> {
    std::env::var("COMPUTERNAME").ok().filter(|h| !h.is_empty())
}

fn hostname_from_binary(bin: &Path) -> Option<String> {
    let mut child = Command::new(bin)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let Ok(out) = read_hostname_output_with_limit(stdout, MAX_HOSTNAME_BIN_BYTES) else {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    };
    if !child.wait().ok()?.success() {
        return None;
    }
    let trimmed = out.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub(crate) fn hostname(bin: Option<&Path>) -> Option<String> {
    let Some(bin) = bin else {
        return platform_hostname();
    };
    let Ok(bin) = resolve_binary(bin) else {
        return platform_hostname();
    };
    hostname_from_binary(&bin).or_else(platform_hostname)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_hostname_output_with_limit_rejects_oversized() {
        let err = read_hostname_output_with_limit(&b"abcdefghijklmnop"[..], 8).unwrap_err();
        assert!(err.to_string().contains("Denial of Service"));
    }

    #[test]
    fn read_hostname_output_with_limit_accepts_at_limit() {
        let got = read_hostname_output_with_limit(&b"host"[..], 4).unwrap();
        assert_eq!(got, "host");
    }

    #[test]
    fn read_hostname_output_with_limit_accepts_empty() {
        let got = read_hostname_output_with_limit(&b""[..], 16).unwrap();
        assert!(got.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn read_hostname_output_with_limit_bounds_device_without_eof() {
        let file = std::fs::File::open("/dev/zero").unwrap();
        let err = read_hostname_output_with_limit(file, 64).unwrap_err();
        assert!(err.to_string().contains("Denial of Service"));
    }

    #[cfg(unix)]
    fn exec_script(tag: &str, body: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("hgrep_host_{tag}_{}_{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("host.sh");
        std::fs::write(&script, body).unwrap();
        let mut perms = std::fs::metadata(&script).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
        std::fs::set_permissions(&script, perms).unwrap();
        (dir, script)
    }

    #[cfg(unix)]
    #[test]
    fn hostname_from_binary_is_trimmed() {
        let (dir, script) = exec_script("trim", "#!/bin/sh\nprintf '  example.test \\n'\n");
        let got = hostname(Some(&script));
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(got.as_deref(), Some("example.test"));
    }

    #[cfg(unix)]
    #[test]
    fn hostname_from_binary_bounds_device_without_eof() {
        let (dir, script) = exec_script("devzero", "#!/bin/sh\nexec cat /dev/zero\n");
        let got = hostname(Some(&script));
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(got, platform_hostname());
    }

    #[cfg(unix)]
    #[test]
    fn platform_hostname_is_available() {
        assert!(platform_hostname().is_some_and(|h| !h.is_empty()));
    }
}

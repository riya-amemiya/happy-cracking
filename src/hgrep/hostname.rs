use std::path::Path;
use std::process::{Command, Stdio};

use super::process::{CommandReader, resolve_binary};

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

pub(crate) fn hostname(bin: Option<&Path>) -> Option<String> {
    let Some(bin) = bin else {
        return platform_hostname();
    };
    let Ok(bin) = resolve_binary(bin) else {
        return platform_hostname();
    };
    let mut cmd = Command::new(&bin);
    cmd.stdin(Stdio::null());
    let Ok(reader) = CommandReader::new(&mut cmd) else {
        return platform_hostname();
    };
    let Ok(out) = std::io::read_to_string(reader) else {
        return platform_hostname();
    };
    let trimmed = out.trim();
    if trimmed.is_empty() {
        platform_hostname()
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn hostname_from_binary_is_trimmed() {
        let dir = std::env::temp_dir().join(format!("hgrep_host_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("host.sh");
        std::fs::write(&script, "#!/bin/sh\nprintf '  example.test \\n'\n").unwrap();
        let mut perms = std::fs::metadata(&script).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
        std::fs::set_permissions(&script, perms).unwrap();
        assert_eq!(hostname(Some(&script)).as_deref(), Some("example.test"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn platform_hostname_is_available() {
        assert!(platform_hostname().is_some_and(|h| !h.is_empty()));
    }
}

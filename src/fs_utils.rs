use std::collections::HashMap;
use std::env;
use std::io;
use std::path::Path;

/// Set up the working directory for the session, preserving the user's cwd if possible.
pub fn setup_working_directory(original_cwd: &Path) -> io::Result<()> {
    if original_cwd.exists() {
        env::set_current_dir(original_cwd)?;
    } else {
        // Fall back to home directory or /
        let home = env::var("HOME").unwrap_or_else(|_| "/".into());
        env::set_current_dir(home)?;
    }
    Ok(())
}

/// Prepare environment variables for the sketch session.
pub fn setup_session_env(original_uid: u32, original_gid: u32) -> HashMap<String, String> {
    let mut env_vars = HashMap::new();

    env_vars.insert("SKETCH_SESSION".into(), "1".into());
    env_vars.insert("SKETCH_ORIGINAL_UID".into(), original_uid.to_string());
    env_vars.insert("SKETCH_ORIGINAL_GID".into(), original_gid.to_string());

    // Preserve important env vars
    for key in &[
        "HOME", "USER", "LOGNAME", "SHELL", "TERM", "LANG", "PATH", "EDITOR", "VISUAL",
    ] {
        if let Ok(val) = env::var(key) {
            env_vars.insert((*key).into(), val);
        }
    }

    env_vars
}

/// Ensure essential device nodes are accessible.
pub fn check_device_access() -> Vec<String> {
    let mut warnings = Vec::new();
    let essential_devices = ["/dev/null", "/dev/zero", "/dev/urandom", "/dev/tty"];

    for dev in &essential_devices {
        if !Path::new(dev).exists() {
            warnings.push(format!("Warning: {} not available in session", dev));
        }
    }

    warnings
}

/// Minimum free space required in /tmp before creating a session (100 MB).
const MIN_FREE_BYTES: u64 = 100 * 1024 * 1024;

/// Threshold at which a warning is shown (500 MB).
const WARN_FREE_BYTES: u64 = 500 * 1024 * 1024;

/// Result of a disk space check.
#[derive(Debug)]
pub struct DiskInfo {
    pub available: u64,
    pub used_pct: u8,
}

/// Possible outcomes of a disk space pre-check.
#[derive(Debug)]
pub enum DiskCheck {
    /// Plenty of space.
    Ok(DiskInfo),
    /// Space is getting low — warn but allow.
    Warning(DiskInfo, String),
    /// Critically low — block session creation unless forced.
    Critical(DiskInfo, String),
}

/// Check available disk space at the given path (typically /tmp).
pub fn check_disk_space(path: &Path) -> Result<DiskCheck, String> {
    let stat = nix::sys::statvfs::statvfs(path)
        .map_err(|e| format!("Failed to check disk space on {}: {}", path.display(), e))?;

    let total = stat.blocks() * stat.fragment_size();
    let available = stat.blocks_available() * stat.fragment_size();
    let used_pct = if total > 0 {
        (100 - (available * 100 / total)) as u8
    } else {
        0
    };

    let info = DiskInfo {
        available,
        used_pct,
    };

    if available < MIN_FREE_BYTES {
        let msg = format!(
            "Only {} free in {} ({}% used). Minimum {} required.",
            crate::metadata::format_size(available),
            path.display(),
            used_pct,
            crate::metadata::format_size(MIN_FREE_BYTES),
        );
        Ok(DiskCheck::Critical(info, msg))
    } else if available < WARN_FREE_BYTES {
        let msg = format!(
            "{} free in {} ({}% used). Consider cleaning up old sessions.",
            crate::metadata::format_size(available),
            path.display(),
            used_pct,
        );
        Ok(DiskCheck::Warning(info, msg))
    } else {
        Ok(DiskCheck::Ok(info))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // ============================================================
    // setup_working_directory tests
    // ============================================================

    #[test]
    fn setup_working_directory_falls_back_for_nonexistent() {
        let result = setup_working_directory(Path::new("/nonexistent_dir_xyz_12345"));
        // Should fallback to HOME or /
        assert!(result.is_ok());
    }

    // ============================================================
    // setup_session_env tests
    // ============================================================

    #[test]
    fn setup_session_env_includes_sketch_session() {
        let env_vars = setup_session_env(1000, 1000);
        assert_eq!(env_vars.get("SKETCH_SESSION").unwrap(), "1");
    }

    #[test]
    fn setup_session_env_includes_uid_gid() {
        let env_vars = setup_session_env(1234, 5678);
        assert_eq!(env_vars.get("SKETCH_ORIGINAL_UID").unwrap(), "1234");
        assert_eq!(env_vars.get("SKETCH_ORIGINAL_GID").unwrap(), "5678");
    }

    #[test]
    fn setup_session_env_preserves_path() {
        env::set_var("PATH", "/usr/bin:/bin");
        let env_vars = setup_session_env(0, 0);
        assert!(env_vars.contains_key("PATH"));
    }

    // ============================================================
    // check_device_access tests
    // ============================================================

    #[test]
    fn check_device_access_on_host() {
        let warnings = check_device_access();
        // On a normal host, /dev/null should exist
        assert!(
            !warnings.iter().any(|w| w.contains("/dev/null")),
            "/dev/null should be accessible on host"
        );
    }
}

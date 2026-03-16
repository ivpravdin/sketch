use std::collections::HashMap;
use std::env;
use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

/// Resolve a path, following symlinks and normalizing components.
pub fn resolve_path(path: &Path) -> io::Result<PathBuf> {
    fs::canonicalize(path)
}

/// Check if a path is a symlink and return its target.
pub fn read_symlink(path: &Path) -> io::Result<PathBuf> {
    fs::read_link(path)
}

/// Create a temporary directory within the session overlay.
pub fn create_temp_dir(prefix: &str) -> io::Result<PathBuf> {
    let dir = env::temp_dir().join(format!("{}-{}", prefix, uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Create a temporary file within the session overlay.
pub fn create_temp_file(prefix: &str, suffix: &str) -> io::Result<PathBuf> {
    let path = env::temp_dir().join(format!("{}-{}{}", prefix, uuid::Uuid::new_v4(), suffix));
    fs::File::create(&path)?;
    Ok(path)
}

/// Set file permissions using mode bits (e.g., 0o755).
pub fn set_permissions(path: &Path, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = fs::Permissions::from_mode(mode);
    fs::set_permissions(path, perms)
}

/// Get file metadata including uid, gid, and mode.
pub fn file_info(path: &Path) -> io::Result<FileInfo> {
    let meta = fs::metadata(path)?;
    Ok(FileInfo {
        size: meta.len(),
        uid: meta.uid(),
        gid: meta.gid(),
        mode: meta.mode(),
        is_dir: meta.is_dir(),
        is_symlink: fs::symlink_metadata(path)?.file_type().is_symlink(),
    })
}

#[derive(Debug)]
pub struct FileInfo {
    pub size: u64,
    pub uid: u32,
    pub gid: u32,
    pub mode: u32,
    pub is_dir: bool,
    pub is_symlink: bool,
}

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
    for key in &["HOME", "USER", "LOGNAME", "SHELL", "TERM", "LANG", "PATH", "EDITOR", "VISUAL"] {
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
    pub total: u64,
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
        total,
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

/// Recursively copy a directory tree, preserving permissions.
pub fn copy_dir_recursive(src: &Path, dst: &Path) -> io::Result<u64> {
    let mut count = 0;
    fs::create_dir_all(dst)?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            count += copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
            count += 1;
        }
    }

    // Preserve directory permissions
    let src_perms = fs::metadata(src)?.permissions();
    fs::set_permissions(dst, src_perms)?;

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    // ============================================================
    // resolve_path tests
    // ============================================================

    #[test]
    fn resolve_path_absolute() {
        let resolved = resolve_path(Path::new("/tmp")).unwrap();
        assert!(resolved.is_absolute());
    }

    #[test]
    fn resolve_path_nonexistent_fails() {
        let result = resolve_path(Path::new("/nonexistent_path_xyz_12345"));
        assert!(result.is_err());
    }

    #[test]
    fn resolve_path_follows_symlinks() {
        // /proc/self is a symlink on Linux
        if Path::new("/proc/self").exists() {
            let resolved = resolve_path(Path::new("/proc/self"));
            assert!(resolved.is_ok());
        }
    }

    // ============================================================
    // read_symlink tests
    // ============================================================

    #[test]
    fn read_symlink_on_regular_file_fails() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("regular");
        fs::write(&file, "content").unwrap();
        let result = read_symlink(&file);
        assert!(result.is_err());
    }

    #[test]
    fn read_symlink_returns_target() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        let link = dir.path().join("link");
        fs::write(&target, "content").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let result = read_symlink(&link).unwrap();
        assert_eq!(result, target);
    }

    // ============================================================
    // create_temp_dir tests
    // ============================================================

    #[test]
    fn create_temp_dir_creates_directory() {
        let dir = create_temp_dir("test_sketch").unwrap();
        assert!(dir.is_dir());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn create_temp_dir_unique_paths() {
        let dir1 = create_temp_dir("test_sketch").unwrap();
        let dir2 = create_temp_dir("test_sketch").unwrap();
        assert_ne!(dir1, dir2);
        let _ = fs::remove_dir_all(&dir1);
        let _ = fs::remove_dir_all(&dir2);
    }

    #[test]
    fn create_temp_dir_uses_prefix() {
        let dir = create_temp_dir("myprefix").unwrap();
        let name = dir.file_name().unwrap().to_string_lossy();
        assert!(name.starts_with("myprefix-"), "dir name should start with prefix: {}", name);
        let _ = fs::remove_dir_all(&dir);
    }

    // ============================================================
    // create_temp_file tests
    // ============================================================

    #[test]
    fn create_temp_file_creates_file() {
        let file = create_temp_file("test_sketch", ".txt").unwrap();
        assert!(file.is_file());
        let _ = fs::remove_file(&file);
    }

    #[test]
    fn create_temp_file_has_suffix() {
        let file = create_temp_file("test_sketch", ".log").unwrap();
        let name = file.file_name().unwrap().to_string_lossy();
        assert!(name.ends_with(".log"), "file name should end with suffix: {}", name);
        let _ = fs::remove_file(&file);
    }

    #[test]
    fn create_temp_file_unique_paths() {
        let f1 = create_temp_file("test_sketch", ".tmp").unwrap();
        let f2 = create_temp_file("test_sketch", ".tmp").unwrap();
        assert_ne!(f1, f2);
        let _ = fs::remove_file(&f1);
        let _ = fs::remove_file(&f2);
    }

    // ============================================================
    // set_permissions tests
    // ============================================================

    #[test]
    fn set_permissions_changes_mode() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("perm_test");
        fs::write(&file, "test").unwrap();

        set_permissions(&file, 0o755).unwrap();
        let meta = fs::metadata(&file).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o755);

        set_permissions(&file, 0o644).unwrap();
        let meta = fs::metadata(&file).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o644);
    }

    #[test]
    fn set_permissions_nonexistent_fails() {
        let result = set_permissions(Path::new("/nonexistent_xyz_12345"), 0o644);
        assert!(result.is_err());
    }

    // ============================================================
    // file_info tests
    // ============================================================

    #[test]
    fn file_info_regular_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("info_test");
        fs::write(&file, "hello").unwrap();

        let info = file_info(&file).unwrap();
        assert_eq!(info.size, 5);
        assert!(!info.is_dir);
        assert!(!info.is_symlink);
    }

    #[test]
    fn file_info_directory() {
        let dir = tempfile::tempdir().unwrap();
        let info = file_info(dir.path()).unwrap();
        assert!(info.is_dir);
    }

    #[test]
    fn file_info_nonexistent_fails() {
        let result = file_info(Path::new("/nonexistent_xyz_12345"));
        assert!(result.is_err());
    }

    // ============================================================
    // setup_working_directory tests
    // ============================================================

    #[test]
    fn setup_working_directory_sets_existing_dir() {
        let dir = tempfile::tempdir().unwrap();
        setup_working_directory(dir.path()).unwrap();
        assert_eq!(env::current_dir().unwrap(), fs::canonicalize(dir.path()).unwrap());
    }

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

    // ============================================================
    // copy_dir_recursive tests
    // ============================================================

    #[test]
    fn copy_dir_recursive_copies_files() {
        let src_dir = tempfile::tempdir().unwrap();
        let dst_dir = tempfile::tempdir().unwrap();
        let dst = dst_dir.path().join("copy");

        fs::write(src_dir.path().join("a.txt"), "aaa").unwrap();
        fs::write(src_dir.path().join("b.txt"), "bbb").unwrap();

        let count = copy_dir_recursive(src_dir.path(), &dst).unwrap();
        assert_eq!(count, 2);
        assert_eq!(fs::read_to_string(dst.join("a.txt")).unwrap(), "aaa");
        assert_eq!(fs::read_to_string(dst.join("b.txt")).unwrap(), "bbb");
    }

    #[test]
    fn copy_dir_recursive_copies_nested_dirs() {
        let src_dir = tempfile::tempdir().unwrap();
        let dst_dir = tempfile::tempdir().unwrap();
        let dst = dst_dir.path().join("copy");

        let sub = src_dir.path().join("sub");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("nested.txt"), "nested").unwrap();

        let count = copy_dir_recursive(src_dir.path(), &dst).unwrap();
        assert_eq!(count, 1);
        assert_eq!(fs::read_to_string(dst.join("sub/nested.txt")).unwrap(), "nested");
    }

    #[test]
    fn copy_dir_recursive_empty_dir() {
        let src_dir = tempfile::tempdir().unwrap();
        let dst_dir = tempfile::tempdir().unwrap();
        let dst = dst_dir.path().join("copy");

        let count = copy_dir_recursive(src_dir.path(), &dst).unwrap();
        assert_eq!(count, 0);
        assert!(dst.is_dir());
    }
}

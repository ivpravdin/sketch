//! User namespace support for non-root operation.
//!
//! When running without root privileges, we create a user namespace that maps
//! the current user to UID 0 inside the namespace. This grants the capabilities
//! needed for OverlayFS mounting and pivot_root without actual host root access.
//!
//! Requirements:
//! - Linux kernel 3.8+ (user namespaces)
//! - Linux kernel 5.11+ (unprivileged OverlayFS in user namespaces)
//! - /proc/sys/user/max_user_namespaces > 0

use nix::sched::{unshare, CloneFlags};
use std::fs;
use std::path::Path;

/// Check if user namespaces are available on this system.
pub fn user_namespaces_available() -> bool {
    // Check kernel support via max_user_namespaces sysctl
    let max_ns = fs::read_to_string("/proc/sys/user/max_user_namespaces")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0);

    if max_ns == 0 {
        return false;
    }

    // Check that /proc/self/ns/user exists (namespace support compiled in)
    Path::new("/proc/self/ns/user").exists()
}

/// Check if the kernel supports unprivileged OverlayFS (5.11+).
pub fn unprivileged_overlayfs_available() -> bool {
    if let Ok(version) = fs::read_to_string("/proc/version") {
        if let Some(kver) = version.split_whitespace().nth(2) {
            return parse_kernel_version(kver)
                .map(|(major, minor)| major > 5 || (major == 5 && minor >= 11))
                .unwrap_or(false);
        }
    }
    false
}

/// Parse "X.Y.Z-..." into (major, minor).
fn parse_kernel_version(version: &str) -> Option<(u32, u32)> {
    let mut parts = version.split(|c: char| !c.is_ascii_digit());
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}

/// Determine whether we can use user namespaces for non-root operation.
pub fn can_use_user_namespaces() -> bool {
    user_namespaces_available() && unprivileged_overlayfs_available()
}

/// Setup user namespace for non-root operation.
/// CRITICAL: Must be called BEFORE unshare to capture current UID/GID.
pub fn setup_user_namespace() -> Result<(), String> {
    // Save UID/GID BEFORE unshare - after unshare they change
    let real_uid = nix::unistd::getuid().as_raw();
    let real_gid = nix::unistd::getgid().as_raw();

    // Create user namespace
    unshare(CloneFlags::CLONE_NEWUSER)
        .map_err(|e| format!("unshare(CLONE_NEWUSER) failed: {}", e))?;

    // Disable setgroups (required before UID/GID mapping on newer kernels)
    write_proc_file("/proc/self/setgroups", "deny")?;

    // Map container UID 0 to host UID
    write_proc_file("/proc/self/uid_map", &format!("0 {} 1", real_uid))?;

    // Map container GID 0 to host GID
    write_proc_file("/proc/self/gid_map", &format!("0 {} 1", real_gid))?;

    // Create mount namespace for isolation (sessions will create additional ones)
    unshare(CloneFlags::CLONE_NEWNS)
        .map_err(|e| format!("unshare(CLONE_NEWNS) failed: {}", e))?;

    Ok(())
}

fn write_proc_file(path: &str, contents: &str) -> Result<(), String> {
    fs::write(path, contents).map_err(|e| format!("{}: {}", path, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_kernel_version_standard() {
        assert_eq!(parse_kernel_version("5.11.0-generic"), Some((5, 11)));
        assert_eq!(parse_kernel_version("6.1.0-21-amd64"), Some((6, 1)));
        assert_eq!(parse_kernel_version("4.19.128"), Some((4, 19)));
    }

    #[test]
    fn parse_kernel_version_edge_cases() {
        assert_eq!(parse_kernel_version("5.11"), Some((5, 11)));
        assert_eq!(parse_kernel_version("6.14.0-37-generic"), Some((6, 14)));
    }

    #[test]
    fn parse_kernel_version_invalid() {
        assert_eq!(parse_kernel_version(""), None);
        assert_eq!(parse_kernel_version("abc"), None);
    }

    #[test]
    fn unprivileged_overlayfs_check() {
        // This test just verifies the function doesn't panic
        let _ = unprivileged_overlayfs_available();
    }

    #[test]
    fn user_namespaces_check() {
        // This test just verifies the function doesn't panic
        let _ = user_namespaces_available();
    }

    #[test]
    fn can_use_check() {
        // Verify the combined check doesn't panic
        let _ = can_use_user_namespaces();
    }
}

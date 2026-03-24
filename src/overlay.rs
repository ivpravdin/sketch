use nix::mount::{mount, umount2, MntFlags, MsFlags};
use nix::sched::{unshare, CloneFlags};
use sha2::{Sha256, Digest};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub struct OverlaySession {
    pub session_dir: PathBuf,
    pub upper_dir: PathBuf,
    pub work_dir: PathBuf,
    pub merged_dir: PathBuf,
    pub mounted: bool,
    pub extra_mounts: Vec<PathBuf>,
}

/// Generate a unique, collision-free name for a mount point's overlay directories.
///
/// Uses SHA256 hash of the mount path to ensure different mount points
/// (e.g., /home/user and /home_user) don't collide.
fn mount_name_from_path(mountpoint: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(mountpoint.as_bytes());
    let hash = hasher.finalize();
    // Use first 12 hex characters of hash (48 bits) for reasonable uniqueness
    // and readability. Format each byte as 2-character hex.
    hash[..6]
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>()
}

impl OverlaySession {
    pub fn new() -> io::Result<Self> {
        let session_id = uuid::Uuid::new_v4();
        let session_dir = PathBuf::from(format!("/tmp/sketch-{}", session_id));

        let upper_dir = session_dir.join("upper");
        let work_dir = session_dir.join("work");
        let merged_dir = session_dir.join("merged");

        fs::create_dir_all(&upper_dir)?;
        fs::create_dir_all(&work_dir)?;
        fs::create_dir_all(&merged_dir)?;

        Ok(Self {
            session_dir,
            upper_dir,
            work_dir,
            merged_dir,
            mounted: false,
            extra_mounts: Vec::new(),
        })
    }

    /// Extract the session UUID from the session directory name.
    pub fn session_id(&self) -> String {
        self.session_dir
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_prefix("sketch-"))
            .unwrap_or("unknown")
            .to_string()
    }

    /// Set up mount namespace for the session.
    /// If we're already in a mount namespace (via unshare --mount), skip creating another.
    /// Otherwise creates a new mount namespace.
    /// Mounts will be made private after the overlay is mounted (see finalize_mounts).
    pub fn setup_namespaces(&self) -> Result<(), String> {
        // Check if we're already in a mount namespace created by unshare
        if std::env::var("SKETCH_IN_UNSHARE").is_ok() {
            // Already in a mount namespace from unshare, don't create another
            return Ok(());
        }

        // Create mount namespace only if not already in one
        unshare(CloneFlags::CLONE_NEWNS)
            .map_err(|e| format!("Failed to create mount namespace: {}", e))?;

        Ok(())
    }

    /// Finalize root structure for user namespaces by bind-mounting essential system directories
    pub fn finalize_root_structure(&self) -> Result<(), String> {
        let in_user_namespace = std::env::var("SKETCH_USER_NAMESPACE").is_ok();

        if in_user_namespace {
            // Bind mount essential system directories from the host root to merged root
            // This provides access to system files without trying to use "/" as lowerdir
            for dir in &["bin", "sbin", "lib", "lib64", "etc", "usr", "var", "opt", "srv"] {
                let host_path = format!("/{}", dir);
                let target = self.merged_dir.join(dir);

                // Only bind mount if the host directory exists
                if Path::new(&host_path).exists() {
                    let _ = mount(
                        Some(host_path.as_str()),
                        &target,
                        None::<&str>,
                        MsFlags::MS_BIND | MsFlags::MS_REC,
                        None::<&str>,
                    );
                }
            }
        }

        Ok(())
    }

    /// Finalize mounts by making them private so our changes don't propagate to the host.
    /// This should be called after the overlay is mounted.
    pub fn finalize_mounts(&self) -> Result<(), String> {
        mount(
            None::<&str>,
            "/",
            None::<&str>,
            MsFlags::MS_REC | MsFlags::MS_PRIVATE,
            None::<&str>,
        )
        .map_err(|e| format!("Failed to make mounts private: {}", e))?;

        Ok(())
    }

    pub fn mount_overlay(&mut self) -> Result<(), String> {
        let is_root = nix::unistd::geteuid().is_root();
        let in_user_namespace = std::env::var("SKETCH_USER_NAMESPACE").is_ok();

        // In user namespaces, "/" can't be used as lowerdir for overlayfs
        // Skip the root overlay mount and rely on mount_additional_filesystems and
        // bind mounts to create the necessary filesystem structure
        if in_user_namespace {
            // Create essential system directories in merged_dir that will be needed for pivot_root
            // These will be bind-mounted from the host in finalize_root_structure()
            for dir in &["bin", "sbin", "lib", "lib64", "etc", "usr", "var", "tmp"] {
                let path = self.merged_dir.join(dir);
                let _ = fs::create_dir_all(&path);
            }
            return Ok(());
        }

        let mut overlay_opts = format!(
            "lowerdir=/,upperdir={},workdir={}",
            self.upper_dir.display(),
            self.work_dir.display()
        );

        // For unprivileged mounting, use userxattr to work with user.overlay.* xattr namespace
        // instead of requiring trusted.overlay.* which needs CAP_SYS_ADMIN
        if !is_root {
            overlay_opts.push_str(",userxattr");
        }

        mount(
            Some("overlay"),
            &self.merged_dir,
            Some("overlay"),
            MsFlags::empty(),
            Some(overlay_opts.as_str()),
        )
        .map_err(|e| format!("Failed to mount overlay: {}", e))?;

        self.mounted = true;
        Ok(())
    }

    pub fn mount_virtual_filesystems(&self) -> Result<(), String> {
        struct VirtualFs {
            fstype: &'static str,
            host_path: &'static str,
            relative_target: &'static str,
        }

        let virtual_filesystems = [
            VirtualFs { fstype: "proc",     host_path: "/proc", relative_target: "proc" },
            VirtualFs { fstype: "sysfs",    host_path: "/sys",  relative_target: "sys" },
            VirtualFs { fstype: "devtmpfs", host_path: "/dev",  relative_target: "dev" },
        ];

        // Mount /proc, /sys, /dev into the merged root.
        // Try a fresh mount first; fall back to bind mount from host.
        for vfs in &virtual_filesystems {
            let target = self.merged_dir.join(vfs.relative_target);
            if target.exists() {
                mount(
                    Some(vfs.fstype),
                    &target,
                    Some(vfs.fstype),
                    MsFlags::empty(),
                    None::<&str>,
                )
                .or_else(|_| {
                    mount(
                        Some(vfs.host_path),
                        &target,
                        None::<&str>,
                        MsFlags::MS_BIND | MsFlags::MS_REC,
                        None::<&str>,
                    )
                })
                .map_err(|e| format!("Failed to mount {} at {}: {}", vfs.fstype, target.display(), e))?;
            }
        }

        // Bind mount /dev/pts if it exists
        let devpts_target = self.merged_dir.join("dev/pts");
        if devpts_target.exists() && Path::new("/dev/pts").exists() {
            let _ = mount(
                Some("/dev/pts"),
                &devpts_target,
                None::<&str>,
                MsFlags::MS_BIND | MsFlags::MS_REC,
                None::<&str>,
            );
        }

        // Bind mount /dev/shm if it exists
        let devshm_target = self.merged_dir.join("dev/shm");
        if devshm_target.exists() && Path::new("/dev/shm").exists() {
            let _ = mount(
                Some("/dev/shm"),
                &devshm_target,
                None::<&str>,
                MsFlags::MS_BIND | MsFlags::MS_REC,
                None::<&str>,
            );
        }

        // Bind mount /run for systemd runtime state (D-Bus, resolved, etc.)
        // This is critical for package managers on systemd-based systems where
        // /etc/resolv.conf is a symlink to /run/systemd/resolve/stub-resolv.conf.
        let run_target = self.merged_dir.join("run");
        if run_target.exists() && Path::new("/run").exists() {
            let _ = mount(
                Some("/run"),
                &run_target,
                None::<&str>,
                MsFlags::MS_BIND | MsFlags::MS_REC,
                None::<&str>,
            );
        }

        Ok(())
    }

    /// Ensure DNS resolution works inside the overlay.
    ///
    /// On many systems /etc/resolv.conf is a symlink into /run/systemd/resolve/,
    /// which we handle by bind-mounting /run above. As a fallback, if the merged
    /// resolv.conf is missing or empty, we copy the host's resolv.conf directly
    /// into the overlay upper directory so the merged view always has it.
    pub fn ensure_dns_resolution(&self) -> Result<(), String> {
        let host_resolv = Path::new("/etc/resolv.conf");
        let merged_resolv = self.merged_dir.join("etc/resolv.conf");

        // If the merged resolv.conf is already readable and non-empty, DNS is fine
        if merged_resolv.exists() {
            if let Ok(contents) = fs::read_to_string(&merged_resolv) {
                if !contents.trim().is_empty() {
                    return Ok(());
                }
            }
        }

        // Copy the host resolv.conf into the upper layer
        if host_resolv.exists() {
            let upper_etc = self.upper_dir.join("etc");
            let _ = fs::create_dir_all(&upper_etc);
            let upper_resolv = upper_etc.join("resolv.conf");

            // Read the real content (follow symlinks)
            match fs::read_to_string(host_resolv) {
                Ok(contents) => {
                    fs::write(&upper_resolv, contents)
                        .map_err(|e| format!("Failed to write resolv.conf to overlay: {}", e))?;
                }
                Err(e) => {
                    return Err(format!("Failed to read host resolv.conf: {}", e));
                }
            }
        }

        Ok(())
    }

    pub fn mount_additional_filesystems(&mut self, verbose: bool) -> Result<(), String> {
        // Filesystems to skip: virtual/pseudo filesystems
        let skip_fstypes = [
            "proc", "sysfs", "devtmpfs", "devpts", "tmpfs", "cgroup", "cgroup2",
            "pstore", "efivarfs", "bpf", "autofs", "hugetlbfs", "mqueue", "fusectl",
            "configfs", "debugfs", "tracefs", "securityfs", "overlay", "nsfs", "ramfs",
            "squashfs",
        ];

        // Mount prefixes to skip
        let skip_prefixes = ["/proc", "/sys", "/dev", "/run", "/tmp"];

        // Read /proc/self/mounts
        let mounts_content = fs::read_to_string("/proc/self/mounts")
            .map_err(|e| format!("Failed to read /proc/self/mounts: {}", e))?;

        for line in mounts_content.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 3 {
                continue; // Malformed line, skip
            }

            let mountpoint = parts[1];
            let fstype = parts[2];

            // Skip virtual/pseudo filesystems
            if skip_fstypes.contains(&fstype) {
                continue;
            }

            // Skip special mount prefixes
            if skip_prefixes.iter().any(|p| mountpoint.starts_with(p)) {
                continue;
            }

            // Skip if it's the session dir itself (avoid recursion)
            if mountpoint.starts_with(self.session_dir.to_string_lossy().as_ref()) {
                continue;
            }

            // Skip the root filesystem
            // - In non-user-namespace mode: already mounted as the main overlay
            // - In user-namespace mode: can't be mounted as overlayfs, rely on other mounts
            if mountpoint == "/" {
                continue;
            }

            // Create overlay upper and work directories for this mount
            // Use hash-based naming to avoid collisions (e.g., /home/user vs /home_user)
            // Create these at the session_dir level, not inside the main work_dir
            // (the main work_dir must stay empty for the root overlay mount)
            let mount_name = mount_name_from_path(mountpoint);
            let mount_upper = self.session_dir.join(format!("upper-{}", mount_name));
            let mount_work = self.session_dir.join(format!("work-{}", mount_name));

            if let Err(e) = fs::create_dir_all(&mount_upper) {
                if verbose {
                    eprintln!(
                        "sketch: warning: failed to create upper dir for {}: {}",
                        mountpoint, e
                    );
                }
                continue;
            }

            if let Err(e) = fs::create_dir_all(&mount_work) {
                if verbose {
                    eprintln!(
                        "sketch: warning: failed to create work dir for {}: {}",
                        mountpoint, e
                    );
                }
                continue;
            }

            // Create target directory in merged_dir
            let target = self
                .merged_dir
                .join(mountpoint.trim_start_matches('/'));

            // Create the target directory if it doesn't exist
            if let Err(e) = fs::create_dir_all(&target) {
                if verbose {
                    eprintln!(
                        "sketch: warning: failed to create directory for mount {}: {}",
                        mountpoint, e
                    );
                }
                continue;
            }

            // Mount overlay for this filesystem
            let is_root = nix::unistd::geteuid().is_root();
            let in_user_namespace = std::env::var("SKETCH_USER_NAMESPACE").is_ok();
            let mut overlay_opts = format!(
                "lowerdir={},upperdir={},workdir={}",
                mountpoint,
                mount_upper.display(),
                mount_work.display()
            );

            // For unprivileged mounting, use userxattr
            if !is_root || in_user_namespace {
                overlay_opts.push_str(",userxattr");
            }

            match mount(
                Some("overlay"),
                &target,
                Some("overlay"),
                MsFlags::empty(),
                Some(overlay_opts.as_str()),
            ) {
                Ok(_) => {
                    if verbose {
                        eprintln!("sketch: mounted overlay for {} at {}", mountpoint, target.display());
                    }
                    self.extra_mounts.push(target);
                }
                Err(e) => {
                    if verbose {
                        eprintln!(
                            "sketch: warning: failed to mount overlay for {} at {}: {}",
                            mountpoint,
                            target.display(),
                            e
                        );
                    }
                    // Don't abort; skip this mount and continue with others
                }
            }
        }

        Ok(())
    }

    pub fn pivot_root(&self) -> Result<(), String> {
        let old_root = self.merged_dir.join("mnt");
        fs::create_dir_all(&old_root)
            .map_err(|e| format!("Failed to create old root dir: {}", e))?;

        nix::unistd::pivot_root(&self.merged_dir, &old_root)
            .map_err(|e| format!("Failed to pivot_root: {}", e))?;

        std::env::set_current_dir("/")
            .map_err(|e| format!("Failed to chdir to /: {}", e))?;

        // Unmount old root lazily
        umount2("/mnt", MntFlags::MNT_DETACH)
            .map_err(|e| format!("Failed to unmount old root: {}", e))?;

        Ok(())
    }

    pub fn cleanup(&mut self) {
        if self.mounted {
            // Unmount extra mounts first (best effort)
            for path in self.extra_mounts.drain(..) {
                let _ = umount2(&path, MntFlags::MNT_DETACH);
            }

            // Unmount virtual filesystems (best effort)
            for path in &["run", "dev/shm", "dev/pts", "dev", "sys", "proc"] {
                let target = self.merged_dir.join(path);
                let _ = umount2(&target, MntFlags::MNT_DETACH);
            }

            // Unmount overlay
            let _ = umount2(&self.merged_dir, MntFlags::MNT_DETACH);
            self.mounted = false;
        }

        // Remove temp directories - retry if it fails the first time
        // Some systems may need a brief delay after unmounting before removal succeeds
        if fs::remove_dir_all(&self.session_dir).is_err() {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let _ = fs::remove_dir_all(&self.session_dir);
        }
    }
}

impl Drop for OverlaySession {
    fn drop(&mut self) {
        self.cleanup();
    }
}

/// Tests for mount name generation
#[cfg(test)]
mod tests {
    use super::mount_name_from_path;

    #[test]
    fn test_mount_name_is_deterministic() {
        // Same path should always generate the same hash
        let name1 = mount_name_from_path("/home/user");
        let name2 = mount_name_from_path("/home/user");
        assert_eq!(name1, name2, "hash should be deterministic");
    }

    #[test]
    fn test_mount_name_avoids_collisions() {
        // Paths that would collide with simple replace('/', "_") should differ
        let name_home_user = mount_name_from_path("/home/user");
        let name_home_user_flat = mount_name_from_path("/home_user");
        assert_ne!(
            name_home_user, name_home_user_flat,
            "hash should distinguish between /home/user and /home_user"
        );
    }

    #[test]
    fn test_mount_name_is_reasonable_length() {
        let name = mount_name_from_path("/home");
        assert_eq!(name.len(), 12, "hash should be 12 hex characters (6 bytes)");
    }

    #[test]
    fn test_mount_name_different_paths() {
        let home = mount_name_from_path("/home");
        let etc = mount_name_from_path("/etc");
        let var = mount_name_from_path("/var");

        // All different paths should have different hashes
        assert_ne!(home, etc);
        assert_ne!(etc, var);
        assert_ne!(home, var);
    }
}

/// Clean up orphaned sketch overlay mounts and temp directories
/// Only removes sessions if the process is no longer alive
pub fn clean_orphaned() -> io::Result<u32> {
    use std::thread;
    use std::time::Duration;

    let mut cleaned = 0;
    let tmp = Path::new("/tmp");

    if let Ok(entries) = fs::read_dir(tmp) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("sketch-") {
                let path = entry.path();

                // Check if session is actually stale by reading metadata
                let metadata_path = path.join(".sketch-metadata.json");
                if let Ok(metadata) = fs::read_to_string(&metadata_path) {
                    if let Ok(meta) = serde_json::from_str::<crate::metadata::SessionMetadata>(&metadata) {
                        // Only clean up stale sessions (process no longer exists)
                        if !meta.is_alive() {
                            let merged = path.join("merged");

                            // Aggressively unmount everything
                            let _ = umount2(&merged, MntFlags::MNT_DETACH);

                            // Try to remove the directory
                            // Retry once after a brief delay if it fails
                            if fs::remove_dir_all(&path).is_err() {
                                thread::sleep(Duration::from_millis(50));
                                if fs::remove_dir_all(&path).is_ok() {
                                    cleaned += 1;
                                }
                            } else {
                                cleaned += 1;
                            }
                        }
                    }
                } else {
                    // If we can't read metadata, try to clean anyway
                    let merged = path.join("merged");
                    let _ = umount2(&merged, MntFlags::MNT_DETACH);
                    if fs::remove_dir_all(&path).is_ok() {
                        cleaned += 1;
                    }
                }
            }
        }
    }

    Ok(cleaned)
}

use nix::mount::{mount, umount2, MntFlags, MsFlags};
use nix::sched::{unshare, CloneFlags};
use nix::unistd::{sethostname, gethostname};
use core::result::Result;
use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use crate::utils::{fnv1a_hash, session_id};

pub struct OverlaySession {
    pub session_id: String,
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
pub fn mount_name_from_path(mountpoint: &str) -> String {
    let hash = fnv1a_hash(mountpoint.as_bytes());
    format!("{:08x}", hash)
}

impl OverlaySession {
    pub fn new() -> io::Result<Self> {
        let session_id = session_id().to_string();
        let session_dir = PathBuf::from(format!("/tmp/sketch-{}", session_id));

        let upper_dir = session_dir.join("upper");
        let work_dir = session_dir.join("work");
        let merged_dir = session_dir.join("merged");

        fs::create_dir_all(&upper_dir)?;
        fs::create_dir_all(&work_dir)?;
        fs::create_dir_all(&merged_dir)?;

        Ok(Self {
            session_id: session_id,
            session_dir,
            upper_dir,
            work_dir,
            merged_dir,
            mounted: false,
            extra_mounts: Vec::new(),
        })
    }

    /// Extract the session UUID from the session directory name.
    fn session_id(&self) -> String {
        self.session_dir
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_prefix("sketch-"))
            .unwrap_or("unknown")
            .to_string()
    }

    /// Set up mount namespace for the session.
    /// Creates a new mount namespace for filesystem isolation.
    pub fn setup_namespaces(&self) -> Result<(), String> {
        unshare(CloneFlags::CLONE_NEWNS)
            .map_err(|e| format!("Failed to create mount namespace: {}", e))?;

        unshare(CloneFlags::CLONE_NEWUTS)
            .map_err(|e| format!("Failed to create UTS namespace: {}", e))?;

        Ok(())
    }

    pub fn change_hostname(&self) -> Result<(), String> {
        let hostname = format!("sketch-{}", self.session_id());
        sethostname(&hostname)
            .map_err(|e| format!("Failed to set hostname: {}", e))?;
        Ok(())
    }

    pub fn add_hostname_entry(&self) -> Result<(), String> {
        let hosts_path = self.merged_dir.join("etc/hosts");
        let hostname = gethostname()
            .map_err(|e| format!("Failed to get hostname: {}", e))?
            .into_string()
            .map_err(|_| "Hostname wasn't valid UTF-8".to_string())?;

        let entry = format!("127.0.0.1\t{}\n", hostname);

        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&hosts_path)
            .map_err(|e| format!("Failed to open hosts file: {}", e))?;

        io::Write::write_all(&mut file, entry.as_bytes())
            .map_err(|e| format!("Failed to write hostname entry: {}", e))?;

        Ok(())
    }

    pub fn make_mount_private(&self) -> Result<(), String> {
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
        let overlay_opts = format!(
            "lowerdir=/,upperdir={},workdir={}",
            self.upper_dir.display(),
            self.work_dir.display()
        );

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
            VirtualFs {
                fstype: "proc",
                host_path: "/proc",
                relative_target: "proc",
            },
            VirtualFs {
                fstype: "sysfs",
                host_path: "/sys",
                relative_target: "sys",
            },
            VirtualFs {
                fstype: "devtmpfs",
                host_path: "/dev",
                relative_target: "dev",
            },
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
                .map_err(|e| {
                    format!(
                        "Failed to mount {} at {}: {}",
                        vfs.fstype,
                        target.display(),
                        e
                    )
                })?;
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

    pub fn mount_additional_filesystems(&mut self, verbose: bool) -> Result<(), String> {
        // Filesystems to skip: virtual/pseudo filesystems
        let skip_fstypes = [
            "proc",
            "sysfs",
            "devtmpfs",
            "devpts",
            "tmpfs",
            "cgroup",
            "cgroup2",
            "pstore",
            "efivarfs",
            "bpf",
            "autofs",
            "hugetlbfs",
            "mqueue",
            "fusectl",
            "configfs",
            "debugfs",
            "tracefs",
            "securityfs",
            "overlay",
            "nsfs",
            "ramfs",
            "squashfs",
        ];

        // Mount prefixes to skip
        let skip_prefixes = ["/proc", "/sys", "/dev", "/run", "/tmp", "/boot"];

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
            let target = self.merged_dir.join(mountpoint.trim_start_matches('/'));

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
            let overlay_opts = format!(
                "lowerdir={},upperdir={},workdir={}",
                mountpoint,
                mount_upper.display(),
                mount_work.display()
            );

            match mount(
                Some("overlay"),
                &target,
                Some("overlay"),
                MsFlags::empty(),
                Some(overlay_opts.as_str()),
            ) {
                Ok(_) => {
                    if verbose {
                        eprintln!(
                            "sketch: mounted overlay for {} at {}",
                            mountpoint,
                            target.display()
                        );
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

    pub fn bind_x11_sock(&self) -> Result<(), String> {
        let x11_sock = "/tmp/.X11-unix";
        let x11_sock_merged = self.merged_dir.join(x11_sock.trim_start_matches("/"));
        fs::create_dir_all(&x11_sock_merged)
            .map_err(|e| format!("Failed to create X11 socket dir in merged: {}", e))?;

        mount(
            Some("/tmp/.X11-unix"),
            &x11_sock_merged,
            None::<&str>,
            MsFlags::MS_BIND | MsFlags::MS_REC,
            None::<&str>,
        ).map_err(|e| format!("Failed to mount X11 socker: {}", e))?;

        Ok(())
    }

    pub fn pivot_root(&self) -> Result<(), String> {
        let old_root = self.merged_dir.join("mnt");
        fs::create_dir_all(&old_root)
            .map_err(|e| format!("Failed to create old root dir: {}", e))?;

        nix::unistd::pivot_root(&self.merged_dir, &old_root)
            .map_err(|e| format!("Failed to pivot_root: {}", e))?;

        std::env::set_current_dir("/").map_err(|e| format!("Failed to chdir to /: {}", e))?;

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
                    if let Ok(meta) =
                        serde_json::from_str::<crate::metadata::SessionMetadata>(&metadata)
                    {
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

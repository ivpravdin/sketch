use nix::mount::{mount, umount2, MntFlags, MsFlags};
use nix::sched::{unshare, CloneFlags};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub struct OverlaySession {
    pub session_dir: PathBuf,
    pub upper_dir: PathBuf,
    pub work_dir: PathBuf,
    pub merged_dir: PathBuf,
    mounted: bool,
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

    /// Set up namespaces for the session.
    ///
    /// If running as root, only a mount namespace is needed.
    /// If running as non-root, a user namespace is created first to gain the
    /// capabilities needed for OverlayFS mounting and pivot_root.
    pub fn setup_namespaces(&self) -> Result<(), String> {
        let is_root = nix::unistd::geteuid().is_root();

        if !is_root {
            // Non-root: create user namespace first for privilege elevation
            let real_uid = nix::unistd::getuid().as_raw();
            let real_gid = nix::unistd::getgid().as_raw();
            crate::userns::setup_user_namespace(real_uid, real_gid)
                .map_err(|e| format!(
                    "Failed to set up user namespace: {}. Try running with sudo.",
                    e,
                ))?;
        }

        // Create mount namespace (works after user namespace gives us CAP_SYS_ADMIN)
        unshare(CloneFlags::CLONE_NEWNS)
            .map_err(|e| format!("Failed to create mount namespace: {}", e))?;

        // Make all mounts private so our changes don't propagate to the host
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
            // Unmount virtual filesystems first (best effort)
            for path in &["run", "dev/shm", "dev/pts", "dev", "sys", "proc"] {
                let target = self.merged_dir.join(path);
                let _ = umount2(&target, MntFlags::MNT_DETACH);
            }

            // Unmount overlay
            let _ = umount2(&self.merged_dir, MntFlags::MNT_DETACH);
            self.mounted = false;
        }

        // Remove temp directories
        let _ = fs::remove_dir_all(&self.session_dir);
    }
}

impl Drop for OverlaySession {
    fn drop(&mut self) {
        self.cleanup();
    }
}

/// Clean up orphaned sketch overlay mounts and temp directories
pub fn clean_orphaned() -> io::Result<u32> {
    let mut cleaned = 0;
    let tmp = Path::new("/tmp");

    if let Ok(entries) = fs::read_dir(tmp) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("sketch-") {
                let path = entry.path();
                let merged = path.join("merged");

                // Try to unmount if still mounted
                let _ = umount2(&merged, MntFlags::MNT_DETACH);

                // Remove the directory tree
                if fs::remove_dir_all(&path).is_ok() {
                    cleaned += 1;
                }
            }
        }
    }

    Ok(cleaned)
}

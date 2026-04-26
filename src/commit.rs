use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::io::Write;
use std::path::PathBuf;
use std::os::linux::fs::MetadataExt;

use nix::libc::DT_DIR;

use crate::overlay::OverlaySession;
use crate::cli::Config;

pub fn handle_commit(files: &[String]) -> Result<(), String> {
    // Check if we're running inside a sketch session
    let session_file_path = "/var/.sketch-session";

    if !std::path::Path::new(session_file_path).exists() {
        return Err(
            "'sketch commit' can only be run inside an active sketch session.\n\
             Use it within a session to mark files for persistence."
                .into(),
        );
    }

    // Write commit list inside the session (in overlay, not in /tmp/sketch-xxx)
    // This goes into the overlay upper directory where parent can access it
    // Use /var for metadata since it's a standard location for such files
    let commit_list_path = "/tmp/.sketch-commit";

    let mut commit_file = if PathBuf::from(commit_list_path).exists() {
        std::fs::OpenOptions::new()
            .append(true)
            .open(&commit_list_path)
            .map_err(|e| format!("Failed to open commit list: {}", e))?
    } else {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&commit_list_path)
            .map_err(|e| format!("Failed to create commit list: {}", e))?;
        std::fs::set_permissions(&commit_list_path, std::fs::Permissions::from_mode(0o666))
            .map_err(|e| format!("Failed to set permissions on commit list: {}", e))?;
        file
    };

    // Build a list of mount points from /proc/mounts for finding which mount each file belongs to
    let mount_points = get_mount_points()?;

    for file_path in files {
        // Resolve relative paths to absolute paths
        let abs_path = if PathBuf::from(file_path).is_absolute() {
            PathBuf::from(file_path)
        } else {
            // Relative path: resolve against current directory
            match std::env::current_dir() {
                Ok(cwd) => cwd.join(file_path),
                Err(e) => {
                    return Err(format!(
                        "Failed to resolve path '{}': couldn't get current dir: {}",
                        file_path, e
                    ));
                }
            }
        };

        // Check if the file exists in the overlayfs (in the current merged view)
        if !abs_path.exists() {
            return Err(format!(
                "File does not exist in overlayfs: {}",
                abs_path.display()
            ));
        }

        // Find the longest matching mount point for this file
        let mount_point = find_mount_for_path(&abs_path, &mount_points)?;

        let abs_path_str = abs_path.to_string_lossy().to_string();
        writeln!(commit_file, "{}|{}", mount_point, abs_path_str)
            .map_err(|e| format!("Failed to write to commit list: {}", e))?;
        println!("sketch: marked for commit: {}", abs_path_str);
    }

    Ok(())
}

/// Process files marked for commitment from the overlay to the base filesystem.
pub fn commit_marked_files(config: &Config, overlay: &OverlaySession) {
    // The commit list is written inside the session (at /var/.sketch-commit)
    // which goes into the overlay upper directory
    let commit_list_in_upper = overlay.upper_dir.join("tmp/.sketch-commit");

    // Check if a commit list exists
    if !commit_list_in_upper.exists() {
        if config.verbose {
            eprintln!("sketch: no marked files to commit");
        }
        return;
    }

    // Read the commit list from the overlay upper directory
    match std::fs::read_to_string(&commit_list_in_upper) {
        Ok(content) => {
            let mut committed_count = 0;
            let mut missing_count = 0;

            for line in content.lines() {
                let entry = line.trim();
                if entry.is_empty() {
                    continue;
                }

                // Parse the new format: MOUNTPOINT|FILE_PATH
                // e.g., "/home|/home/user/.bashrc" or "/|/etc/nginx.conf"
                let parts: Vec<&str> = entry.splitn(2, '|').collect();
                if parts.len() != 2 {
                    eprintln!(
                        "sketch: warning: invalid commit list entry (no mount): {}",
                        entry
                    );
                    continue;
                }

                let mount_point = parts[0];
                let file_path = parts[1];

                // Find the correct upper directory for this mount point
                let upper_dir = if mount_point == "/" {
                    // Root overlay
                    overlay.upper_dir.clone()
                } else {
                    // Extra mount: compute the upper directory path
                    let mount_hash = crate::overlay::mount_name_from_path(mount_point);
                    overlay
                        .session_dir
                        .join(format!("upper-{}", mount_hash))
                };

                // Compute relative path within the mount point
                let rel_path = if mount_point == "/" {
                    file_path.trim_start_matches('/')
                } else {
                    // Remove the mount_point prefix from file_path
                    file_path
                        .strip_prefix(mount_point)
                        .unwrap_or(file_path)
                        .trim_start_matches('/')
                };

                let upper_file = upper_dir.join(rel_path);

                // Check if the file exists in the overlay
                if upper_file.exists() {
                    let metadata = match std::fs::metadata(&upper_file) {
                        Ok(m) => m,
                        Err(e) => {
                            eprintln!("sketch: warning: failed to get metadata for {}: {}", upper_file.display(), e);
                            missing_count += 1;
                            continue;
                        }
                    };

                    if upper_file.is_dir() {
                        // recurisvely commit directory
                        match commit_dir(&upper_file, file_path) {
                            Ok(c) => {
                                committed_count += c;
                            }
                            Err(e) => {
                                eprintln!("sketch: warning: failed to commit directory {}: {}", file_path, e);
                                missing_count += 1;
                            }
                        }

                    } else {
                        match commit_file(&upper_file, file_path) {
                            Ok(_) => {
                                committed_count += 1;
                            }
                            Err(e) => {
                                eprintln!("sketch: warning: failed to commit {}: {}", file_path, e);
                                missing_count += 1;
                            }
                        }
                    }
                } else {
                    eprintln!(
                        "sketch: warning: marked file does not exist in overlay: {}",
                        file_path
                    );
                    missing_count += 1;
                }
            }

            if config.verbose {
                if committed_count > 0 {
                    eprintln!("sketch: committed {} file(s)", committed_count);
                }
                if missing_count > 0 {
                    eprintln!("sketch: {} marked file(s) were not found", missing_count);
                }
            }
        }
        Err(e) => {
            eprintln!("sketch: warning: failed to read commit list: {}", e);
        }
    }
}

fn commit_file(upper_file: &PathBuf, file_path: &str) -> Result<(), String> {
    let metadata = match std::fs::metadata(&upper_file) {
                        Ok(m) => m,
                        Err(e) => {
                            return Err(format!("Failed to get metadata for {}: {}", upper_file.display(), e));
                        }
                    };

    match std::fs::copy(upper_file, file_path) {
        Ok(_) => (),
        Err(e) => return Err(format!("Failed to commit {}: {}", file_path, e)),
    };

    match nix::unistd::chown(file_path, Some(nix::unistd::Uid::from_raw(metadata.st_uid())), Some(nix::unistd::Gid::from_raw(metadata.st_gid()))) {
                            Ok(_) => (),
                            Err(e) => {
                                return Err(format!("Failed to set file owner {}: {}", file_path, e))
                            }
                        };

    Ok(())
}

fn commit_dir(upper_dir: &PathBuf, dir_path: &str) -> Result<i32, String> {
    let mut count = 0;

    let metadata = match std::fs::metadata(&upper_dir) {
                        Ok(m) => m,
                        Err(e) => {
                            return Err(format!("Failed to get metadata for {}: {}", upper_dir.display(), e));
                        }
                    };
    
    fs::create_dir_all(dir_path)
        .map_err(|e| format!("Failed to create directory {}: {}", dir_path, e))?;

    for entry in fs::read_dir(upper_dir).map_err(|e| format!("{}", e))? {
        let path = entry.map_err(|e| format!("{}", e))?.path();
        let target_path = format!("{}/{}", dir_path, path.file_name().unwrap().to_string_lossy());
        if path.is_dir() {
            count += commit_dir(&path, &target_path)?;
        } else {
            commit_file(&path, &target_path)?;
            count += 1;
        }
    }

    match nix::unistd::chown(dir_path, Some(nix::unistd::Uid::from_raw(metadata.st_uid())), Some(nix::unistd::Gid::from_raw(metadata.st_gid()))) {
                            Ok(_) => (),
                            Err(e) => {
                                return Err(format!("Failed to set file owner {}: {}", dir_path, e))
                            }
                        };

    Ok(count)
}

/// Parse /proc/mounts and return a sorted list of mount points (longest first)
fn get_mount_points() -> Result<Vec<String>, String> {
    let mounts_content = std::fs::read_to_string("/proc/mounts")
        .map_err(|e| format!("Failed to read /proc/mounts: {}", e))?;

    let mut mount_points: Vec<String> = Vec::new();
    for line in mounts_content.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            mount_points.push(parts[1].to_string());
        }
    }

    // Sort by length descending so we match the longest (most specific) mount point first
    mount_points.sort_by(|a, b| b.len().cmp(&a.len()));
    Ok(mount_points)
}

/// Find the longest matching mount point for a file path
fn find_mount_for_path(
    path: &std::path::PathBuf,
    mount_points: &[String],
) -> Result<String, String> {
    let path_str = path.to_string_lossy();

    for mount in mount_points {
        if path_str.starts_with(mount) {
            // Make sure it's a complete path component match (not partial)
            // e.g., /home matches /home/user but not /homex
            if mount == "/"
                || path_str.len() == mount.len()
                || path_str.as_bytes()[mount.len()] == b'/'
            {
                return Ok(mount.clone());
            }
        }
    }

    // Fallback to root mount if no match found
    Ok("/".to_string())
}
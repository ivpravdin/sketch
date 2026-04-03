mod cli;
mod metadata;
mod overlay;
mod session;
mod utils;

use std::process;

fn main() {
    let config = cli::parse_args();

    // Check privileges: need root for session commands
    if !nix::unistd::geteuid().is_root() {
        match &config.command {
            cli::Command::Clean | cli::Command::List(_) | cli::Command::Status => {}
            _ => {
                eprintln!("sketch: must be run as root (try: sudo sketch)");
                process::exit(1);
            }
        }
    }

    match &config.command {
        cli::Command::Shell => {
            let session = match session::Session::new(&config) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("sketch: {}", e);
                    process::exit(1);
                }
            };
            match session.start_shell() {
                Ok(code) => process::exit(code),
                Err(e) => {
                    eprintln!("sketch: {}", e);
                    process::exit(1);
                }
            }
        }
        cli::Command::Exec(args) => {
            let session = match session::Session::new(&config) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("sketch: {}", e);
                    process::exit(1);
                }
            };
            match session.start_exec(&args) {
                Ok(code) => process::exit(code),
                Err(e) => {
                    eprintln!("sketch: {}", e);
                    process::exit(1);
                }
            }
        }
        cli::Command::Run(args, options) => {
            let session = match session::Session::new(&config) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("sketch: {}", e);
                    process::exit(1);
                }
            };
            match session.start_run() {
                Ok(code) => process::exit(code),
                Err(e) => {
                    eprintln!("sketch: {}", e);
                    process::exit(1);
                }
            }
        }
        cli::Command::Commit(files) => match handle_commit(&files) {
            Ok(()) => {}
            Err(e) => {
                eprintln!("sketch: {}", e);
                process::exit(1);
            }
        }
        cli::Command::List(options) => {
            let sessions = metadata::list_sessions();
            if sessions.is_empty() {
                if options.json {
                    println!("[]");
                } else {
                    println!("No active sessions.");
                }
                return;
            }
            if options.json {
                match serde_json::to_string_pretty(&sessions) {
                    Ok(json) => println!("{}", json),
                    Err(e) => {
                        eprintln!("sketch: failed to serialize sessions: {}", e);
                        process::exit(1);
                    }
                }
            } else {
                println!(
                    "{:<38}  {:<12}  {:>6}  {:>7}  {:>8}  {}",
                    "SESSION ID", "NAME", "PID", "STATUS", "AGE", "COMMAND"
                );
                for s in &sessions {
                    let status = if s.is_alive() { "active" } else { "stale" };
                    let name = s.name.as_deref().unwrap_or("-");
                    let size = metadata::session_size(
                        &metadata::get_session_dir(&s.id).unwrap_or_default(),
                    );
                    let size_str = metadata::format_size(size);
                    let cmd_display = if s.command.len() > 30 {
                        format!("{}...", &s.command[..27])
                    } else {
                        s.command.clone()
                    };
                    println!(
                        "{:<38}  {:<12}  {:>6}  {:>7}  {:>8}  {}",
                        s.id,
                        name,
                        s.pid,
                        status,
                        s.format_age(),
                        cmd_display,
                    );
                    if config.verbose {
                        eprintln!("  size: {}, path: {}", size_str, s.overlay_path);
                    }
                }
            }
        }
        cli::Command::Status => {
            print_status();
        }
        cli::Command::Clean => match overlay::clean_orphaned() {
            Ok(0) => println!("sketch: no orphaned sessions found"),
            Ok(n) => println!("sketch: cleaned up {} orphaned session(s)", n),
            Err(e) => {
                eprintln!("sketch: cleanup failed: {}", e);
                process::exit(1);
            }
        },
    }
}

fn handle_commit(files: &[String]) -> Result<(), String> {
    // Check if we're running inside a sketch session
    if std::env::var("SKETCH_SESSION").is_err() {
        return Err(
            "'sketch commit' can only be run inside an active sketch session.\n\
             Use it within a session to mark files for persistence."
                .into(),
        );
    }

    // Write commit list inside the session (in overlay, not in /tmp/sketch-xxx)
    // This goes into the overlay upper directory where parent can access it
    // Use /var for metadata since it's a standard location for such files
    let commit_list_path = "/var/.sketch-commit";

    // Append files to the commit list
    use std::io::Write;
    use std::path::PathBuf;

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&commit_list_path)
        .map_err(|e| format!("Failed to open commit list: {}", e))?;

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
        writeln!(file, "{}|{}", mount_point, abs_path_str)
            .map_err(|e| format!("Failed to write to commit list: {}", e))?;
        println!("sketch: marked for commit: {}", abs_path_str);
    }

    Ok(())
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

fn print_status() {
    println!("sketch - ephemeral session system\n");

    // OverlayFS support
    let overlayfs = std::fs::read_to_string("/proc/filesystems")
        .map(|s| s.contains("overlay"))
        .unwrap_or(false);
    println!("System:");
    if let Ok(ver) = std::fs::read_to_string("/proc/version") {
        if let Some(kver) = ver.split_whitespace().nth(2) {
            println!("  Kernel:              {}", kver);
        }
    }
    println!(
        "  OverlayFS:           {}",
        if overlayfs {
            "available"
        } else {
            "not available"
        }
    );

    // Disk space
    println!("\nDisk:");
    if let Ok(stat) = nix::sys::statvfs::statvfs("/tmp") {
        let avail = stat.blocks_available() * stat.fragment_size();
        let total = stat.blocks() * stat.fragment_size();
        let pct = if total > 0 {
            100 - (avail * 100 / total)
        } else {
            0
        };
        println!(
            "  /tmp available:      {} ({}% used)",
            metadata::format_size(avail),
            pct
        );
    }

    // Sessions
    let sessions = metadata::list_sessions();
    let active = sessions.iter().filter(|s| s.is_alive()).count();
    let stale = sessions.len() - active;
    println!("\nSessions:");
    println!("  Active:              {}", active);
    if stale > 0 {
        println!(
            "  Stale:               {} (run 'sketch --clean' to remove)",
            stale
        );
    }

    // Privileges
    println!("\nPrivileges:");
    let is_root = nix::unistd::geteuid().is_root();
    println!(
        "  Running as root:     {}",
        if is_root { "yes" } else { "no" }
    );
}

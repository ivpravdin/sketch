mod cli;
mod commit;
mod fs_utils;
mod metadata;
mod overlay;
mod package;
mod session;
mod userns;

use std::process;

fn main() {
    let config = cli::parse_args();

    // Check privileges: need root OR user namespace support for session commands
    if !nix::unistd::geteuid().is_root() {
        match config.command {
            cli::Command::Clean | cli::Command::List(_) | cli::Command::Status => {}
            _ => {
                if !userns::can_use_user_namespaces() {
                    eprintln!("sketch: must be run as root (try: sudo sketch)");
                    eprintln!("sketch: tip: user namespaces not available on this system (requires kernel 5.11+)");
                    process::exit(1);
                }
                if config.verbose {
                    eprintln!("sketch: running without root via user namespaces");
                }
            }
        }
    }

    match config.command {
        cli::Command::Shell => {
            let session = match session::Session::new(config.verbose) {
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
            let session = match session::Session::new(config.verbose) {
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
            let session = match session::Session::new(config.verbose) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("sketch: {}", e);
                    process::exit(1);
                }
            };
            match session.start_run(&args, &options) {
                Ok(code) => process::exit(code),
                Err(e) => {
                    eprintln!("sketch: {}", e);
                    process::exit(1);
                }
            }
        }
        cli::Command::Commit(files) => {
            match handle_commit(&files) {
                Ok(()) => {},
                Err(e) => {
                    eprintln!("sketch: {}", e);
                    process::exit(1);
                }
            }
        }
        cli::Command::Attach(session_id) => {
            match session::Session::attach_existing(&session_id, config.verbose) {
                Ok(session) => {
                    match session.start_shell() {
                        Ok(code) => process::exit(code),
                        Err(e) => {
                            eprintln!("sketch: {}", e);
                            process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("sketch: {}", e);
                    process::exit(1);
                }
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
        cli::Command::Clean => {
            match overlay::clean_orphaned() {
                Ok(0) => println!("sketch: no orphaned sessions found"),
                Ok(n) => println!("sketch: cleaned up {} orphaned session(s)", n),
                Err(e) => {
                    eprintln!("sketch: cleanup failed: {}", e);
                    process::exit(1);
                }
            }
        }
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

    let session_dir = std::env::var("SKETCH_SESSION_DIR").map_err(|_| {
        "SKETCH_SESSION_DIR environment variable not set".to_string()
    })?;

    use std::path::Path;
    let commit_list_path = Path::new(&session_dir).join(".sketch-commit");

    // Append files to the commit list
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&commit_list_path)
        .map_err(|e| format!("Failed to open commit list: {}", e))?;

    for file_path in files {
        writeln!(file, "{}", file_path)
            .map_err(|e| format!("Failed to write to commit list: {}", e))?;
        println!("sketch: marked for commit: {}", file_path);
    }

    Ok(())
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
        if overlayfs { "available" } else { "not available" }
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
        println!("  Stale:               {} (run 'sketch --clean' to remove)", stale);
    }

    // Privileges
    println!("\nPrivileges:");
    let is_root = nix::unistd::geteuid().is_root();
    println!(
        "  Running as root:     {}",
        if is_root { "yes" } else { "no" }
    );

    // User namespace support
    let userns = std::fs::read_to_string("/proc/sys/user/max_user_namespaces")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .map(|n| n > 0)
        .unwrap_or(false);
    println!(
        "  User namespaces:     {}",
        if userns {
            "available"
        } else {
            "not available"
        }
    );

    // Package manager
    println!("\nPackage manager:");
    if let Some(pm) = package::detect_package_manager() {
        println!("  System:              {}", pm.name());
    } else {
        println!("  System:              none detected");
    }
    let user_pms = package::detect_user_package_managers();
    if !user_pms.is_empty() {
        println!("  User-level:          {}", user_pms.join(", "));
    }
}

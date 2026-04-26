mod cli;
mod metadata;
mod overlay;
mod session;
mod utils;
mod commit;

use std::process;
use crate::commit::handle_commit;

fn is_root() -> bool {
    nix::unistd::geteuid().is_root()
}
fn main() {
    let config = cli::parse_args();

    match &config.command {
        cli::Command::Shell => {
            if !is_root() {
                eprintln!("sketch: 'shell' command requires root privileges");
                process::exit(1);
            }
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
        cli::Command::Run(_, _) => {
            if !is_root() {
                eprintln!("sketch: 'run' command requires root privileges");
                process::exit(1);
            }
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
        cli::Command::Clean => {
            if !is_root() {
                eprintln!("sketch: 'clean' command requires root privileges");
                process::exit(1);
            }
            match overlay::clean_orphaned() {
                Ok(0) => println!("sketch: no orphaned sessions found"),
                Ok(n) => println!("sketch: cleaned up {} orphaned session(s)", n),
                Err(e) => {
                    eprintln!("sketch: cleanup failed: {}", e);
                    process::exit(1);
                }
            }
        },
    }
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

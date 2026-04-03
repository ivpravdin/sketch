use nix::sched::{unshare, CloneFlags};
use nix::sys::signal::Signal;
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::{fork, ForkResult, Pid};
use std::{env, process};

use crate::cli::{Config, Command, RunOptions};
use crate::metadata::SessionMetadata;
use crate::overlay::OverlaySession;

pub struct Session<'a> {
    overlay: OverlaySession,
    original_cwd: std::path::PathBuf,
    original_uid: u32,
    original_gid: u32,
    config: &'a Config,
}

impl<'a> Session<'a> {

    pub fn new(config: &'a Config) -> Result<Self, String> {

        let overlay = OverlaySession::new()
            .map_err(|e| format!("Failed to create session directories: {}", e))?;

        let original_cwd = std::env::current_dir().unwrap_or_else(|_| "/".into());
        let original_uid = nix::unistd::getuid().as_raw();
        let original_gid = nix::unistd::getgid().as_raw();

        if config.verbose {
            eprintln!("sketch: session dir: {}", overlay.session_dir.display());
        }

        Ok(Self {
            overlay,
            original_cwd,
            original_uid,
            original_gid,
            config,
        })
    }

    pub fn start_shell(mut self) -> Result<i32, String> {
        self.write_metadata(self.config.name.as_ref(), "shell")?;
        self.setup()?;
        let shell = detect_shell();
        self.run_command()
    }

    pub fn start_exec(mut self, args: &[String]) -> Result<i32, String> {
        if args.is_empty() {
            return Err("No command specified".into());
        }
        self.write_metadata(None, &format!("exec {}", args.join(" ")))?;
        self.setup()?;
        let cmd = &args[0];
        let cmd_args: Vec<&str> = args[1..].iter().map(|s| s.as_str()).collect();
        self.run_command()
    }

    pub fn start_run(mut self) -> Result<i32, String> {
        self.setup()?;

        let Command::Run(args, run_opts) = &self.config.command else {
            return Err("Invalid command type for start_run".into());
        };

        let cmd_str = format!("run {}", args.join(" "));
        self.write_metadata(self.config.name.as_ref(), &cmd_str)?;

        self.run_command()
    }

    /// Write session metadata to disk for `sketch list` to discover.
    fn write_metadata(&self, name: Option<&String>, command: &str) -> Result<(), String> {
        let meta = SessionMetadata::new(
            &self.overlay.session_id,
            name.map(|s| s.clone()),
            command,
            &self.overlay.session_dir,
        );
        meta.save(&self.overlay.session_dir)
    }

    fn setup(&mut self) -> Result<(), String> {
        if self.config.verbose {
            eprintln!("sketch: creating mount namespace...");
        }
        self.overlay.setup_namespaces()?;

        if self.config.verbose {
            eprintln!("sketch: making root mount private...");
        }
        self.overlay.make_mount_private()?;

        if self.config.verbose {
            eprintln!("sketch: chaning hostname...");
        }
        self.overlay.change_hostname()?;

        if self.config.verbose {
            eprintln!("sketch: mounting overlay filesystem...");
        }
        self.overlay.mount_overlay()?;

        if self.config.verbose {
            eprintln!("sketch: mounting virtual filesystems...");
        }
        self.overlay.mount_virtual_filesystems()?;

        if self.config.verbose {
            eprintln!("sketch: mounting additional partitions...");
        }
        self.overlay.mount_additional_filesystems(self.config.verbose)?;

        if self.config.verbose {
            eprintln!("sketch: adding hostname entry to /etc/hosts...");
        }
        self.overlay.add_hostname_entry()?;

        if self.config.x11 {
            if self.config.verbose {
                eprintln!("sketch: binding X11 socket...");
            }
            self.overlay.bind_x11_sock()?;
        }

        Ok(())
    }

    /// Process files marked for commitment from the overlay to the base filesystem.
    fn commit_marked_files(&self) {
        // The commit list is written inside the session (at /var/.sketch-commit)
        // which goes into the overlay upper directory
        let commit_list_in_upper = self.overlay.upper_dir.join("var/.sketch-commit");

        // Check if a commit list exists
        if !commit_list_in_upper.exists() {
            if self.config.verbose {
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
                        self.overlay.upper_dir.clone()
                    } else {
                        // Extra mount: compute the upper directory path
                        let mount_hash = crate::overlay::mount_name_from_path(mount_point);
                        self.overlay
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
                        match std::fs::copy(&upper_file, file_path) {
                            Ok(_) => {
                                if self.config.verbose {
                                    eprintln!("sketch: committed {}", file_path);
                                }
                                committed_count += 1;
                            }
                            Err(e) => {
                                eprintln!("sketch: warning: failed to commit {}: {}", file_path, e);
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

                if self.config.verbose {
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

    fn run_command(mut self) -> Result<i32, String> {
        let (cmd, args, timeout, extra_env) = match &self.config.command {
            Command::Shell => {
                let shell = detect_shell();
                (shell, vec![], None, vec![])
            }
            Command::Run(args, run_opts) => {
                let cmd = args[0].clone();
                let cmd_args: Vec<&str> = args.get(1..).unwrap_or(&[]).iter().map(|s| s.as_str()).collect();
                let extra_env = run_opts
                    .env_vars
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect();
                (cmd, cmd_args, run_opts.timeout.clone(), extra_env)
            }
            Command::Exec(args) => {
                let cmd = args[0].clone();
                let cmd_args: Vec<&str> = args[1..].iter().map(|s| s.as_str()).collect();
                (cmd, cmd_args, None, vec![])
            }
            _ => return Err("Invalid command type for run_command".into()),
        };

        if self.config.verbose {
            eprintln!("sketch: spawning: {} {}", cmd, args.iter().map(|s| s.as_ref()).collect::<Vec<_>>().join(" "));
            if let Some(t) = &timeout {
                eprintln!("sketch: timeout: {}s", t);
            }
        }

        // Fork so we can wait for the child and then clean up
        match unsafe { fork() } {
            Ok(ForkResult::Child) => {
                // Child process: create its own mount namespace (inherits parent's mounts)
                if self.config.verbose {
                    eprintln!("sketch: [child] creating isolated mount namespace...");
                }
                if let Err(e) = unshare(CloneFlags::CLONE_NEWNS) {
                    eprintln!("sketch: [child] failed to create mount namespace: {}", e);
                    process::exit(1);
                }

                if self.config.verbose {
                    eprintln!("sketch: [child] changing root...");
                }

                if let Err(e) = self.overlay.pivot_root() {
                    eprintln!("sketch: [child] failed to change root: {}", e);
                    process::exit(1);
                }

                // Set session identifiers for child processes to detect they're in a session
                std::env::set_var("SKETCH_SESSION", "1");
                std::env::set_var("SKETCH_SESSION_DIR", &self.overlay.session_dir);
                std::env::set_var("SKETCH_ORIGINAL_UID", self.original_uid.to_string());
                std::env::set_var("SKETCH_ORIGINAL_GID", self.original_gid.to_string());

                // Set user-provided environment variables (--env KEY=VALUE)
                for (key, val) in extra_env {
                    std::env::set_var(key, val);
                }
                
                // Set the working directory inside the session to match the original cwd if possible
                env::set_current_dir(self.original_cwd).unwrap_or_else(|_| {
                    eprintln!("sketch: [child] warning: failed to set working directory, using /");
                    env::set_current_dir("/").expect("Failed to set working directory to /");
                });

                let args_str: Vec<&str> = args.iter().map(|s| s.as_ref()).collect();
                let err = exec_command(&cmd, &args_str);
                eprintln!("sketch: exec failed: {}", err);
                process::exit(127);
            }
            Ok(ForkResult::Parent { child }) => {
                // Parent stays in original root. Wait for child to complete.
                let exit_code = wait_for_child(child, timeout);

                // Now safe to access original filesystem for cleanup
                self.commit_marked_files();
                self.overlay.cleanup();
                // Prevent double-cleanup in Drop
                std::mem::forget(self);
                Ok(exit_code)
            }
            Err(e) => {
                self.overlay.cleanup();
                Err(format!("Failed to fork: {}", e))
            }
        }
    }
}

fn detect_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into())
}

fn exec_command(cmd: &str, args: &[&str]) -> nix::Error {
    use std::ffi::CString;

    let c_cmd = CString::new(cmd).expect("Invalid command string");
    let mut c_args: Vec<CString> = vec![c_cmd.clone()];
    for arg in args {
        c_args.push(CString::new(*arg).expect("Invalid argument string"));
    }

    nix::unistd::execvp(&c_cmd, &c_args).unwrap_err()
}

fn wait_for_child(pid: Pid, timeout: Option<u64>) -> i32 {
    // Forward common signals to child
    setup_signal_forwarding(pid);

    if let Some(timeout_secs) = timeout {
        wait_with_timeout(pid, timeout_secs)
    } else {
        wait_no_timeout(pid)
    }
}

fn wait_no_timeout(pid: Pid) -> i32 {
    loop {
        match waitpid(pid, None) {
            Ok(WaitStatus::Exited(_, code)) => return code,
            Ok(WaitStatus::Signaled(_, sig, _)) => return 128 + sig as i32,
            Ok(_) => continue,
            Err(nix::errno::Errno::EINTR) => continue,
            Err(_) => return 1,
        }
    }
}

fn wait_with_timeout(pid: Pid, timeout_secs: u64) -> i32 {
    use std::time::{Duration, Instant};

    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    let poll_interval = Duration::from_millis(100);

    loop {
        match waitpid(pid, Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::Exited(_, code)) => return code,
            Ok(WaitStatus::Signaled(_, sig, _)) => return 128 + sig as i32,
            Ok(WaitStatus::StillAlive) => {
                if Instant::now() >= deadline {
                    eprintln!(
                        "sketch: timeout ({}s) exceeded, killing session",
                        timeout_secs
                    );
                    // Send SIGTERM first, then SIGKILL if needed
                    let _ = nix::sys::signal::kill(pid, Signal::SIGTERM);
                    std::thread::sleep(Duration::from_millis(500));

                    // Check if it exited after SIGTERM
                    match waitpid(pid, Some(WaitPidFlag::WNOHANG)) {
                        Ok(WaitStatus::Exited(_, code)) => return code,
                        Ok(WaitStatus::Signaled(_, sig, _)) => return 128 + sig as i32,
                        _ => {
                            // Force kill
                            let _ = nix::sys::signal::kill(pid, Signal::SIGKILL);
                            match waitpid(pid, None) {
                                Ok(WaitStatus::Exited(_, code)) => return code,
                                Ok(WaitStatus::Signaled(_, sig, _)) => return 128 + sig as i32,
                                _ => return 124, // Standard timeout exit code
                            }
                        }
                    }
                }
                std::thread::sleep(poll_interval);
            }
            Ok(_) => continue,
            Err(nix::errno::Errno::EINTR) => continue,
            Err(_) => return 1,
        }
    }
}

fn setup_signal_forwarding(_child: Pid) {
    use nix::sys::signal::{sigaction, SaFlags, SigAction, SigHandler, SigSet};

    // Ignore signals in the parent process. The child is in the same process group
    // and receives these signals directly from the terminal, so no explicit
    // forwarding is needed. The parent just needs to keep waiting in waitpid().
    let signals = [Signal::SIGINT, Signal::SIGTERM, Signal::SIGHUP];

    for sig in &signals {
        let sa = SigAction::new(SigHandler::SigIgn, SaFlags::empty(), SigSet::empty());
        unsafe {
            let _ = sigaction(*sig, &sa);
        }
    }
}

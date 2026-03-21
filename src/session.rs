use nix::sys::signal::Signal;
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::{fork, ForkResult, Pid};
use std::process;

use crate::cli::RunOptions;
use crate::fs_utils;
use crate::metadata::SessionMetadata;
use crate::overlay::OverlaySession;
use crate::package;

pub struct Session {
    overlay: OverlaySession,
    original_cwd: std::path::PathBuf,
    original_uid: u32,
    original_gid: u32,
    verbose: bool,
}

impl Session {
    pub fn new(verbose: bool) -> Result<Self, String> {
        // Pre-check disk space before creating session directories
        use std::path::Path;
        match fs_utils::check_disk_space(Path::new("/tmp"))? {
            fs_utils::DiskCheck::Ok(info) => {
                if verbose {
                    eprintln!(
                        "sketch: /tmp has {} free ({}% used)",
                        crate::metadata::format_size(info.available),
                        info.used_pct,
                    );
                }
            }
            fs_utils::DiskCheck::Warning(_info, msg) => {
                eprintln!("sketch: warning: {}", msg);
            }
            fs_utils::DiskCheck::Critical(_info, msg) => {
                return Err(format!(
                    "insufficient disk space: {}. Use 'sketch --clean' to free space.",
                    msg,
                ));
            }
        }

        let overlay = OverlaySession::new()
            .map_err(|e| format!("Failed to create session directories: {}", e))?;

        let original_cwd = std::env::current_dir().unwrap_or_else(|_| "/".into());
        let original_uid = nix::unistd::getuid().as_raw();
        let original_gid = nix::unistd::getgid().as_raw();

        if verbose {
            eprintln!("sketch: session dir: {}", overlay.session_dir.display());
        }

        Ok(Self {
            overlay,
            original_cwd,
            original_uid,
            original_gid,
            verbose,
        })
    }

    pub fn start_shell(mut self) -> Result<i32, String> {
        self.write_metadata(None, "shell")?;
        self.setup()?;
        let shell = detect_shell();
        self.run_command(&shell, &[], None, &[])
    }

    pub fn start_exec(mut self, args: &[String]) -> Result<i32, String> {
        if args.is_empty() {
            return Err("No command specified".into());
        }
        self.write_metadata(None, &format!("exec {}", args.join(" ")))?;
        self.setup()?;
        let cmd = &args[0];
        let cmd_args: Vec<&str> = args[1..].iter().map(|s| s.as_str()).collect();
        self.run_command(cmd, &cmd_args, None, &[])
    }

    pub fn start_run(mut self, args: &[String], options: &RunOptions) -> Result<i32, String> {
        if args.is_empty() {
            return Err("No command specified".into());
        }
        let cmd_str = format!("run {}", args.join(" "));
        self.write_metadata(options.name.clone(), &cmd_str)?;
        self.setup()?;
        let cmd = &args[0];
        let cmd_args: Vec<&str> = args[1..].iter().map(|s| s.as_str()).collect();
        self.run_command(cmd, &cmd_args, options.timeout, &options.env_vars)
    }

    /// Write session metadata to disk for `sketch list` to discover.
    fn write_metadata(&self, name: Option<String>, command: &str) -> Result<(), String> {
        let id = self.overlay.session_id();
        let meta = SessionMetadata::new(&id, name, command, &self.overlay.session_dir);
        meta.save(&self.overlay.session_dir)
    }

    fn setup(&mut self) -> Result<(), String> {
        if self.verbose {
            eprintln!("sketch: creating mount namespace...");
        }
        self.overlay.setup_namespaces()?;

        if self.verbose {
            eprintln!("sketch: mounting overlay filesystem...");
        }
        self.overlay.mount_overlay()?;

        if self.verbose {
            eprintln!("sketch: mounting virtual filesystems...");
        }
        self.overlay.mount_virtual_filesystems()?;

        if self.verbose {
            eprintln!("sketch: mounting additional partitions...");
        }
        self.overlay.mount_additional_filesystems(self.verbose)?;

        if self.verbose {
            eprintln!("sketch: ensuring DNS resolution...");
        }
        self.overlay.ensure_dns_resolution()?;

        if self.verbose {
            eprintln!("sketch: preparing package manager support...");
        }
        self.prepare_package_managers();

        if self.verbose {
            eprintln!("sketch: pivoting root...");
        }
        self.overlay.pivot_root()?;

        Ok(())
    }

    /// Detect available package managers, log in verbose mode, and ensure
    /// their state directories exist in the overlay upper layer so writes
    /// (lock files, DB updates) don't fail.
    fn prepare_package_managers(&self) {
        if let Some(pm) = package::detect_package_manager() {
            if self.verbose {
                eprintln!("sketch: detected package manager: {}", pm.name());
            }

            // Ensure package manager state directories are writable in the overlay.
            // On some minimal systems these directories may not exist; create them
            // in the upper layer so the merged view includes them.
            for dir in pm.state_dirs() {
                let upper_path = self.overlay.upper_dir.join(dir.trim_start_matches('/'));
                let _ = std::fs::create_dir_all(&upper_path);
            }
        } else if self.verbose {
            eprintln!("sketch: no system package manager detected");
        }
    }

    fn run_command(mut self, cmd: &str, args: &[&str], timeout: Option<u64>, extra_env: &[(String, String)]) -> Result<i32, String> {
        if self.verbose {
            eprintln!("sketch: spawning: {} {}", cmd, args.join(" "));
            if let Some(t) = timeout {
                eprintln!("sketch: timeout: {}s", t);
            }
        }

        // Fork so we can wait for the child and then clean up
        match unsafe { fork() } {
            Ok(ForkResult::Child) => {
                // Set up session environment variables
                let env_vars = fs_utils::setup_session_env(self.original_uid, self.original_gid);
                for (key, val) in &env_vars {
                    std::env::set_var(key, val);
                }

                // Set package-manager-specific environment variables
                if let Some(pm) = package::detect_package_manager() {
                    for (key, val) in pm.env_vars() {
                        std::env::set_var(key, val);
                    }
                }

                // Set user-provided environment variables (--env KEY=VALUE)
                for (key, val) in extra_env {
                    std::env::set_var(key, val);
                }

                // Update PS1 to indicate we're in a sketch session
                if let Ok(ps1) = std::env::var("PS1") {
                    std::env::set_var("PS1", format!("(sketch) {}", ps1));
                } else {
                    std::env::set_var("PS1", "(sketch) \\u@\\h:\\w\\$ ");
                }

                // Restore working directory
                let _ = fs_utils::setup_working_directory(&self.original_cwd);

                // Check device access and warn
                if self.verbose {
                    for warning in fs_utils::check_device_access() {
                        eprintln!("sketch: {}", warning);
                    }
                }

                let err = exec_command(cmd, args);
                eprintln!("sketch: exec failed: {}", err);
                process::exit(127);
            }
            Ok(ForkResult::Parent { child }) => {
                let exit_code = wait_for_child(child, timeout);
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
                    eprintln!("sketch: timeout ({}s) exceeded, killing session", timeout_secs);
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

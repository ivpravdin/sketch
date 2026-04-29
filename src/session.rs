use nix::sched::{unshare, CloneFlags};
use nix::sys::signal::Signal;
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::{fork, ForkResult, Pid};
use std::io::Write;
use std::{env, process};

use crate::cli::{Command, Config};
use crate::commit::commit_marked_files;
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
        let overlay = OverlaySession::new(&config)
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
        self.write_metadata("shell")?;
        self.setup()?;
        self.run_command()
    }

    pub fn start_run(mut self) -> Result<i32, String> {
        self.setup()?;

        let Command::Run(args, _) = &self.config.command else {
            return Err("Invalid command type for start_run".into());
        };

        let cmd_str = format!("run {}", args.join(" "));
        self.write_metadata(&cmd_str)?;

        self.run_command()
    }

    /// Write session metadata to disk for `sketch list` to discover.
    fn write_metadata(&self, command: &str) -> Result<(), String> {
        let mut meta = SessionMetadata::new(
            &self.overlay.session_id,
            &self.overlay.session_dir.display().to_string(),
            self.config.name.clone(),
            std::time::Instant::now().elapsed().as_secs(),
            command,
            &self.overlay.upper_dir.display().to_string(),
            &self.overlay.work_dir.display().to_string(),
            &self.overlay.merged_dir.display().to_string(),
            self.overlay.extra_mounts.clone(),
        );
        meta.takeover(nix::unistd::getpid().as_raw(), &self.overlay.session_dir)
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
        self.overlay
            .mount_additional_filesystems(self.config.verbose)?;

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

    fn run_command(mut self) -> Result<i32, String> {
        let (cmd, args, timeout, extra_env) = match &self.config.command {
            Command::Shell => {
                // Empty cmd; runuser -l will detect the shell from /etc/passwd
                ("".into(), vec![], None, vec![])
            }
            Command::Run(args, run_opts) => {
                let cmd = args[0].clone();
                let cmd_args: Vec<&str> = args
                    .get(1..)
                    .unwrap_or(&[])
                    .iter()
                    .map(|s| s.as_str())
                    .collect();
                let extra_env = run_opts
                    .env_vars
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect();
                (cmd, cmd_args, run_opts.timeout.clone(), extra_env)
            }
            _ => return Err("Invalid command type for run_command".into()),
        };

        if self.config.verbose {
            eprintln!(
                "sketch: spawning: {} {}",
                cmd,
                args.iter()
                    .map(|s| s.as_ref())
                    .collect::<Vec<_>>()
                    .join(" ")
            );
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

                let session_file_path = "/var/.sketch-session";

                if self.config.verbose {
                    eprintln!(
                        "sketch: [child] writing session metadata to {}",
                        session_file_path
                    );
                }

                // Set session identifiers for child processes to detect they're in a session
                let mut session_file = std::fs::OpenOptions::new()
                    .create(true)
                    .write(true)
                    .open(&session_file_path)
                    .map_err(|e| format!("Failed to open sketch session file: {}", e))?;

                writeln!(
                    session_file,
                    "{}\n{}\n{}\n{}",
                    self.overlay.session_id,
                    self.original_uid,
                    self.original_gid,
                    self.config.name.as_deref().unwrap_or("")
                )
                .map_err(|e| format!("Failed to write to sketch session file: {}", e))?;

                // Set user-provided environment variables (--env KEY=VALUE)
                for (key, val) in extra_env {
                    std::env::set_var(key, val);
                }

                // Set the working directory inside the session to match the original cwd if possible
                env::set_current_dir(self.original_cwd).unwrap_or_else(|_| {
                    eprintln!("sketch: [child] warning: failed to set working directory, using /");
                    env::set_current_dir("/").expect("Failed to set working directory to /");
                });

                // Determine which user to run as
                let target_username = if self.config.as_root {
                    "root".to_string()
                } else {
                    if self.config.verbose {
                        eprintln!(
                            "sketch: [child] dropping privileges to UID={}",
                            self.original_uid
                        );
                    }
                    let sudo_uid = env::var("SUDO_UID")
                        .map_err(|_| "SUDO_UID not set".to_string())
                        .and_then(|uid_str| {
                            uid_str
                                .parse::<u32>()
                                .map_err(|_| format!("Invalid SUDO_UID '{}'", uid_str))
                        });

                    match sudo_uid {
                        Ok(_target_uid) => {
                            // Get username from SUDO_USER or use uid as fallback
                            env::var("SUDO_USER").unwrap_or_else(|_| self.original_uid.to_string())
                        }
                        Err(e) => {
                            eprintln!("sketch: [child] failed to get user info: {}", e);
                            process::exit(1);
                        }
                    }
                };

                let args_str: Vec<&str> = args.iter().map(|s| s.as_ref()).collect();
                let err = exec_with_runuser(&target_username, &cmd, &args_str);
                eprintln!("sketch: exec failed: {}", err);
                process::exit(127);
            }
            Ok(ForkResult::Parent { child }) => {
                // Parent stays in original root. Wait for child to complete.
                let exit_code = wait_for_child(child, timeout);

                // Now safe to access original filesystem for cleanup
                commit_marked_files(self.config, &self.overlay);
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

fn exec_with_runuser(username: &str, cmd: &str, args: &[&str]) -> nix::Error {
    use std::ffi::CString;

    let mut runuser_args = vec![
        CString::new("runuser").expect("Invalid command string"),
        CString::new("--pty").expect("Invalid command string"),
    ];

    runuser_args.push(CString::new(username).expect("Invalid command string"));

    runuser_args.push(CString::new("-l").expect("Invalid command string"));

    // For commands: runuser <user> -c "<cmd> <args>"
    if !cmd.is_empty() {
        runuser_args.push(CString::new("-c").expect("Invalid command string"));

        let mut cmd_str = cmd.to_string();
        for arg in args {
            cmd_str.push(' ');
            cmd_str.push_str(arg);
        }

        runuser_args.push(CString::new(cmd_str).expect("Invalid command string"));
    }

    nix::unistd::execvp(&runuser_args[0], &runuser_args).unwrap_err()
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

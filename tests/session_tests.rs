//! Session lifecycle and command execution tests.
//!
//! Tests for session creation, command execution, exit codes,
//! environment variables, and signal handling.

use std::process::Command;

fn sketch_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_sketch"))
}

fn is_root() -> bool {
    nix::unistd::geteuid().is_root()
}

// ============================================================
// Command execution (exec mode)
// ============================================================

#[test]
fn exec_true_exits_zero() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }
    let output = sketch_bin().args(["exec", "true"]).output().unwrap();
    assert!(output.status.success(), "exec true should exit 0");
}

#[test]
fn exec_false_exits_nonzero() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }
    let output = sketch_bin().args(["exec", "false"]).output().unwrap();
    assert!(!output.status.success(), "exec false should exit non-zero");
}

#[test]
fn exec_exit_code_propagated() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }
    let output = sketch_bin()
        .args(["exec", "sh", "-c", "exit 42"])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(42),
        "exit code should be propagated from inner command"
    );
}

#[test]
fn exec_captures_stdout() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }
    let output = sketch_bin()
        .args(["exec", "echo", "hello_from_sketch"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("hello_from_sketch"),
        "stdout should contain command output"
    );
}

#[test]
fn exec_captures_stderr() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }
    let output = sketch_bin()
        .args(["exec", "sh", "-c", "echo err_test >&2"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("err_test"),
        "stderr should contain command error output"
    );
}

#[test]
fn exec_with_arguments() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }
    let output = sketch_bin()
        .args(["exec", "echo", "-n", "no_newline"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.as_ref(), "no_newline", "arguments should be passed to command");
}

#[test]
fn exec_nonexistent_command() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }
    let output = sketch_bin()
        .args(["exec", "nonexistent_command_xyz"])
        .output()
        .unwrap();
    assert!(!output.status.success(), "nonexistent command should fail");
    // Convention: exit code 127 for command not found
    assert_eq!(
        output.status.code(),
        Some(127),
        "nonexistent command should exit with 127"
    );
}

// ============================================================
// Environment variables
// ============================================================

#[test]
fn sketch_session_env_var_set() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }
    let output = sketch_bin()
        .args(["exec", "sh", "-c", "echo $SKETCH_SESSION"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.trim(),
        "1",
        "SKETCH_SESSION should be set to 1 inside session"
    );
}

#[test]
fn path_env_var_available() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }
    let output = sketch_bin()
        .args(["exec", "sh", "-c", "echo $PATH"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.trim().is_empty(),
        "PATH should be available inside session"
    );
}

#[test]
fn home_env_var_available() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }
    let output = sketch_bin()
        .args(["exec", "sh", "-c", "echo $HOME"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.trim().is_empty(),
        "HOME should be available inside session"
    );
}

// ============================================================
// Verbose mode
// ============================================================

#[test]
fn verbose_mode_prints_session_info() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }
    let output = sketch_bin()
        .args(["--verbose", "exec", "true"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("session dir"),
        "verbose mode should print session directory"
    );
    assert!(
        stderr.contains("mounting overlay"),
        "verbose mode should print mount info"
    );
}

#[test]
fn verbose_mode_prints_namespace_info() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }
    let output = sketch_bin()
        .args(["--verbose", "exec", "true"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("namespace") || stderr.contains("mount namespace"),
        "verbose mode should mention namespace creation"
    );
}

// ============================================================
// File operations inside session
// ============================================================

#[test]
fn can_write_to_etc_in_session() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }
    let output = sketch_bin()
        .args(["exec", "sh", "-c", "echo test > /etc/sketch_test_file && cat /etc/sketch_test_file"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "should be able to write to /etc in session");
    assert!(stdout.contains("test"), "should read back written content");

    // Verify it didn't persist on host
    assert!(
        !std::path::Path::new("/etc/sketch_test_file").exists(),
        "file written to /etc should not persist on host"
    );
}

#[test]
fn can_create_and_read_files() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }
    let output = sketch_bin()
        .args(["exec", "sh", "-c", "echo hello > /tmp/sketch_rw_test && cat /tmp/sketch_rw_test"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert_eq!(stdout.trim(), "hello");
}

#[test]
fn can_create_symlinks() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }
    let output = sketch_bin()
        .args(["exec", "sh", "-c", "echo data > /tmp/sketch_link_target && ln -s /tmp/sketch_link_target /tmp/sketch_link_test && cat /tmp/sketch_link_test"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert_eq!(stdout.trim(), "data", "symlink should resolve correctly in session");
}

#[test]
fn can_change_permissions() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }
    let output = sketch_bin()
        .args(["exec", "sh", "-c", "touch /tmp/sketch_perm_test && chmod 755 /tmp/sketch_perm_test && stat -c %a /tmp/sketch_perm_test"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert_eq!(stdout.trim(), "755");
}

// ============================================================
// Multiple sessions don't interfere
// ============================================================

#[test]
fn concurrent_sessions_isolated() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }

    use std::thread;

    let handles: Vec<_> = (0..3)
        .map(|i| {
            thread::spawn(move || {
                let marker = format!("session_marker_{}", i);
                let output = sketch_bin()
                    .args(["exec", "sh", "-c", &format!(
                        "echo {} > /tmp/sketch_concurrent_test && cat /tmp/sketch_concurrent_test",
                        marker
                    )])
                    .output()
                    .unwrap();
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                assert_eq!(stdout, marker, "each session should see its own writes");
            })
        })
        .collect();

    for h in handles {
        h.join().expect("thread panicked");
    }

    // No file should persist
    assert!(
        !std::path::Path::new("/tmp/sketch_concurrent_test").exists(),
        "concurrent test file should not persist"
    );
}

// ============================================================
// Permission denied (non-root)
// ============================================================

#[test]
fn non_root_shell_shows_root_error() {
    if is_root() {
        eprintln!("SKIPPED: running as root");
        return;
    }
    let output = sketch_bin().arg("shell").output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("must be run as root")
            || stderr.contains("sudo")
            || stderr.contains("mount")
            || stderr.contains("EINVAL"),
        "should explain why non-root execution failed, got: {}",
        stderr
    );
}

#[test]
fn non_root_exec_shows_root_error() {
    if is_root() {
        eprintln!("SKIPPED: running as root");
        return;
    }
    let output = sketch_bin().args(["exec", "ls"]).output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("must be run as root")
            || stderr.contains("sudo")
            || stderr.contains("mount")
            || stderr.contains("EINVAL"),
        "should explain why non-root execution failed, got: {}",
        stderr
    );
}

// ============================================================
// Edge cases
// ============================================================

#[test]
fn exec_command_with_spaces_in_args() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }
    let output = sketch_bin()
        .args(["exec", "echo", "hello world"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert_eq!(stdout.trim(), "hello world");
}

#[test]
fn exec_command_with_special_characters() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }
    let output = sketch_bin()
        .args(["exec", "echo", "test!@#$%"])
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[test]
fn exec_empty_output_command() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }
    let output = sketch_bin()
        .args(["exec", "true"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stdout.is_empty(), "true should produce no stdout");
}

#[test]
fn exec_large_output() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }
    // Generate ~100KB of output
    let output = sketch_bin()
        .args(["exec", "sh", "-c", "seq 1 10000"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.trim().split('\n').collect();
    assert_eq!(lines.len(), 10000, "should capture all 10000 lines");
}

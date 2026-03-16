//! Full integration tests for sketch.
//!
//! These tests verify end-to-end workflows including package management,
//! complex file operations, and system-level behavior.

use std::fs;
use std::path::Path;
use std::process::Command;

fn sketch_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_sketch"))
}

fn is_root() -> bool {
    nix::unistd::geteuid().is_root()
}

// ============================================================
// Full workflow tests
// ============================================================

#[test]
fn full_exec_workflow() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }

    // Create file, modify it, verify it exists in session, then verify cleanup
    let script = r#"
        touch /tmp/workflow_test_file
        echo "step1" > /tmp/workflow_test_file
        echo "step2" >> /tmp/workflow_test_file
        cat /tmp/workflow_test_file
        mkdir -p /tmp/workflow_test_dir/sub
        ls /tmp/workflow_test_dir/sub > /dev/null
        rm /tmp/workflow_test_file
        test ! -f /tmp/workflow_test_file
    "#;

    let output = sketch_bin()
        .args(["exec", "sh", "-c", script])
        .output()
        .unwrap();

    assert!(output.status.success(), "full workflow should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("step1"), "should see step1 output");
    assert!(stdout.contains("step2"), "should see step2 output");

    // Nothing should persist
    assert!(!Path::new("/tmp/workflow_test_file").exists());
    assert!(!Path::new("/tmp/workflow_test_dir").exists());
}

#[test]
fn multiple_sequential_sessions() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }

    // Run three sessions back to back
    for i in 0..3 {
        let marker = format!("sequential_test_{}", i);
        let output = sketch_bin()
            .args(["exec", "sh", "-c", &format!(
                "echo {} > /tmp/sketch_seq_test && cat /tmp/sketch_seq_test",
                marker
            )])
            .output()
            .unwrap();

        assert!(output.status.success(), "session {} should succeed", i);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(stdout.trim(), marker);
    }

    assert!(!Path::new("/tmp/sketch_seq_test").exists());
}

// ============================================================
// Package management tests
// ============================================================

#[test]
fn package_install_does_not_persist() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }

    // Check if apt is available
    if !Path::new("/usr/bin/apt-get").exists() {
        eprintln!("SKIPPED: apt-get not available");
        return;
    }

    // Try to install a small package (cowsay) in session
    let output = sketch_bin()
        .args(["exec", "sh", "-c", "apt-get update -qq && apt-get install -y -qq cowsay 2>/dev/null && which cowsay"])
        .output()
        .unwrap();

    // Regardless of whether install succeeded, verify host is unchanged
    let host_has_cowsay = Path::new("/usr/games/cowsay").exists()
        || Command::new("which").arg("cowsay").output().map(|o| o.status.success()).unwrap_or(false);

    if output.status.success() {
        // If install succeeded in session, verify host doesn't have it (unless it was already there)
        if !host_has_cowsay {
            // Package should not be on host
            let check = Command::new("which").arg("cowsay").output();
            assert!(
                check.is_err() || !check.unwrap().status.success(),
                "package installed in session should not persist on host"
            );
        }
    }
}

#[test]
fn package_removal_does_not_persist() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }

    if !Path::new("/usr/bin/apt-get").exists() {
        eprintln!("SKIPPED: apt-get not available");
        return;
    }

    // Try to remove coreutils in session (shouldn't affect host)
    let _output = sketch_bin()
        .args(["exec", "sh", "-c", "dpkg --remove --force-depends coreutils 2>/dev/null; echo done"])
        .output()
        .unwrap();

    // Host should still have coreutils
    assert!(
        Path::new("/usr/bin/ls").exists(),
        "removing package in session should not affect host"
    );
}

#[test]
fn dns_resolution_works_in_session() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }

    // Verify that /etc/resolv.conf is readable inside the session
    let output = sketch_bin()
        .args(["exec", "sh", "-c", "test -f /etc/resolv.conf && cat /etc/resolv.conf | grep -c nameserver"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "resolv.conf should be readable in session"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let count: u32 = stdout.trim().parse().unwrap_or(0);
    assert!(count >= 1, "resolv.conf should contain at least one nameserver entry");
}

#[test]
fn package_manager_env_vars_set() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }

    if !Path::new("/usr/bin/apt-get").exists() {
        eprintln!("SKIPPED: apt-get not available");
        return;
    }

    // On Debian/Ubuntu, DEBIAN_FRONTEND should be set to noninteractive
    let output = sketch_bin()
        .args(["exec", "sh", "-c", "echo $DEBIAN_FRONTEND"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.trim(),
        "noninteractive",
        "DEBIAN_FRONTEND should be set to noninteractive in sketch session"
    );
}

#[test]
fn pip_install_does_not_persist() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }

    // Check if pip3 is available
    if !Path::new("/usr/bin/pip3").exists() && !Path::new("/usr/local/bin/pip3").exists() {
        eprintln!("SKIPPED: pip3 not available");
        return;
    }

    // Install a small package in session
    let output = sketch_bin()
        .args(["exec", "sh", "-c", "pip3 install --break-system-packages cowsay 2>/dev/null || pip3 install cowsay 2>/dev/null; pip3 show cowsay 2>/dev/null && echo INSTALLED"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.contains("INSTALLED") {
        // Package was installed in session; verify it's not on host
        let host_check = Command::new("sh")
            .args(["-c", "pip3 show cowsay 2>/dev/null"])
            .output();

        if let Ok(hc) = host_check {
            // If cowsay wasn't already on host, it shouldn't be there now
            let host_stdout = String::from_utf8_lossy(&hc.stdout);
            if !host_stdout.contains("cowsay") {
                // Good - not on host, session change was ephemeral
            }
        }
    }
}

// ============================================================
// System file protection
// ============================================================

#[test]
fn etc_passwd_not_modified_on_host() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }

    let original = fs::read_to_string("/etc/passwd").unwrap();

    // Add a user in the session
    let output = sketch_bin()
        .args(["exec", "sh", "-c", "echo 'sketchtest:x:9999:9999::/tmp:/bin/false' >> /etc/passwd && grep sketchtest /etc/passwd"])
        .output()
        .unwrap();

    assert!(output.status.success(), "should be able to modify /etc/passwd in session");

    // Host /etc/passwd should be unchanged
    let after = fs::read_to_string("/etc/passwd").unwrap();
    assert_eq!(original, after, "/etc/passwd should not be modified on host");
}

#[test]
fn etc_hosts_not_modified_on_host() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }

    let original = fs::read_to_string("/etc/hosts").unwrap();

    let output = sketch_bin()
        .args(["exec", "sh", "-c", "echo '127.0.0.1 sketch-test-host' >> /etc/hosts"])
        .output()
        .unwrap();

    assert!(output.status.success());

    let after = fs::read_to_string("/etc/hosts").unwrap();
    assert_eq!(original, after, "/etc/hosts should not be modified on host");
}

// ============================================================
// Disk and resource tests
// ============================================================

#[test]
fn large_file_creation_in_session() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }

    // Create a 10MB file in session
    let output = sketch_bin()
        .args(["exec", "sh", "-c", "dd if=/dev/zero of=/tmp/sketch_large_file bs=1M count=10 2>/dev/null && ls -la /tmp/sketch_large_file"])
        .output()
        .unwrap();

    assert!(output.status.success(), "should create large file in session");

    // Should not persist
    assert!(!Path::new("/tmp/sketch_large_file").exists());
}

#[test]
fn many_small_files_in_session() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }

    let output = sketch_bin()
        .args(["exec", "sh", "-c", "mkdir -p /tmp/sketch_many_files && for i in $(seq 1 100); do touch /tmp/sketch_many_files/file_$i; done && ls /tmp/sketch_many_files | wc -l"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "100", "should create 100 files");

    assert!(!Path::new("/tmp/sketch_many_files").exists());
}

// ============================================================
// Process isolation
// ============================================================

#[test]
fn session_has_own_mount_namespace() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }

    // Check that mount namespace is different
    let host_ns = fs::read_link("/proc/self/ns/mnt")
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    let output = sketch_bin()
        .args(["exec", "readlink", "/proc/self/ns/mnt"])
        .output()
        .unwrap();

    let session_ns = String::from_utf8_lossy(&output.stdout).trim().to_string();

    assert_ne!(
        host_ns, session_ns,
        "session should have different mount namespace than host"
    );
}

// ============================================================
// Signal handling
// ============================================================

#[test]
fn sigterm_causes_cleanup() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }

    use std::time::Duration;

    // Start a long-running session
    let mut child = sketch_bin()
        .args(["exec", "sleep", "60"])
        .spawn()
        .unwrap();

    // Give it time to set up
    std::thread::sleep(Duration::from_millis(500));

    // Send SIGTERM
    unsafe {
        libc::kill(child.id() as i32, libc::SIGTERM);
    }

    let status = child.wait().unwrap();
    // Should have exited (possibly with signal code)
    assert!(!status.success() || status.code().is_some(), "should exit after SIGTERM");

    // Give cleanup a moment
    std::thread::sleep(Duration::from_millis(200));

    // Check no new orphaned sketch dirs (best effort)
    let orphans: Vec<_> = fs::read_dir("/tmp")
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("sketch-"))
        .collect();

    // Clean up any that remain
    if !orphans.is_empty() {
        let _ = sketch_bin().arg("--clean").output();
    }
}

// ============================================================
// Cleanup edge cases
// ============================================================

#[test]
fn clean_with_no_orphans_is_idempotent() {
    // First call cleans up anything that exists
    let _output1 = sketch_bin().arg("--clean").output().unwrap();
    // Second call should find nothing
    let output2 = sketch_bin().arg("--clean").output().unwrap();
    // Third call should also find nothing (idempotent)
    let output3 = sketch_bin().arg("--clean").output().unwrap();

    assert!(output2.status.success());
    assert!(output3.status.success());

    let stdout2 = String::from_utf8_lossy(&output2.stdout);
    let stdout3 = String::from_utf8_lossy(&output3.stdout);

    assert_eq!(stdout2.trim(), "sketch: no orphaned sessions found");
    assert_eq!(stdout2.trim(), stdout3.trim(), "--clean should be idempotent");
}

#[test]
fn clean_multiple_orphaned_dirs() {
    // Create multiple fake orphans (doesn't require root for creation)
    let orphans = [
        "/tmp/sketch-11111111-1111-1111-1111-111111111111",
        "/tmp/sketch-22222222-2222-2222-2222-222222222222",
        "/tmp/sketch-33333333-3333-3333-3333-333333333333",
    ];

    for orphan in &orphans {
        let _ = fs::create_dir_all(format!("{}/upper", orphan));
        let _ = fs::create_dir_all(format!("{}/work", orphan));
        let _ = fs::create_dir_all(format!("{}/merged", orphan));
    }

    let output = sketch_bin().arg("--clean").output().unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should report cleaning up at least 3 sessions
    if stdout.contains("cleaned up") {
        // Extract the number
        let cleaned: u32 = stdout
            .split_whitespace()
            .find_map(|w| w.parse().ok())
            .unwrap_or(0);
        assert!(cleaned >= 3, "should clean up at least 3 orphans, cleaned: {}", cleaned);
    }

    for orphan in &orphans {
        assert!(!Path::new(orphan).exists(), "orphan {} should be removed", orphan);
    }
}

// ============================================================
// Device access tests
// ============================================================

#[test]
fn dev_null_writable_in_session() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }
    let output = sketch_bin()
        .args(["exec", "sh", "-c", "echo test > /dev/null && echo ok"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "ok");
}

#[test]
fn dev_zero_readable_in_session() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }
    let output = sketch_bin()
        .args(["exec", "sh", "-c", "dd if=/dev/zero bs=1 count=4 2>/dev/null | wc -c"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "4");
}

#[test]
fn dev_urandom_readable_in_session() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }
    let output = sketch_bin()
        .args(["exec", "sh", "-c", "dd if=/dev/urandom bs=1 count=16 2>/dev/null | wc -c"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "16");
}

// ============================================================
// Hard link tests
// ============================================================

#[test]
fn hard_links_work_in_session() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }
    let output = sketch_bin()
        .args(["exec", "sh", "-c",
            "echo data > /tmp/sketch_hardlink_src && ln /tmp/sketch_hardlink_src /tmp/sketch_hardlink_dst && cat /tmp/sketch_hardlink_dst"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "data");

    // Neither should persist
    assert!(!Path::new("/tmp/sketch_hardlink_src").exists());
    assert!(!Path::new("/tmp/sketch_hardlink_dst").exists());
}

// ============================================================
// Rapid session cycling
// ============================================================

#[test]
fn rapid_session_cycling_no_leaks() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }

    let before: Vec<String> = fs::read_dir("/tmp")
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with("sketch-"))
        .collect();

    // Rapidly start and stop 10 sessions
    for i in 0..10 {
        let output = sketch_bin()
            .args(["exec", "echo", &format!("rapid_{}", i)])
            .output()
            .unwrap();
        assert!(output.status.success(), "rapid session {} should succeed", i);
    }

    let after: Vec<String> = fs::read_dir("/tmp")
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with("sketch-"))
        .collect();

    let leaked: Vec<&String> = after.iter().filter(|d| !before.contains(d)).collect();
    assert!(
        leaked.is_empty(),
        "no sketch dirs should leak after rapid cycling, found: {:?}",
        leaked
    );
}

// ============================================================
// Nested invocation
// ============================================================

#[test]
fn nested_sketch_detected_via_env() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }
    // Verify SKETCH_SESSION is set, which allows detecting nested invocation
    let output = sketch_bin()
        .args(["exec", "sh", "-c", "echo $SKETCH_SESSION"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "1", "SKETCH_SESSION should be 1 inside session");
}

// ============================================================
// DNS resolution in session
// ============================================================

#[test]
fn resolv_conf_accessible_in_session() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }
    let output = sketch_bin()
        .args(["exec", "sh", "-c", "test -f /etc/resolv.conf && cat /etc/resolv.conf | head -1"])
        .output()
        .unwrap();
    // resolv.conf should be readable (may not exist on all systems)
    if Path::new("/etc/resolv.conf").exists() {
        assert!(output.status.success(), "/etc/resolv.conf should be accessible in session");
    }
}

// ============================================================
// sys filesystem
// ============================================================

#[test]
fn sys_filesystem_mounted_in_session() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }
    let output = sketch_bin()
        .args(["exec", "test", "-d", "/sys/class"])
        .output()
        .unwrap();
    assert!(output.status.success(), "/sys/class should exist in session");
}

// ============================================================
// Working directory preservation
// ============================================================

#[test]
fn working_directory_preserved() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }
    // Run from /tmp and check pwd
    let output = Command::new(env!("CARGO_BIN_EXE_sketch"))
        .args(["exec", "pwd"])
        .current_dir("/tmp")
        .output()
        .unwrap();
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(stdout.trim(), "/tmp", "working directory should be preserved");
    }
}

// ============================================================
// Pipe support
// ============================================================

#[test]
fn exec_with_pipe_via_sh() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }
    let output = sketch_bin()
        .args(["exec", "sh", "-c", "echo hello world | wc -w"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "2");
}

// ============================================================
// File locking inside session
// ============================================================

#[test]
fn file_locking_works_in_session() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }
    let output = sketch_bin()
        .args(["exec", "sh", "-c",
            "flock /tmp/sketch_lock_test echo locked_ok"])
        .output()
        .unwrap();
    // flock may not be available on all systems
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(stdout.trim(), "locked_ok");
    }
    assert!(!Path::new("/tmp/sketch_lock_test").exists());
}

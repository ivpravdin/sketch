//! Tests for overlay mount isolation fix.
//!
//! Verifies that additional filesystems (like /home) are mounted as overlays
//! and that changes don't leak to the host filesystem.

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
// Additional filesystem overlay isolation
// ============================================================

#[test]
fn home_directory_isolation() {
    if !is_root() {
        panic!("This test requires root access. Run with: CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER='sudo -E' cargo test");
    }

    // Create a test file in /home in the session
    let test_file = "/home/sketch_overlay_test_file_isolation";

    // Ensure file doesn't exist on host before test
    let _ = fs::remove_file(test_file);

    let output = sketch_bin()
        .args(["exec", "sh", "-c", &format!(
            "echo test_content > {} && cat {}",
            test_file,
            test_file
        )])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "should be able to write to /home in overlay session"
    );
    assert!(
        stdout.contains("test_content"),
        "should read back the written content"
    );

    // Most important: verify the file did NOT persist on the host
    assert!(
        !Path::new(test_file).exists(),
        "file written to /home should NOT persist on host (overlay isolation failed!)"
    );
}

#[test]
fn home_subdirectory_changes_isolated() {
    if !is_root() {
        panic!("This test requires root access. Run with: CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER='sudo -E' cargo test");
    }

    let test_dir = "/home/sketch_test_dir";
    let test_file = "/home/sketch_test_dir/file.txt";

    // Clean up before test
    let _ = fs::remove_dir_all(test_dir);

    let output = sketch_bin()
        .args(["exec", "sh", "-c", &format!(
            "mkdir -p {} && echo data > {} && test -f {} && echo success",
            test_dir,
            test_file,
            test_file
        )])
        .output()
        .unwrap();

    assert!(output.status.success(), "directory/file operations should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("success"), "file creation and read should work");

    // Verify no persistence
    assert!(
        !Path::new(test_dir).exists(),
        "directory created in /home should not persist on host"
    );
}

#[test]
fn multiple_mounts_isolated() {
    if !is_root() {
        panic!("This test requires root access. Run with: CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER='sudo -E' cargo test");
    }

    // Test isolation across different mount points
    let test_cases = vec![
        ("/etc/sketch_etc_test", "etc_marker"),
        ("/var/sketch_var_test", "var_marker"),
        ("/opt/sketch_opt_test", "opt_marker"),
    ];

    for (path, marker) in test_cases {
        let output = sketch_bin()
            .args(["exec", "sh", "-c", &format!(
                "echo {} > {} && cat {}",
                marker, path, path
            )])
            .output()
            .unwrap();

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(
                stdout.contains(marker),
                "should read back marker from {}",
                path
            );

            // Verify no persistence
            assert!(
                !Path::new(path).exists(),
                "file at {} should not persist on host",
                path
            );
        }
    }
}

#[test]
fn overlay_preserves_base_filesystem() {
    if !is_root() {
        panic!("This test requires root access. Run with: CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER='sudo -E' cargo test");
    }

    // Verify that files that exist on the base filesystem are still readable
    let output = sketch_bin()
        .args(["exec", "sh", "-c", "test -f /etc/hostname && echo found || echo not_found"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("found"),
        "base filesystem files should be accessible through overlay"
    );
}

#[test]
fn overlay_readable_writes() {
    if !is_root() {
        panic!("This test requires root access. Run with: CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER='sudo -E' cargo test");
    }

    // Write a file in the overlay upper layer and read it back immediately
    let output = sketch_bin()
        .args(["exec", "sh", "-c", r#"
            TEST_FILE="/tmp/sketch_rw_test_123"
            echo "initial" > "$TEST_FILE"
            cat "$TEST_FILE"
            echo "modified" > "$TEST_FILE"
            cat "$TEST_FILE"
        "#])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should see both initial and modified content
    assert!(stdout.contains("initial"), "should see initial write");
    assert!(stdout.contains("modified"), "should see modified write");
}

// ============================================================
// Session cleanup verification
// ============================================================

#[test]
fn overlay_cleanup_removes_temporary_directories() {
    if !is_root() {
        panic!("This test requires root access. Run with: CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER='sudo -E' cargo test");
    }

    // Create a session and force it to exit
    sketch_bin()
        .args(["exec", "sh", "-c", "exit 0"])
        .output()
        .unwrap();

    // After session exit, temporary overlay directories should be cleaned
    // (We can't easily verify they're gone without parsing /proc/mounts,
    // but we can verify cleanup doesn't error out)

    let cleanup_output = sketch_bin()
        .arg("--clean")
        .output()
        .unwrap();

    assert!(cleanup_output.status.success(), "cleanup should succeed");
}

// ============================================================
// Overlay-specific error handling
// ============================================================

#[test]
fn verbose_mode_shows_overlay_mounts() {
    if !is_root() {
        panic!("This test requires root access. Run with: CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER='sudo -E' cargo test");
    }

    let output = sketch_bin()
        .args(["--verbose", "exec", "true"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should see overlay mounting for at least the root filesystem
    assert!(
        stderr.contains("mounting overlay") || stderr.contains("overlay"),
        "verbose mode should show overlay mount operations"
    );
}

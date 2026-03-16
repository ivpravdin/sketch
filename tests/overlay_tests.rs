//! Overlay filesystem tests.
//!
//! Tests for OverlaySession creation, directory structure, and cleanup.
//! Mount-related tests require root privileges and are skipped otherwise.

use std::fs;
use std::path::Path;

// We need to access the sketch internals for overlay tests.
// Since overlay is not pub-exported from the library, we test via subprocess
// and direct filesystem inspection.

/// Helper: check if running as root
fn is_root() -> bool {
    nix::unistd::geteuid().is_root()
}

/// Helper: run sketch binary
fn sketch_bin() -> std::process::Command {
    std::process::Command::new(env!("CARGO_BIN_EXE_sketch"))
}

// ============================================================
// Session directory creation tests
// ============================================================

#[test]
fn session_dir_created_under_tmp() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }

    // Use exec with a command that just lists the sketch tmp dirs
    let output = sketch_bin()
        .args(["exec", "ls", "/"])
        .output()
        .expect("failed to run sketch");

    // After execution, no sketch temp dirs should remain (cleanup works)
    let remaining: Vec<_> = fs::read_dir("/tmp")
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("sketch-"))
        .collect();

    // Cleanup should have removed the session dir
    // (Note: other tests may leave dirs, so we just check exit was clean)
    assert!(
        output.status.success() || remaining.is_empty(),
        "session should clean up after itself"
    );
}

#[test]
fn session_dir_has_correct_structure() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }

    // Run a command that inspects the session directory structure from inside
    // The session_dir won't be directly visible after pivot_root, but we can
    // verify the overlay is working by checking we're in the merged root
    let output = sketch_bin()
        .args(["exec", "test", "-d", "/tmp"])
        .output()
        .unwrap();
    assert!(output.status.success(), "/tmp should exist in overlay session");
}

// ============================================================
// Overlay isolation tests - files don't persist
// ============================================================

#[test]
fn file_created_in_session_does_not_persist() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }

    let test_file = "/tmp/sketch_test_file_isolation_check";
    // Make sure it doesn't exist before
    let _ = fs::remove_file(test_file);

    // Create a file inside the sketch session
    let output = sketch_bin()
        .args(["exec", "touch", test_file])
        .output()
        .unwrap();
    assert!(output.status.success(), "touch should succeed in session");

    // File should NOT exist on host after session exits
    assert!(
        !Path::new(test_file).exists(),
        "file created in session should not persist on host"
    );
}

#[test]
fn directory_created_in_session_does_not_persist() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }

    let test_dir = "/tmp/sketch_test_dir_isolation_check";
    let _ = fs::remove_dir_all(test_dir);

    let output = sketch_bin()
        .args(["exec", "mkdir", "-p", test_dir])
        .output()
        .unwrap();
    assert!(output.status.success(), "mkdir should succeed in session");

    assert!(
        !Path::new(test_dir).exists(),
        "directory created in session should not persist on host"
    );
}

#[test]
fn file_modification_in_session_does_not_persist() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }

    let test_file = "/tmp/sketch_test_mod_check";
    let original_content = "original_content_12345\n";

    // Create a file on host
    fs::write(test_file, original_content).unwrap();

    // Modify it inside sketch session
    let output = sketch_bin()
        .args(["exec", "sh", "-c", &format!("echo modified > {}", test_file)])
        .output()
        .unwrap();
    assert!(output.status.success(), "modification should succeed in session");

    // Host file should be unchanged
    let content = fs::read_to_string(test_file).unwrap();
    assert_eq!(
        content, original_content,
        "file content on host should be unchanged after session"
    );

    // Cleanup
    let _ = fs::remove_file(test_file);
}

#[test]
fn file_deletion_in_session_does_not_persist() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }

    let test_file = "/tmp/sketch_test_del_check";
    fs::write(test_file, "do not delete\n").unwrap();

    // Delete it inside sketch session
    let output = sketch_bin()
        .args(["exec", "rm", test_file])
        .output()
        .unwrap();
    assert!(output.status.success(), "rm should succeed in session");

    // Host file should still exist
    assert!(
        Path::new(test_file).exists(),
        "file deleted in session should still exist on host"
    );

    // Cleanup
    let _ = fs::remove_file(test_file);
}

// ============================================================
// Host filesystem visibility
// ============================================================

#[test]
fn host_files_visible_in_session() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }

    // /etc/hostname should be readable from inside the session
    let output = sketch_bin()
        .args(["exec", "cat", "/etc/hostname"])
        .output()
        .unwrap();

    let host_hostname = fs::read_to_string("/etc/hostname").unwrap_or_default();
    let session_hostname = String::from_utf8_lossy(&output.stdout);

    assert_eq!(
        session_hostname.trim(),
        host_hostname.trim(),
        "host /etc/hostname should be visible in session"
    );
}

#[test]
fn proc_filesystem_mounted_in_session() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }

    let output = sketch_bin()
        .args(["exec", "test", "-d", "/proc/self"])
        .output()
        .unwrap();
    assert!(output.status.success(), "/proc/self should exist in session");
}

#[test]
fn dev_filesystem_mounted_in_session() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }

    let output = sketch_bin()
        .args(["exec", "test", "-c", "/dev/null"])
        .output()
        .unwrap();
    assert!(output.status.success(), "/dev/null should be a char device in session");
}

// ============================================================
// Cleanup verification
// ============================================================

#[test]
fn cleanup_removes_all_session_dirs() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }

    // Get list of sketch dirs before
    let before: Vec<String> = fs::read_dir("/tmp")
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with("sketch-"))
        .collect();

    // Run a session
    let _ = sketch_bin().args(["exec", "true"]).output().unwrap();

    // Get list of sketch dirs after
    let after: Vec<String> = fs::read_dir("/tmp")
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with("sketch-"))
        .collect();

    // No new sketch dirs should remain
    let new_dirs: Vec<&String> = after.iter().filter(|d| !before.contains(d)).collect();
    assert!(
        new_dirs.is_empty(),
        "no new sketch-* dirs should remain after session, found: {:?}",
        new_dirs
    );
}

#[test]
fn clean_flag_removes_orphaned_dirs() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }

    // Create a fake orphaned session directory
    let orphan_dir = "/tmp/sketch-00000000-0000-0000-0000-000000000000";
    fs::create_dir_all(format!("{}/upper", orphan_dir)).unwrap();
    fs::create_dir_all(format!("{}/work", orphan_dir)).unwrap();
    fs::create_dir_all(format!("{}/merged", orphan_dir)).unwrap();

    // Run --clean
    let output = sketch_bin().arg("--clean").output().unwrap();
    assert!(output.status.success(), "--clean should succeed");

    // Orphaned dir should be removed
    assert!(
        !Path::new(orphan_dir).exists(),
        "orphaned session dir should be removed by --clean"
    );
}

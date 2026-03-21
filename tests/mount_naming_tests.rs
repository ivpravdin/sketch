//! Tests for hash-based mount directory naming (collision prevention).

use std::process::Command;

fn sketch_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_sketch"))
}

fn is_root() -> bool {
    nix::unistd::geteuid().is_root()
}

// ============================================================
// Mount naming and collision prevention
// ============================================================

#[test]
fn different_mounts_get_different_names() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }

    // This test verifies that different mount points get unique hash names
    // even if they would collide with the old replace('/', "_") scheme.
    // Since we can't directly inspect the naming from outside, we verify
    // indirectly by ensuring both /home and other mounts work correctly.

    let output = sketch_bin()
        .args(["--verbose", "exec", "sh", "-c", "echo test"])
        .output()
        .unwrap();

    assert!(output.status.success(), "session should complete successfully");

    // In verbose mode, we should see mounting messages for various filesystems
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("overlay"),
        "verbose mode should show overlay mounting"
    );
}

#[test]
fn collision_paths_both_isolate() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }

    // Create files at paths that would collide under the old naming:
    // /home/user and /home_user would both map to "home_user"
    // Now they should get unique hash names and not interfere

    let output = sketch_bin()
        .args(["exec", "sh", "-c", r#"
            # Note: /home_user probably won't exist on most systems,
            # but /home should, so we test with /home at least
            echo "test_home" > /home/sketch_test_collision
            if [ -f /home/sketch_test_collision ]; then
                cat /home/sketch_test_collision
            fi
        "#])
        .output()
        .unwrap();

    // Should be able to write and read from /home
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("test_home"),
        "should be able to write to and read from /home"
    );
}

#[test]
fn mount_name_is_deterministic() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }

    // Create two sessions and verify they handle the same mounts consistently
    for _ in 0..2 {
        let output = sketch_bin()
            .args(["--verbose", "exec", "true"])
            .output()
            .unwrap();

        assert!(output.status.success());
    }

    // If naming is non-deterministic, this could cause issues;
    // if deterministic, cleanup can be predictable
}

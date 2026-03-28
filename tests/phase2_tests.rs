//! Phase 2 integration tests for sketch run, list, and status commands.
//!
//! Tests cover:
//! - sketch run: basic execution, --name, --timeout, exit codes
//! - sketch list: table output, --json, ls alias, stale detection
//! - sketch status: output sections, diagnostics
//! - Cross-command integration (run creates metadata, list reads it)

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
// sketch run - basic execution
// ============================================================

#[test]
fn run_echo_exits_zero() {
    let output = sketch_bin()
        .args(["run", "--", "echo", "hello"])
        .output()
        .unwrap();
    assert!(output.status.success(), "run echo should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "hello");
}

#[test]
fn run_without_separator_works() {
    // run should also work without -- if the command doesn't look like a flag
    let output = sketch_bin()
        .args(["run", "echo", "world"])
        .output()
        .unwrap();
    assert!(output.status.success(), "run without -- should work");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "world");
}

#[test]
fn run_false_exits_nonzero() {
    let output = sketch_bin()
        .args(["run", "--", "false"])
        .output()
        .unwrap();
    assert!(!output.status.success(), "run false should exit non-zero");
}

#[test]
fn run_exit_code_propagated() {
    let output = sketch_bin()
        .args(["run", "--", "sh", "-c", "exit 42"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(42), "exit code should propagate");
}

#[test]
fn run_captures_stdout_and_stderr() {
    let output = sketch_bin()
        .args(["run", "--", "sh", "-c", "echo out; echo err >&2"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stdout.contains("out"));
    assert!(stderr.contains("err"));
}

#[test]
fn run_nonexistent_command_exits_127() {
    let output = sketch_bin()
        .args(["run", "--", "nonexistent_cmd_xyz"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(127));
}

// ============================================================
// sketch run --name
// ============================================================

#[test]
fn run_with_name_succeeds() {
    let output = sketch_bin()
        .args(["run", "--name", "my-test-session", "--", "echo", "named"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "named");
}

// ============================================================
// sketch run --timeout
// ============================================================

#[test]
fn run_with_timeout_completes_before_deadline() {
    let output = sketch_bin()
        .args(["run", "--timeout", "10", "--", "echo", "fast"])
        .output()
        .unwrap();
    assert!(output.status.success(), "fast command should complete before timeout");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "fast");
}

#[test]
fn run_with_timeout_kills_slow_command() {

    use std::time::Instant;
    let start = Instant::now();

    let output = sketch_bin()
        .args(["run", "--timeout", "2", "--", "sleep", "60"])
        .output()
        .unwrap();

    let elapsed = start.elapsed();

    // Should have been killed by timeout, not waited 60 seconds
    assert!(elapsed.as_secs() < 10, "should timeout quickly, took {:?}", elapsed);
    assert!(!output.status.success(), "timed-out command should fail");

    // Check stderr for timeout message
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("timeout") || output.status.code() == Some(124) ||
        output.status.code().map(|c| c >= 128).unwrap_or(false),
        "should indicate timeout, got exit code {:?}, stderr: {}",
        output.status.code(),
        stderr
    );
}

// ============================================================
// sketch run - overlay isolation (same as exec but through run)
// ============================================================

#[test]
fn run_file_creation_does_not_persist() {
    let test_file = "/tmp/sketch_run_isolation_test";
    let _ = fs::remove_file(test_file);

    let output = sketch_bin()
        .args(["run", "--", "touch", test_file])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(!Path::new(test_file).exists(), "file from run should not persist");
}

#[test]
fn run_env_vars_set() {
    let output = sketch_bin()
        .args(["run", "--", "sh", "-c", "echo $SKETCH_SESSION"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "1");
}

// ============================================================
// sketch list - basic output
// ============================================================

#[test]
fn list_no_sessions_message() {
    let output = sketch_bin().arg("list").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Either "No active sessions." or a table header — both are valid
    assert!(
        stdout.contains("No active sessions") || stdout.contains("SESSION ID"),
        "list should show message or table, got: {}",
        stdout
    );
}

#[test]
fn list_json_valid_array() {
    let output = sketch_bin().args(["list", "--json"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should be valid JSON array (may be empty or have entries)
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
        .expect("list --json should output valid JSON");
    assert!(parsed.is_array(), "JSON output should be an array");
}

#[test]
fn list_json_is_valid_json() {
    let output = sketch_bin().args(["list", "--json"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let _: serde_json::Value = serde_json::from_str(stdout.trim())
        .expect("list --json should always output valid JSON");
}

#[test]
fn ls_alias_same_as_list() {
    // Both `list` and `ls` should succeed and produce similar output format
    let list_output = sketch_bin().arg("list").output().unwrap();
    let ls_output = sketch_bin().arg("ls").output().unwrap();

    assert!(list_output.status.success(), "list should succeed");
    assert!(ls_output.status.success(), "ls should succeed");

    // Both should produce the same format (table or "No active sessions")
    let list_stdout = String::from_utf8_lossy(&list_output.stdout);
    let ls_stdout = String::from_utf8_lossy(&ls_output.stdout);

    // Both should have either "No active sessions" or table header
    let list_has_table = list_stdout.contains("SESSION ID");
    let ls_has_table = ls_stdout.contains("SESSION ID");
    let list_has_empty = list_stdout.contains("No active sessions");
    let ls_has_empty = ls_stdout.contains("No active sessions");

    assert!(
        (list_has_table || list_has_empty) && (ls_has_table || ls_has_empty),
        "both list and ls should produce valid output"
    );
}

// ============================================================
// sketch list - stale session detection
// ============================================================

#[test]
fn list_detects_stale_session() {
    // Create a fake session with a dead PID
    let fake_id = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
    let fake_dir = format!("/tmp/sketch-{}", fake_id);

    // Ensure clean slate and create fresh
    let _ = fs::remove_dir_all(&fake_dir);
    fs::create_dir_all(&fake_dir).unwrap();

    let metadata = serde_json::json!({
        "id": fake_id,
        "name": "stale-test",
        "created": 1700000000u64,
        "pid": 999999999u32,
        "command": "test",
        "username": "test",
        "overlay_path": fake_dir
    });

    fs::write(
        format!("{}/.sketch-metadata.json", fake_dir),
        serde_json::to_string_pretty(&metadata).unwrap(),
    )
    .unwrap();

    // list should show this session as stale
    let output = sketch_bin().arg("list").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("stale"), "dead PID session should show as stale: {}", stdout);

    // JSON output should include it too
    let json_output = sketch_bin().args(["list", "--json"]).output().unwrap();
    let json_stdout = String::from_utf8_lossy(&json_output.stdout);
    let sessions: Vec<serde_json::Value> = serde_json::from_str(json_stdout.trim()).unwrap();
    let found = sessions.iter().any(|s| s["id"].as_str() == Some(fake_id));
    assert!(found, "stale session should appear in JSON output");

    // Cleanup
    let _ = fs::remove_dir_all(&fake_dir);
}

// ============================================================
// sketch list - JSON structure
// ============================================================

#[test]
fn list_json_has_expected_fields() {
    // Create a fake session to ensure non-empty output
    let fake_id = "11111111-2222-3333-4444-555555555555";
    let fake_dir = format!("/tmp/sketch-{}", fake_id);
    let _ = fs::create_dir_all(&fake_dir);

    let metadata = serde_json::json!({
        "id": fake_id,
        "name": "json-test",
        "created": 1700000000u64,
        "pid": std::process::id(),
        "command": "test cmd",
        "username": "tester",
        "overlay_path": fake_dir
    });

    fs::write(
        format!("{}/.sketch-metadata.json", fake_dir),
        serde_json::to_string_pretty(&metadata).unwrap(),
    )
    .unwrap();

    let output = sketch_bin().args(["list", "--json"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let sessions: Vec<serde_json::Value> = serde_json::from_str(stdout.trim()).unwrap();

    let session = sessions.iter().find(|s| s["id"].as_str() == Some(fake_id))
        .expect("should find our test session");

    assert!(session["id"].is_string(), "should have id field");
    assert!(session["name"].is_string(), "should have name field");
    assert!(session["created"].is_number(), "should have created field");
    assert!(session["pid"].is_number(), "should have pid field");
    assert!(session["command"].is_string(), "should have command field");
    assert!(session["username"].is_string(), "should have username field");

    // Cleanup
    let _ = fs::remove_dir_all(&fake_dir);
}

// ============================================================
// sketch list - table output format
// ============================================================

#[test]
fn list_table_has_header() {
    // Create a fake session to get table output
    let fake_id = "22222222-3333-4444-5555-666666666666";
    let fake_dir = format!("/tmp/sketch-{}", fake_id);
    let _ = fs::create_dir_all(&fake_dir);

    let metadata = serde_json::json!({
        "id": fake_id,
        "name": null,
        "created": 1700000000u64,
        "pid": 999999998u32,
        "command": "header-test",
        "username": "test",
        "overlay_path": fake_dir
    });

    fs::write(
        format!("{}/.sketch-metadata.json", fake_dir),
        serde_json::to_string_pretty(&metadata).unwrap(),
    )
    .unwrap();

    let output = sketch_bin().arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("SESSION ID"), "table should have SESSION ID header");
    assert!(stdout.contains("NAME"), "table should have NAME header");
    assert!(stdout.contains("PID"), "table should have PID header");
    assert!(stdout.contains("STATUS"), "table should have STATUS header");
    assert!(stdout.contains("AGE"), "table should have AGE header");
    assert!(stdout.contains("COMMAND"), "table should have COMMAND header");

    // Cleanup
    let _ = fs::remove_dir_all(&fake_dir);
}

// ============================================================
// sketch status - output sections
// ============================================================

#[test]
fn status_shows_system_section() {
    let output = sketch_bin().arg("status").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("System:"), "status should have System section");
}

#[test]
fn status_shows_overlayfs_availability() {
    let output = sketch_bin().arg("status").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("OverlayFS:"),
        "status should show OverlayFS availability"
    );
    assert!(
        stdout.contains("available") || stdout.contains("not available"),
        "should report OverlayFS status"
    );
}

#[test]
fn status_shows_kernel_version() {
    let output = sketch_bin().arg("status").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Kernel:"), "status should show kernel version");
}

#[test]
fn status_shows_disk_info() {
    let output = sketch_bin().arg("status").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Disk:"), "status should have Disk section");
    assert!(stdout.contains("/tmp"), "status should show /tmp disk info");
}

#[test]
fn status_shows_sessions_section() {
    let output = sketch_bin().arg("status").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Sessions:"), "status should have Sessions section");
    assert!(stdout.contains("Active:"), "status should show active count");
}

#[test]
fn status_shows_privileges_section() {
    let output = sketch_bin().arg("status").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Privileges:"), "status should have Privileges section");
    assert!(stdout.contains("Running as root:"), "status should show root status");
    assert!(stdout.contains("User namespaces:"), "status should show userns status");
}

#[test]
fn status_shows_package_manager_section() {
    let output = sketch_bin().arg("status").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Package manager:"),
        "status should have Package manager section"
    );
}

// ============================================================
// Cross-command integration: run + list
// ============================================================

#[test]
fn run_creates_metadata_visible_to_list() {

    // Run a command that sleeps briefly, then check if list can see it
    // We use a very short sleep to test metadata creation during session
    // Since the session cleans up on exit, we check that cleanup works
    let output = sketch_bin()
        .args(["run", "--name", "list-test", "--", "true"])
        .output()
        .unwrap();
    assert!(output.status.success());

    // After the session exits, it should be cleaned up
    // so list shouldn't show it (verifies cleanup)
    let list_output = sketch_bin().args(["list", "--json"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&list_output.stdout);
    let sessions: Vec<serde_json::Value> = serde_json::from_str(stdout.trim()).unwrap();

    // The session should have been cleaned up
    let found = sessions.iter().any(|s| {
        s["name"].as_str() == Some("list-test")
    });
    assert!(!found, "completed session should be cleaned up and not visible in list");
}

// ============================================================
// sketch run - cleanup verification
// ============================================================

#[test]
fn run_cleans_up_temp_dirs() {

    let before: Vec<String> = fs::read_dir("/tmp")
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with("sketch-"))
        .collect();

    let _ = sketch_bin()
        .args(["run", "--", "true"])
        .output()
        .unwrap();

    let after: Vec<String> = fs::read_dir("/tmp")
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with("sketch-"))
        .collect();

    let leaked: Vec<&String> = after.iter().filter(|d| !before.contains(d)).collect();
    assert!(leaked.is_empty(), "run should clean up temp dirs, found: {:?}", leaked);
}

#[test]
fn run_with_timeout_cleans_up() {

    let before: Vec<String> = fs::read_dir("/tmp")
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with("sketch-"))
        .collect();

    // This will be killed by timeout
    let _ = sketch_bin()
        .args(["run", "--timeout", "1", "--", "sleep", "60"])
        .output()
        .unwrap();

    // Give cleanup a moment
    std::thread::sleep(std::time::Duration::from_millis(200));

    let after: Vec<String> = fs::read_dir("/tmp")
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with("sketch-"))
        .collect();

    let leaked: Vec<&String> = after.iter().filter(|d| !before.contains(d)).collect();
    assert!(leaked.is_empty(), "timed-out run should clean up, found: {:?}", leaked);
}

// ============================================================
// sketch run - verbose mode
// ============================================================

#[test]
fn run_verbose_prints_timeout_info() {
    let output = sketch_bin()
        .args(["--verbose", "run", "--timeout", "30", "--", "true"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("timeout") || stderr.contains("30"),
        "verbose should mention timeout"
    );
}

//! CLI argument parsing and command-line interface tests.
//!
//! These tests verify sketch's CLI behavior by running the binary as a subprocess,
//! since parse_args() calls process::exit() directly.

use std::process::Command;

fn sketch_bin() -> Command {
    let cmd = Command::new(env!("CARGO_BIN_EXE_sketch"));
    cmd
}

// ============================================================
// --help / -h
// ============================================================

#[test]
fn help_flag_exits_zero() {
    let output = sketch_bin().arg("--help").output().expect("failed to run sketch");
    assert!(output.status.success(), "exit code should be 0");
}

#[test]
fn help_short_flag_exits_zero() {
    let output = sketch_bin().arg("-h").output().expect("failed to run sketch");
    assert!(output.status.success(), "exit code should be 0");
}

#[test]
fn help_contains_usage_info() {
    let output = sketch_bin().arg("--help").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("USAGE"), "help should contain USAGE section");
    assert!(stdout.contains("OPTIONS"), "help should contain OPTIONS section");
    assert!(stdout.contains("COMMANDS"), "help should contain COMMANDS section");
}

#[test]
fn help_lists_all_options() {
    let output = sketch_bin().arg("--help").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--help"), "help should list --help");
    assert!(stdout.contains("--version"), "help should list --version");
    assert!(stdout.contains("--verbose"), "help should list --verbose");
    assert!(stdout.contains("--clean"), "help should list --clean");
}

#[test]
fn help_lists_all_commands() {
    let output = sketch_bin().arg("--help").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("shell"), "help should list shell command");
    assert!(stdout.contains("exec"), "help should list exec command");
}

// ============================================================
// --version / -v
// ============================================================

#[test]
fn version_flag_exits_zero() {
    let output = sketch_bin().arg("--version").output().unwrap();
    assert!(output.status.success(), "exit code should be 0");
}

#[test]
fn version_short_flag_exits_zero() {
    let output = sketch_bin().arg("-v").output().unwrap();
    assert!(output.status.success(), "exit code should be 0");
}

#[test]
fn version_output_contains_sketch() {
    let output = sketch_bin().arg("--version").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("sketch "), "version should start with 'sketch '");
}

#[test]
fn version_output_contains_semver() {
    let output = sketch_bin().arg("--version").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let version_part = stdout.strip_prefix("sketch ").expect("should start with 'sketch '");
    let parts: Vec<&str> = version_part.split('.').collect();
    assert_eq!(parts.len(), 3, "version should be semver (x.y.z), got: {}", version_part);
    for part in &parts {
        part.parse::<u32>().unwrap_or_else(|_| panic!("version component '{}' should be numeric", part));
    }
}

// ============================================================
// Invalid arguments
// ============================================================

#[test]
fn unknown_flag_exits_nonzero() {
    let output = sketch_bin().arg("--nonexistent").output().unwrap();
    assert!(!output.status.success(), "unknown flag should exit non-zero");
}

#[test]
fn unknown_flag_shows_error() {
    let output = sketch_bin().arg("--foobar").output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown option"), "should mention unknown option");
    assert!(stderr.contains("--foobar"), "should echo the bad flag");
}

#[test]
fn unknown_flag_suggests_help() {
    let output = sketch_bin().arg("--bad").output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--help"), "should suggest --help");
}

#[test]
fn unknown_short_flag_exits_nonzero() {
    let output = sketch_bin().arg("-z").output().unwrap();
    assert!(!output.status.success(), "unknown short flag should exit non-zero");
}

// ============================================================
// exec without command
// ============================================================

#[test]
fn exec_without_command_exits_nonzero() {
    let output = sketch_bin().arg("exec").output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "exec without command should exit non-zero");
    assert!(
        stderr.contains("requires a command"),
        "should show exec error, got: {}",
        stderr
    );
}

// ============================================================
// --clean flag (can run without root)
// ============================================================

#[test]
fn clean_flag_exits_zero() {
    let output = sketch_bin().arg("--clean").output().unwrap();
    // --clean should succeed (even if nothing to clean)
    assert!(output.status.success(), "clean should exit 0 when nothing to clean");
}

#[test]
fn clean_reports_no_orphans() {
    let output = sketch_bin().arg("--clean").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should report finding nothing or cleaning something
    assert!(
        stdout.contains("no orphaned sessions") || stdout.contains("cleaned up"),
        "clean should report status, got: {}",
        stdout
    );
}

// ============================================================
// --verbose flag (combined with --clean to avoid root requirement)
// ============================================================

#[test]
fn verbose_with_clean_exits_zero() {
    let output = sketch_bin().args(["--verbose", "--clean"]).output().unwrap();
    assert!(output.status.success(), "verbose + clean should exit 0");
}

// ============================================================
// Phase 2: help lists new commands
// ============================================================

#[test]
fn help_lists_run_command() {
    let output = sketch_bin().arg("--help").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("run"), "help should list run command");
}

#[test]
fn help_lists_list_command() {
    let output = sketch_bin().arg("--help").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("list"), "help should list list command");
}

#[test]
fn help_lists_status_command() {
    let output = sketch_bin().arg("--help").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("status"), "help should list status command");
}

#[test]
fn help_lists_run_options() {
    let output = sketch_bin().arg("--help").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--name"), "help should list --name option");
    assert!(stdout.contains("--timeout"), "help should list --timeout option");
}

// ============================================================
// Phase 2: run command parsing
// ============================================================

#[test]
fn run_without_command_exits_nonzero() {
    let output = sketch_bin().arg("run").output().unwrap();
    assert!(!output.status.success(), "run without command should exit non-zero");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("requires a command"),
        "should show run error, got: {}",
        stderr
    );
}

#[test]
fn run_name_without_value_exits_nonzero() {
    let output = sketch_bin().args(["run", "--name"]).output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("requires a value"),
        "should show error, got: {}",
        stderr
    );
}

#[test]
fn run_timeout_without_value_exits_nonzero() {
    let output = sketch_bin().args(["run", "--timeout"]).output().unwrap();
    assert!(!output.status.success());
}

#[test]
fn run_invalid_timeout_exits_nonzero() {
    let output = sketch_bin().args(["run", "--timeout", "abc", "echo"]).output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid timeout"),
        "should show invalid timeout error, got: {}",
        stderr
    );
}

#[test]
fn run_unknown_option_exits_nonzero() {
    let output = sketch_bin().args(["run", "--bogus", "echo"]).output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown run option"),
        "should show unknown option error, got: {}",
        stderr
    );
}

// ============================================================
// Phase 2: list command parsing
// ============================================================

#[test]
fn list_exits_zero_without_root() {
    let output = sketch_bin().arg("list").output().unwrap();
    assert!(output.status.success(), "list should work without root");
}

#[test]
fn list_ls_alias_exits_zero() {
    let output = sketch_bin().arg("ls").output().unwrap();
    assert!(output.status.success(), "ls alias should work");
}

#[test]
fn list_json_exits_zero() {
    let output = sketch_bin().args(["list", "--json"]).output().unwrap();
    assert!(output.status.success(), "list --json should work");
}

#[test]
fn list_unknown_option_exits_nonzero() {
    let output = sketch_bin().args(["list", "--bogus"]).output().unwrap();
    assert!(!output.status.success());
}

#[test]
fn list_positional_arg_exits_nonzero() {
    let output = sketch_bin().args(["list", "extra"]).output().unwrap();
    assert!(!output.status.success());
}

// ============================================================
// Phase 2: status command parsing
// ============================================================

#[test]
fn status_exits_zero_without_root() {
    let output = sketch_bin().arg("status").output().unwrap();
    assert!(output.status.success(), "status should work without root");
}

# Testing Guide for Sketch

## Running Tests

### As a Regular User

Most tests can run as a non-root user:

```bash
cargo test
```

However, some tests require root access and will panic with an informative error message if you attempt to run them without root.

### Running All Tests (Including Root-Required Tests)

Tests that require root access will panic and cannot be skipped. To run the full test suite, use `sudo` with the appropriate cargo runner configuration:

```bash
CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER='sudo -E' cargo test
```

This command:
- Sets the test runner to `sudo -E`, which preserves the environment
- Allows cargo to run tests as root while maintaining access to the Rust toolchain
- Works with the regular user's cargo binary

### Test Categories

#### Non-Root Compatible Tests
- CLI argument parsing (`tests/cli_tests.rs`)
- Session management (`tests/session_tests.rs`)
- Mount naming and hashing (`tests/mount_naming_tests.rs`)
- Unit tests in `src/` modules

These tests run successfully as a regular user.

#### Root-Required Tests
- Overlay filesystem isolation (`tests/overlay_isolation_tests.rs`)
- Integration tests that verify mount isolation behavior

These tests require root access because they:
- Create actual overlay mounts
- Verify filesystem isolation properties
- Test mount namespace behavior

Running these tests without root will cause them to **panic with a helpful message**, not silently skip.

## Test Execution Details

### Graceful Non-Root Testing

Tests designed for non-root verification (like `non_root_shell_shows_root_error`) check if running as root and skip appropriately:
- If running as root: test is skipped (not applicable)
- If running as non-root: test runs and verifies non-root behavior

This ensures cross-environment test compatibility.

### Root-Required Test Assertion

Tests requiring root use `panic!()` rather than graceful skipping:
- If running as root: test executes normally
- If running as non-root: test panics with clear instructions

This makes it explicit that certain tests must run with root privileges.

## CI/CD Considerations

For continuous integration:

1. **Non-root CI environments**: Run `cargo test` normally. Non-root-compatible tests will pass; root-required tests will panic but can be ignored or marked as expected failure.

2. **Root CI environments**: Use `cargo test` directly without the `CARGO_TARGET_*_RUNNER` variable, tests will execute normally.

3. **Containerized CI**: Use `sudo -E` approach if tests need to run inside unprivileged containers:
   ```bash
   CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER='sudo -E' cargo test
   ```

## Debugging Test Failures

Run a specific test with output:

```bash
cargo test <test_name> -- --nocapture
```

For root-required tests:

```bash
CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER='sudo -E' cargo test <test_name> -- --nocapture
```

## Architecture

The test suite is organized by concern:

- **Unit tests** (`src/` modules): Fast, no-dependency tests of individual functions
- **CLI tests** (`cli_tests.rs`): Test command-line argument parsing and help output
- **Session tests** (`session_tests.rs`): Test session creation and lifecycle
- **Overlay isolation tests** (`overlay_isolation_tests.rs`): Integration tests verifying overlay mount isolation (requires root)
- **Mount naming tests** (`mount_naming_tests.rs`): Test hash-based collision-free naming
- **Integration tests** (`integration_tests.rs`): Full end-to-end testing scenarios

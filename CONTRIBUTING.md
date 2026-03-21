# Contributing to Sketch

Thank you for your interest in contributing! This guide covers how to develop and test Sketch.

## Development Setup

### Requirements

- Rust 1.70+ (check with `rustc --version`)
- Linux with OverlayFS support
- Root access (recommended for testing)

### Build

```bash
cargo build          # Debug build
cargo build --release  # Optimized release build
```

### Test

```bash
cargo test           # All tests
cargo test --lib    # Unit tests only
cargo test --test   # Integration tests only
```

**Note:** Integration tests require root and will be skipped if running as non-root with `SKIPPED: requires root` message.

To run as root:

```bash
sudo cargo test
```

## Project Structure

```
src/
├── main.rs              # Entry point, command dispatch
├── cli.rs               # Command-line argument parsing
├── session.rs           # Session lifecycle and management
├── overlay.rs           # OverlayFS mounting and cleanup
├── fs_utils.rs          # Filesystem utilities
├── userns.rs            # User namespace setup
├── metadata.rs          # Session metadata and listing
├── package.rs           # Package manager detection
└── main.rs

tests/
├── session_tests.rs           # Session lifecycle tests
├── overlay_isolation_tests.rs  # Overlay isolation verification
└── mount_naming_tests.rs       # Mount naming (collision prevention)
```

## Architecture

See [ARCHITECTURE.md](ARCHITECTURE.md) for detailed design documentation.

**Key Components:**
1. **OverlayFS Mounting** — `overlay.rs`
2. **Session Lifecycle** — `session.rs`
3. **Namespace Setup** — `userns.rs`, methods in `overlay.rs`
4. **CLI & Configuration** — `cli.rs`, `main.rs`

## Code Style

- Use `cargo fmt` for formatting
- Follow Rust API guidelines
- Prefer `Result<T, String>` for error handling (match on string messages)
- Add doc comments for public functions

Format your code:

```bash
cargo fmt
```

Check for issues:

```bash
cargo clippy
```

## Making Changes

### 1. Create a Feature Branch

```bash
git checkout -b feature/my-feature
git checkout -b fix/my-bug
```

### 2. Implement Changes

Make sure to:
- Keep changes focused and minimal
- Add tests for new functionality
- Update relevant documentation
- Use descriptive commit messages

### 3. Test Thoroughly

```bash
# Unit tests
cargo test --lib

# Integration tests (as root)
sudo cargo test --test

# Run specific test
cargo test test_mount_name_avoids_collisions
```

**Test Coverage Needed For:**
- New mount strategies
- Session lifecycle changes
- CLI parsing changes
- Error handling paths

### 4. Update Documentation

If adding features or changing behavior:
- Update `USER_GUIDE.md` for user-facing changes
- Update `ARCHITECTURE.md` for design changes
- Add code comments for complex logic

## Common Tasks

### Adding a New Command

1. Add variant to `Command` enum in `cli.rs`
2. Add parsing logic in `parse_args()`
3. Add handler in `main.rs`
4. Update help text
5. Add tests in `tests/session_tests.rs`

Example: Adding `sketch foo` command:

**cli.rs:**
```rust
#[derive(Debug)]
pub enum Command {
    // ...
    Foo(String),
}

// In parse_args:
"foo" => {
    if positional.len() < 2 {
        eprintln!("sketch: 'foo' requires an argument");
        process::exit(1);
    }
    Command::Foo(positional[1].clone())
}
```

**main.rs:**
```rust
cli::Command::Foo(arg) => {
    handle_foo(&arg)?;
}

fn handle_foo(arg: &str) -> Result<(), String> {
    // Implementation
    Ok(())
}
```

### Adding Mount Support

If adding support for additional filesystems or mounts:

1. Update `mount_additional_filesystems()` skip conditions if needed
2. Add logic in `mount_virtual_filesystems()` or new method
3. Add cleanup in `cleanup()`
4. Add tests to verify isolation

### Improving Error Messages

When you encounter unhelpful errors:

1. Add context to the error string
2. Suggest remediation (e.g., "Try running with sudo")
3. Reference `sketch status` or `sketch --help` when relevant

Example:

**Bad:**
```rust
Err("Failed to mount overlay".into())
```

**Good:**
```rust
Err(format!(
    "Failed to mount overlay: {} (try 'sketch status' to verify system support)",
    e
))
```

## Testing Guidelines

### Unit Tests

Place in-file for isolated logic:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mount_name_is_deterministic() {
        let name1 = mount_name_from_path("/home/user");
        let name2 = mount_name_from_path("/home/user");
        assert_eq!(name1, name2);
    }
}
```

### Integration Tests

Use `tests/` directory for full session tests:

```rust
#[test]
fn my_feature_works() {
    if !is_root() {
        eprintln!("SKIPPED: requires root");
        return;
    }
    let output = sketch_bin().args(["exec", "echo", "hello"]).output().unwrap();
    assert!(output.status.success());
}
```

### Test Coverage

Focus on:
- **Happy path** — Normal operation works
- **Error cases** — Graceful failure
- **Edge cases** — Empty inputs, large files, special characters
- **Concurrent access** — Multiple sessions don't interfere
- **Cleanup** — Resources released properly

## Performance Considerations

When making changes, consider:

1. **Session Startup Time** — Minimize overlay setup work
2. **Filesystem Operations** — OverlayFS has inherent overhead
3. **Memory Usage** — Avoid large buffers for every session
4. **Disk I/O** — Session directory thrashing

Profile with:

```bash
time sketch shell < /dev/null  # Session startup time
```

## Known Issues & Limitations

See memory files for current task list and known issues:

```bash
ls -la ~/.claude/projects/-home-ipravd-src-sketch/memory/
```

## Pull Request Process

1. **Fork & Branch** — Create feature branch from `main`
2. **Code & Test** — Implement with tests
3. **Document** — Update relevant docs
4. **Clean History** — Rebase for clean commits
5. **Submit PR** — Link to issues if relevant

## Questions?

- Check `ARCHITECTURE.md` for design details
- Run `sketch --help` for usage
- Read code comments and docstrings
- Check test files for examples

## Code of Conduct

- Be respectful and inclusive
- Assume good intent
- Give and accept constructive feedback
- Report issues professionally

## License

Sketch is [MIT licensed](LICENSE). Contributions imply agreement to these terms.

---

Happy coding! We appreciate all contributions, from bug fixes to new features to documentation improvements.

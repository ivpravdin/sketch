# Contributing

## Getting Started

1. Clone the repository
2. Install Rust (1.85+)
3. Build: `cargo build`
4. Test: `cargo test`

## Project Structure

```
sketch/
├── Cargo.toml              # Dependencies and package metadata
├── src/
│   ├── main.rs             # Entry point, root check, command dispatch
│   ├── cli.rs              # Argument parsing, help text
│   ├── overlay.rs          # OverlayFS mount/unmount, namespace setup
│   ├── session.rs          # Session lifecycle, fork/exec, signal handling
│   ├── fs_utils.rs         # Filesystem utilities, environment setup
│   └── package.rs          # Package manager detection and integration
├── tests/
│   ├── cli_tests.rs        # CLI argument parsing tests
│   ├── overlay_tests.rs    # Overlay lifecycle tests
│   ├── session_tests.rs    # Session management tests
│   └── integration_tests.rs # End-to-end integration tests
├── docs/                   # Documentation
└── ARCHITECTURE.md         # Design decisions and rationale
```

## Development

### Building

```bash
cargo build
```

### Testing

Testing requires root privileges since the core functionality uses OverlayFS:

```bash
sudo cargo test
```

### Running Locally

```bash
cargo build && sudo ./target/release/sketch --verbose
```

## Guidelines

- Keep dependencies minimal. Sketch is intentionally lightweight.
- All overlay/mount code goes in `overlay.rs`. Session orchestration goes in `session.rs`. Filesystem helpers go in `fs_utils.rs`. Package manager logic goes in `package.rs`.
- Error messages should be clear and actionable (e.g., suggest `sudo` when privilege is missing).
- Safety is paramount. The host filesystem must never be modified during a session.
- Test cleanup paths thoroughly — bugs in cleanup can leave orphaned mounts.

## Pull Requests

1. Fork and create a feature branch
2. Make your changes
3. Ensure `cargo build` succeeds with no warnings
4. Test manually with `sudo ./target/release/sketch`
5. Submit a pull request with a clear description of the change

## Reporting Issues

When reporting an issue, include:

- Linux distribution and kernel version (`uname -a`)
- Rust version (`rustc --version`)
- Output of `sudo sketch --verbose` showing the failure
- Filesystem type of `/tmp` (`df -T /tmp`)

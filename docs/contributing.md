# Contributing

## Getting Started

1. Clone the repository
2. Install Rust (1.85+)
3. Build: `cargo build`
4. Test: `cargo test`

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

I would really like help developing comprehensive testing suite. If you have any feedback or idea,
please let me know!

### Running Locally

```bash
cargo build && sudo ./target/release/sketch --verbose
```

## Guidelines

- Keep dependencies minimal. Sketch is intentionally lightweight. It also prevents any security concerns.
- All overlay/mount code goes in `overlay.rs`. Session orchestration goes in `session.rs`.
- Error messages should be clear and actionable (e.g., suggest `sudo` when privilege is missing).
- Safety is paramount. The host filesystem must never be modified during a session.

## Pull Requests

1. Fork and create a feature branch on top of `dev`
2. Make your changes
3. Ensure `cargo build` succeeds with no warnings
4. Test manually with `sudo ./target/release/sketch`
5. Submit a pull request to `dev` with a clear description of the change

## Reporting Issues

When reporting an issue, include:

- Linux distribution and kernel version (`uname -a`)
- Rust version (`rustc --version`)
- Output of `sudo sketch --verbose` showing the failure
- Filesystem type of `/tmp` (`df -T /tmp`)

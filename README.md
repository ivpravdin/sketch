# Sketch

Sketch is a lightweight CLI tool that provides ephemeral, disposable machine sessions using temporary filesystem overlays. Run a single command to enter a fully writable environment where you can install packages, modify system files, and experiment freely — all changes are discarded when you exit.

## Quick Start

Sketch requires root/sudo because it uses kernel-level OverlayFS and mount namespaces for filesystem isolation.

```bash
# Build
cargo build --release

# Start an ephemeral session
sudo sketch
# Now in ephemeral environment — install anything, break anything
apt update && apt install -y nodejs
exit
# All changes discarded — host unchanged
```

## Features

- **Instant startup** — launches in under a second
- **Full system access** — see and modify any file on the host
- **Automatic cleanup** — all changes vanish on exit, even after crashes
- **No configuration** — no images, no Dockerfiles, no state to manage
- **Package management** — apt, dnf, yum, pacman, zypper, and apk all work within the session

## Requirements

- Linux kernel 4.0+ (for OverlayFS support)
- Root privileges (sudo) — required for OverlayFS mounting, mount namespace isolation, and pivot_root
- Rust toolchain (for building)

## Usage

```bash
# Interactive shell (default)
sudo sketch

# Run a single command
sudo sketch exec apt install -y nginx

# Clean up orphaned sessions
sudo sketch --clean

# Verbose output
sudo sketch --verbose
```

## Documentation

- [Installation Guide](docs/installation.md)
- [Usage Guide](docs/usage.md)
- [Architecture](docs/architecture.md)
- [Safety Guarantees](docs/safety.md)
- [Troubleshooting](docs/troubleshooting.md)
- [Contributing](docs/contributing.md)

## How It Works

Sketch uses Linux OverlayFS to layer a temporary writable directory on top of your root filesystem. Your host files are the read-only lower layer; all modifications go to a temporary upper layer in `/tmp`. When the session ends, the temporary directory is deleted and your system is unchanged.

## License

MIT

# Installation

## System Requirements

- **Linux** with kernel 4.0 or later (for OverlayFS support)
- **Rust toolchain** (rustc 1.85+, cargo)
- **Root privileges** (sudo) — required for:
  - OverlayFS mounting (CAP_SYS_ADMIN)
  - Mount namespace isolation (CAP_SYS_ADMIN)
  - pivot_root (CAP_SYS_ADMIN)

Future versions may support unprivileged operation via user namespaces.

### Check your kernel version

```bash
uname -r
```

### Verify OverlayFS support

```bash
grep overlay /proc/filesystems
```

If missing, load the module:

```bash
sudo modprobe overlay
```

OverlayFS has been in the mainline Linux kernel since version 3.18. All modern distributions (Ubuntu 16.04+, Fedora 23+, Debian 9+, Arch, etc.) include it.

### Install Rust (if needed)

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

## Building from Source

```bash
git clone <repository-url>
cd sketch
cargo build --release
```

The binary will be at `./target/release/sketch`.

## Installation

Copy the binary to a location in your PATH:

```bash
sudo install target/release/sketch /usr/local/bin/
```

## Verifying the Installation

```bash
sketch --version
sketch --help
```

## Uninstallation

```bash
sudo rm /usr/local/bin/sketch
```

To clean up any orphaned session directories:

```bash
sudo sketch clean
```

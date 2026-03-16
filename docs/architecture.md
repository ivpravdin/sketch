# Architecture

## Overview

Sketch creates ephemeral system sessions using Linux OverlayFS. It layers a temporary writable directory on top of the host root filesystem, isolates it in a mount namespace, and spawns a shell or command within it. On exit, the temporary layer is deleted.

## Core Components

### CLI (`src/cli.rs`)

Hand-rolled argument parser (no external dependencies). Parses options (`--help`, `--version`, `--verbose`, `--clean`) and subcommands (`shell`, `exec`). Returns a `Config` struct with the parsed `Command` variant and flags.

### Overlay (`src/overlay.rs`)

Manages the OverlayFS lifecycle:

1. **`OverlaySession::new()`** — Creates temp directories under `/tmp/sketch-<UUID>/`:
   - `upper/` — writable layer where all changes are stored
   - `work/` — OverlayFS internal working directory
   - `merged/` — mount point where the overlay is assembled

2. **`setup_namespaces()`** — Creates a new mount namespace via `unshare(CLONE_NEWNS)` and marks all mounts as private to prevent propagation to the host.

3. **`mount_overlay()`** — Mounts the OverlayFS with `lowerdir=/` (host root, read-only) and `upperdir`/`workdir` pointing to the temp directories.

4. **`mount_virtual_filesystems()`** — Mounts `/proc`, `/sys`, `/dev` (and optionally `/dev/pts`, `/dev/shm`) inside the merged root so the session has access to hardware and process information.

5. **`pivot_root()`** — Switches the process root to the merged overlay directory and detaches the old root.

6. **`cleanup()`** — Unmounts virtual filesystems and the overlay, then removes temp directories. Also runs automatically via `Drop`.

7. **`clean_orphaned()`** — Scans `/tmp` for leftover `sketch-*` directories, unmounts them, and removes them.

### Session (`src/session.rs`)

Orchestrates the session lifecycle:

1. Creates an `OverlaySession`
2. Calls `setup()` — namespaces, overlay mount, virtual FS mount, pivot root
3. Forks the process
4. **Child**: sets `SKETCH_SESSION=1`, updates `PS1`, then `execvp`s the shell/command
5. **Parent**: ignores signals (child receives them directly), waits for child to exit, then cleans up

### Filesystem Utilities (`src/fs_utils.rs`)

Helper functions for filesystem operations within sessions:

- **`resolve_path()`** / **`read_symlink()`** — Path resolution and symlink handling
- **`create_temp_dir()`** / **`create_temp_file()`** — Temporary file creation within the overlay
- **`set_permissions()`** / **`file_info()`** — Permission management and metadata inspection
- **`setup_working_directory()`** — Preserves the user's working directory inside the session, falling back to `$HOME` or `/`
- **`setup_session_env()`** — Prepares environment variables for the session, setting `SKETCH_SESSION=1`, `SKETCH_ORIGINAL_UID`, `SKETCH_ORIGINAL_GID`, and preserving important host variables (`HOME`, `USER`, `SHELL`, `TERM`, `PATH`, etc.)
- **`check_device_access()`** — Verifies essential devices (`/dev/null`, `/dev/zero`, `/dev/urandom`, `/dev/tty`) are accessible
- **`copy_dir_recursive()`** — Recursive directory copy with permission preservation

### Package Management (`src/package.rs`)

Provides a unified interface for package management across Linux distributions:

- **Auto-detection**: `detect_package_manager()` checks for known binaries (`apt-get`, `dnf`, `yum`, `pacman`, `zypper`, `apk`) and returns the appropriate variant
- **Unified operations**: Each `PackageManager` variant generates correct arguments for install, remove, and update operations
- **Cache management**: `clean_package_cache()` runs the appropriate cache cleanup command, and `cache_dirs()` returns distribution-specific cache paths
- **Supported managers**: apt (Debian/Ubuntu), dnf (Fedora), yum (RHEL/CentOS), pacman (Arch), zypper (openSUSE), apk (Alpine)

### Entry Point (`src/main.rs`)

Checks for root privileges, parses CLI args, and dispatches to the appropriate session command or cleanup routine.

## Filesystem Layout During a Session

```
/tmp/sketch-<UUID>/
├── upper/               # Copy-on-write layer (modifications go here)
├── work/                # OverlayFS metadata
└── merged/              # The assembled overlay (becomes new /)
    ├── bin/             # From host (read-only)
    ├── etc/             # From host (read-only until modified)
    ├── usr/             # From host (read-only until modified)
    └── ...              # Any changes stored in upper/ transparently
```

## Session Lifecycle

```
sketch invoked
  → check root privileges
  → parse CLI arguments
  → create /tmp/sketch-<UUID>/{upper,work,merged}
  → unshare(CLONE_NEWNS) — new mount namespace
  → mount overlay (lower=/, upper=tmp/upper) at tmp/merged
  → mount /proc, /sys, /dev inside merged
  → pivot_root to merged, detach old root
  → fork()
     ├─ child: execvp(shell or command)
     └─ parent: wait for child
  → child exits
  → parent: unmount overlay, remove temp dirs
```

## Design Decisions

**Why OverlayFS?** Built into the Linux kernel since 3.18, zero-copy reads, minimal overhead. No external dependencies or additional kernel modules needed.

**Why fork+exec instead of just exec?** The parent process needs to survive to perform cleanup after the child exits. Using `fork()` lets the parent wait and then unmount/delete temp directories.

**Why not containers?** Sketch intentionally avoids the complexity of container runtimes. No images, no registries, no configuration files. Just a single command for instant ephemeral access.

**Why root required?** OverlayFS mounting, mount namespace creation, and `pivot_root` all require `CAP_SYS_ADMIN`. Future work may explore user namespaces for unprivileged operation.

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
└── docs/                   # Documentation
```

## Dependencies

- `nix` — Safe Rust bindings for Linux system calls (mount, unshare, fork, pivot_root, signal handling)
- `tempfile` — Temporary file/directory creation
- `uuid` — Session ID generation
- `libc` — Low-level C library bindings

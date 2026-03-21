# Sketch Architecture

## Overview

Sketch creates ephemeral sessions using Linux namespaces and OverlayFS. Each session:
1. Creates a private mount namespace
2. Mounts an overlay filesystem stack
3. Pivots root into the overlayed view
4. Spawns a process in that environment

When the session ends, all overlays are unmounted and temporary directories cleaned up.

## Core Components

### 1. OverlayFS Mount Stack

**Single Overlay (Root)**
- `lowerdir=/` (original filesystem, read-only)
- `upperdir=.../upper` (session-local modifications)
- `workdir=.../work` (overlay scratch space)
- `merged_dir=.../merged` (final overlayed view)

**Per-Mount Overlays (Additional Filesystems)**
- For each mounted filesystem (e.g., `/home`, `/var`, `/data`):
  - `lowerdir=/path` (original mount)
  - `upperdir=.../upper/<hash>` (session-local changes)
  - `workdir=.../work/<hash>` (overlay scratch)

**Hash-Based Naming**
- Mount paths → SHA256 hash → unique directory names
- Prevents collisions (e.g., `/home/user` vs `/home_user`)
- Deterministic across sessions

### 2. Namespace Isolation

**Mount Namespace (`CLONE_NEWNS`)**
- Isolates all mount operations to this session
- Changes don't propagate to host
- Mounts set to `MS_REC | MS_PRIVATE`

**User Namespace (Optional, for non-root)**
- Grants `CAP_SYS_ADMIN` needed for overlayfs
- Maps user to root inside namespace
- Enables non-root sessions

**Not Used**
- Network namespace — sessions share host network
- PID namespace — sessions see host processes
- IPC namespace — sessions share host IPC
- UTS namespace — hostname is shared

### 3. Directory Structure

Per-session in `/tmp/sketch-{UUID}/`:

```
/tmp/sketch-{UUID}/
├── upper/                 # Overlay upper directories
│   ├── {hash1}/           # /home modifications
│   ├── {hash2}/           # /var modifications
│   └── etc                # /etc modifications (root)
├── work/                  # Overlay work directories (internal)
│   ├── {hash1}/
│   ├── {hash2}/
│   └── etc
├── merged/                # Final overlayed filesystem (root)
│   ├── etc
│   ├── home
│   ├── var
│   ├── mnt                # Old root after pivot_root
│   └── ...
└── .sketch-metadata.json  # Session metadata
└── .sketch-commit         # Files to commit on exit (optional)
```

### 4. Pivot Root

After mounting overlays:

```
pivot_root(merged, merged/mnt)
```

Results in:
- `/` now points to `merged` (overlayed filesystem)
- Old root mounted at `/mnt`
- Process sees overlayed view

On cleanup, old root is unmounted with `MNT_DETACH`.

## Session Lifecycle

### 1. Session Creation

```
Session::new()
├── Create session directory in /tmp
├── Create upper, work, merged directories
└── Store metadata
```

### 2. Namespace Setup

```
OverlaySession::setup_namespaces()
├── Check if running as root
├── If non-root:
│   └── Create user namespace (privilege escalation)
├── Create mount namespace
└── Set mounts to MS_PRIVATE (prevent propagation)
```

### 3. Mount Overlay

```
OverlaySession::mount_overlay()
└── Mount overlay (lowerdir=/, upperdir, workdir) → merged
```

### 4. Mount Virtual Filesystems

```
OverlaySession::mount_virtual_filesystems()
├── Mount proc, sysfs, devtmpfs
├── Bind-mount /dev/pts, /dev/shm
└── Bind-mount /run (for systemd)
```

### 5. Mount Additional Filesystems

```
OverlaySession::mount_additional_filesystems()
├── Parse /proc/self/mounts
├── For each real filesystem (skip virtual, /proc, /sys, etc):
│   ├── Generate hash-based mount name
│   ├── Create upper/{hash}, work/{hash}
│   └── Mount overlay (lowerdir=/path, upperdir={hash}, workdir={hash})
└── Track all extra mounts for cleanup
```

### 6. DNS Configuration

```
OverlaySession::ensure_dns_resolution()
├── Check if merged resolv.conf is readable
├── If not:
│   └── Copy host /etc/resolv.conf to upper/etc/resolv.conf
└── Ensures DNS works inside session
```

### 7. Package Manager Setup

```
Session::prepare_package_managers()
├── Detect system package manager (apt, dnf, pacman, etc.)
├── Create state directories in upper layer
│   (e.g., /var/cache/apt, /var/lib/rpm)
└── Ensures package manager can write during session
```

### 8. Pivot Root

```
OverlaySession::pivot_root()
├── Create merged/mnt directory
├── pivot_root(merged, merged/mnt)
├── chdir to /
└── Unmount old root
```

### 9. Spawn Process

```
Session::run_command()
├── Fork
├── In child:
│   ├── Set environment variables (SKETCH_SESSION=1, etc.)
│   ├── Set working directory
│   ├── exec(cmd)
└── In parent:
    ├── Wait for child
    ├── Process commits
    ├── Cleanup overlays
    └── Return exit code
```

### 10. Commit Files

```
Session::commit_marked_files()
├── Read .sketch-commit file
├── For each marked file:
│   ├── Find in upper/{path}/
│   ├── Copy to real /{path}
│   └── Log result
└── Log total committed
```

### 11. Cleanup

```
OverlaySession::cleanup()
├── Unmount extra mounts (best effort)
├── Unmount virtual filesystems
├── Unmount overlay
└── Remove session directory
```

## Key Design Decisions

### 1. Per-Mount Overlays

**Why:** Different block devices can't share a single overlay. Each mount point needs its own overlay stack.

**Alternative:** Only root overlay, but additional mounts would need bind-mounts (not truly isolated).

### 2. Hash-Based Naming

**Why:** Prevents collisions between mount paths that differ only in `/` vs `_`.

**Example:** Without hashing:
- `/home/user` → `home_user`
- `/home_user` (if existed) → `home_user` ← collision!

With hashing, each path gets unique directory.

### 3. Lazy Unmounting

**Why:** Some mounts may have processes accessing them. `MNT_DETACH` flag allows unmount to proceed without blocking.

**Trade-off:** Old root not truly gone until last reference released.

### 4. Pivot Root Strategy

**Why:** Completely replaces root filesystem, ensuring process sees overlayed view.

**Alternative:** chroot would be simpler but allows escapes.

### 5. Namespace Isolation Over Container

**Why:** Minimal overhead, just mount isolation.

**Why Not Container:** Much heavier (runc, cgroups, etc.) for simple filesystem isolation.

### 6. /tmp Storage

**Why:** Temporary, writable, standard location.

**Alternative:** Allow custom `--session-dir` for read-only /tmp systems.

## Performance Characteristics

- **Session Startup:** ~1 second (creating/mounting overlays)
- **File Read:** Minimal overhead (overlayfs optimized for read-through)
- **File Write:** Write-once to upper directory
- **Metadata:** Minimal (JSON file per session)
- **Disk:** Proportional to modified data (not copied files)

## Security Scope

**What's Isolated:**
- Filesystem modifications (overlayed)
- File creation/deletion
- Directory structure changes

**What's NOT Isolated:**
- Network (shared with host)
- Processes (can see all host processes)
- IPC (can communicate with host)
- Devices (can access /dev, but safe mounts)

**Implications:**
- Safe for testing commands, configuration, packages
- NOT safe for running untrusted code
- Process can still `ptrace` host processes
- No resource limits (use cgroups for that)

## Extension Points

### 1. Additional Namespace Support

Could add:
- Network namespace → full network isolation
- PID namespace → isolated process tree
- IPC namespace → isolated message queues

### 2. Cgroup Resource Limits

- Memory limits (`--memory 512M`)
- CPU limits (`--cpu 1`)
- I/O limits

### 3. Advanced Mount Strategies

- Squashfs read-only bases for faster startup
- Network mounts (NFS, SMB)
- Encrypted overlays

### 4. Session Persistence

- Save session state to archive
- Migrate sessions between systems
- Restore from snapshot

## Testing

**Unit Tests:**
- Mount name hashing (determinism, collision-free)
- Command parsing

**Integration Tests:**
- Overlay isolation (changes don't persist)
- Multiple concurrent sessions (no interference)
- File I/O and permissions
- Package manager support

**Manual Testing:**
- Destructive operations (`rm -rf /var/*`)
- Large file writes
- Network operations
- Signal handling

## Future Work

1. **Improve Error Messages** — More specific failure causes
2. **Performance Profiling** — Identify bottlenecks
3. **Additional FS Support** — Better handling of NFS, FUSE
4. **Session Sharing** — Export session for inspection on another system
5. **Declarative Config** — Config files for automated session setup

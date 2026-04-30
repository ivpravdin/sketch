# Safety Guarantees

Sketch is designed to make experimentation safe. Here are the guarantees it provides and how they are enforced.

## 1. No Persistent Changes

**Guarantee**: All modifications made during a session are discarded when the session ends.

**How**: All writes go to a temporary `upper/` directory layered via OverlayFS. When the session exits, the overlay is unmounted and the temp directory is deleted. The host filesystem (the `lowerdir`) is never written to.

## 2. Host Isolation

**Guarantee**: A sketch session cannot modify the host's `/`, `/etc`, `/var`, or any other directory on the real filesystem.

**How**: The session runs inside a separate mount namespace created with `unshare(CLONE_NEWNS)`. All mount operations are private to this namespace and invisible to the host. After `pivot_root`, the old root is detached entirely.

## 3. Atomic Cleanup

**Guarantee**: Temporary files are cleaned up even if the session terminates abnormally (crash, `kill`, power loss).

**How**:
- The `OverlaySession` struct implements `Drop`, so cleanup runs when the struct is deallocated.
- The parent process catches the child's exit (even via signals) and runs cleanup before exiting.
- For catastrophic failures (power loss, `kill -9` on the parent), orphaned directories persist in `/tmp` but can be cleaned with `sketch clean`.
- The OS will also reclaim `/tmp` on reboot.

## 4. Filesystem Consistency

**Guarantee**: The host filesystem continues operating normally during and after a session.

**How**: OverlayFS uses copy-on-write semantics. When you modify a file inside the session, the original file is copied to the upper layer and the modification is applied there. The lower layer (host) is never touched. Other processes on the host see the original files without interruption.

## What Sketch Does NOT Isolate

- **Network**: Sessions use the host's network stack. Network requests (HTTP, DNS, etc.) reach the real network.
- **Processes**: Processes started inside the session are real processes visible to the host (via `ps`, `top`, etc.). They terminate when the session ends.
- **Hardware**: `/dev` is bind-mounted from the host. Device access is real.
- **Kernel state**: Kernel parameters modified via `/proc/sys` or `sysctl` may affect the host (these are not filesystem-backed in the overlay sense).

## Risk Scenarios

| Scenario | Outcome |
|---|---|
| `rm -rf /` inside session | Session filesystem destroyed, host untouched |
| `apt install nginx` inside session | Nginx installed in overlay only, gone on exit |
| Edit `/etc/passwd` inside session | Change exists only in overlay, host users unaffected |
| Session crashes | `Drop` handler cleans up; if that fails, `clean` recovers |
| Power loss during session | Orphaned dirs in `/tmp`; cleaned on reboot or via `clean` |
| `iptables` rules changed in session | These affect the host (network is not isolated) |
| Write to `/dev/sda` inside session | This affects the host (device access is real) |

## Privilege Model

Sketch requires root to function because OverlayFS mounting, `unshare(CLONE_NEWNS)`, and `pivot_root` all require `CAP_SYS_ADMIN`. The spawned shell inside the session also runs as root.

Future versions may use user namespaces to allow unprivileged operation, mapping the user to root inside the session without actual host root access.

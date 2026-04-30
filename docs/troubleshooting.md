# Troubleshooting

## Common Issues

### "sketch: must be run as root (try: sudo sketch)"

Sketch needs root privileges for OverlayFS operations. Run with `sudo`:

```bash
sudo sketch
```

### "Failed to create mount namespace"

Your kernel may not support mount namespaces, or you lack `CAP_SYS_ADMIN`.

Check kernel support:
```bash
grep CONFIG_NAMESPACES /boot/config-$(uname -r)
```

Ensure you're running as root (not just a user with some capabilities).

### "Failed to mount overlay"

Possible causes:

1. **Kernel lacks OverlayFS support**:
   ```bash
   grep overlay /proc/filesystems
   ```
   If missing, load the module:
   ```bash
   sudo modprobe overlay
   ```

2. **Incompatible filesystem on /tmp**: OverlayFS requires the upper and work directories to be on the same filesystem, and it must support `d_type`. Most modern filesystems (ext4, xfs with `ftype=1`, btrfs) support this.

   Check:
   ```bash
   df -T /tmp
   ```

3. **Disk full**: The overlay needs space in `/tmp` for the upper directory.
   ```bash
   df -h /tmp
   ```

### "Failed to pivot_root"

This usually means the merged directory is not a valid mount point. Ensure the overlay mount succeeded (use `--verbose` to check).

### Session Exits Immediately

If sketch starts and immediately returns to your normal shell:

1. Check that your `$SHELL` is set correctly:
   ```bash
   echo $SHELL
   ```

2. Try specifying a shell explicitly:
   ```bash
   sudo sketch exec /bin/bash
   ```

3. Use `--verbose` to see where the failure occurs.

### Orphaned Mounts or Temp Directories

If sketch was killed forcefully (`kill -9`) or the system crashed during a session, temporary directories may remain:

```bash
# Clean up orphaned sessions
sudo sketch clean

# Manual cleanup if needed
ls /tmp/sketch-*
sudo umount /tmp/sketch-*/merged 2>/dev/null
sudo rm -rf /tmp/sketch-*
```

### Programs Can't Find Libraries

Within a sketch session, the host's libraries are visible through the overlay. If a newly installed program can't find its libraries:

```bash
ldconfig
```

This refreshes the dynamic linker cache within the session.

### DNS Resolution Fails

The session uses the host's `/etc/resolv.conf` (read-only from the lower layer). If DNS fails, check your host's DNS configuration first.

### Package Manager Complains About Locks

If a package manager on the host is running concurrently, lock files may conflict. Wait for the host operation to complete, or remove the lock inside the session (it only affects the overlay):

```bash
rm /var/lib/dpkg/lock-frontend
rm /var/lib/apt/lists/lock
```

## Getting Debug Information

Run with verbose output to see exactly what sketch is doing:

```bash
sudo sketch --verbose
```

This prints each step: namespace creation, overlay mount, virtual filesystem mounts, pivot root, and shell spawn.

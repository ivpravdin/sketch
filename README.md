# Sketch 📝

**Ephemeral, disposable machine sessions using overlay filesystems.**

Sketch lets you test changes, run commands, and experiment in a **completely isolated environment** where everything is discarded when you exit — the host system always remains unchanged.

```bash
# Try a dangerous command safely
sketch exec rm -rf /var/log/*
# ✓ No effect on your actual system!

# Test package installation
sketch shell
(sketch) $ apt install experimental-package
(sketch) $ # Try it out...
(sketch) $ exit
# ✓ Package not installed on host!

# Make config changes, selectively keep some
sketch shell
(sketch) $ vim /etc/app.conf
(sketch) $ sketch commit /etc/app.conf
(sketch) $ exit
# ✓ Only /etc/app.conf persisted to host!
```

## Features

- ✅ **Complete Filesystem Isolation** — All changes exist in temporary overlays
- ✅ **Selective Persistence** — Commit specific files back to the host
- ✅ **Resumable Sessions** — Reconnect to disconnected sessions
- ✅ **Minimal Overhead** — Fast startup, efficient overlay mounting
- ✅ **Per-Mount Isolation** — Every filesystem gets its own overlay
- ✅ **Safe Experimentation** — Perfect for testing packages, configs, scripts

## Quick Start

### Installation

```bash
cargo build --release
sudo install target/release/sketch /usr/local/bin/
```

### Basic Usage

```bash
# Interactive session (just like bash)
sketch

# Run a command
sketch exec apt update
sketch exec pip install package

# Non-interactive (for scripts/CI)
sketch run -- ./test-script.sh

# List active sessions
sketch list

# Cleanup orphaned sessions
sketch --clean
```

## Documentation

- **[User Guide](docs/usage.md)** — How to use Sketch
- **[Architecture](docs/architecture.md)** — How it works internally
- **[Contributing](docs/contributing.md)** — Development guide

## How It Works

Sketch uses Linux **OverlayFS** and **namespaces** to create an isolated filesystem view:

1. **Overlay Mounts** — Each filesystem (root, /home, /var, etc.) gets an overlay layer
2. **Mount Namespace** — Changes don't propagate to the host
3. **UTS Namespace** — Used for hostname change
3. **Pivot Root** — Process sees only the overlayed filesystem
4. **Cleanup** — All overlays unmounted when session ends

For details, see [docs/architecture.md](docs/architecture.md).

## Use Cases

### Development & Testing

```bash
sketch shell
(sketch) # make changes, compile, test
(sketch) $ ./build.sh
(sketch) $ ./run-tests.sh
(sketch) $ sketch commit Makefile  # Keep just the Makefile changes
(sketch) $ exit
```

### Temporary files

```bash
# Test configuration changes
sketch shell
(sketch) $ vim /home/user/test.c
(sketch) $ gcc test.c && ./a.out
(sketch) $ exit 
```

### Package Testing

```bash
# Try a package without installing it
sketch shell
(sketch) $ apt install experimental-package
(sketch) $ experimental-package
(sketch) $ exit
# experimental-package is not installed on your host
# Does not pollute your system
```


## Commit Command Quick Reference

The `sketch commit` command lets you **selectively persist files** while keeping everything else isolated:

```bash
sketch shell
(sketch) $ # Make changes, install packages, edit configs
(sketch) $ vim /etc/app.conf
(sketch) $ npm install package  # Only in session
(sketch) $ sketch commit /etc/app.conf  # Keep only this file
(sketch) $ exit
# Result: /etc/app.conf persisted, npm package not installed
```

**Common usage:**
```bash
sketch commit /etc/config.conf                    # Single file
sketch commit file1 file2 file3                  # Multiple files
sketch commit /etc/nginx/*.conf                  # Glob patterns
```

## Requirements

- **Linux** with OverlayFS support (Linux 3.18+, most modern kernels)
- **Root access** OR user namespace support (Linux 5.11+)
- Sufficient space in `/tmp`

Check compatibility:

```bash
sketch status
```

## Examples

### Safe Destructive Testing

```bash
sketch exec rm -rf /etc/*
# Completely safe! Host /etc unchanged.
```

### Configuration Testing

```bash
sketch shell
(sketch) $ cp /etc/hosts /etc/hosts.test
(sketch) $ vim /etc/hosts.test
(sketch) $ cat /etc/hosts.test | your-app
(sketch) $ exit
```

### Experiment With Your Shell

```bash
sketch shell
(sketch) $ vim ~/.bashrc
(sketch) $ # Test bashrc changes...
(sketch) $ sketch commit ~/.bashrc  # Keep the changes
(sketch) $ exit
```

### CI/CD Integration

```bash
# In .github/workflows/test.yml
- name: Run tests
  run: |
    sketch run --timeout 600 --name "unit-tests" -- \
      bash -c "npm install && npm test"
```

## Performance

- **Startup:** ~1 second
- **File access:** Near-native speeds (OverlayFS optimized)
- **Memory:** Minimal overhead beyond process requirements
- **Disk:** Proportional to modified data (not used disk space)

## Security

**Isolated:**
- Filesystem modifications
- Created/deleted files
- Directory structure

**Not Isolated (shared with host):**
- Network
- Processes (can see host processes)
- IPC
- Devices

Sketch is **safe for testing**, not for running **untrusted code**.

## Status

**v0.1.0** — Pre-release with core features:
- ✅ Overlay filesystem isolation
- ✅ Mount namespace isolation
- ✅ User namespace support (non-root)
- ✅ File commitment feature
- ✅ Session resumption
- ✅ Package manager detection
- ✅ DNS resolution
- 🚧 Additional features planned

## Contributing

Contributions welcome! See [contributing.md](docs/contributing.md) for development setup and guidelines.

## License

MIT License

## FAQ

**Q: Is Sketch a container?**
A: No. It's much simpler — just overlay filesystems and mount namespaces. Use Docker/Podman for full container isolation with cgroups and more.

**Q: Why not use a container?**
A: Sketch provides more (and less) than a container. A Sketch session represents a copy of the host’s session state rather than a fully isolated environment, preserving things like the user’s shell context, environment variables, and working directory so it feels like a seamless continuation instead of a fresh sandbox. At the same time, it offers less isolation than a container—it doesn’t virtualize the entire filesystem, network stack, or enforce strict resource limits. Unlike containers, Sketch also doesn’t require pulling or managing images, which makes it faster to start and simpler to use. In short, containers prioritize isolation and reproducibility, while Sketch prioritizes continuity, low overhead, and zero image setup.

**Q: Can I run untrusted code in Sketch?**
A: At your own risk. Sketch does not guarantee full isolation.

**Q: Does Sketch work without root?**
A: Not yet, but this feature is planned.

**Q: What about disk space?**
A: Sketch uses `/tmp`. Only modified data counts toward disk usage. Run `sketch --clean` to remove orphaned sessions.

**Q: Can I commit files with different ownership?**
A: Files are committed with your user's ownership. Root can commit any ownership.

**Q: How do I inspect a disconnected session's files?**
A: This feature is in-progress

**Q: Performance impact?**
A: Minimal. OverlayFS adds negligible overhead for reads, writes go to the upper directory. Startup is ~1 second.

## See Also

- [OverlayFS Documentation](https://www.kernel.org/doc/html/latest/filesystems/overlayfs.html)
- [Linux Namespaces](http://man7.org/linux/man-pages/man7/namespaces.7.html)

---

**Questions?** Check the documentation or file an issue on GitHub.

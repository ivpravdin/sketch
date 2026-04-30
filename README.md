# Sketch 📝

**Ephemeral, disposable machine sessions using overlay filesystems.**

Sketch lets you test changes, run commands, and experiment in a **completely isolated environment** where everything is discarded when you exit — the host system always remains unchanged.

```bash
# Try a dangerous command safely
sketch run rm -rf /var/log/*
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

## Demo

<p align="center"><a href="https://asciinema.org/a/mjFbMWs08LFKKj15"><img src="https://asciinema.org/a/mjFbMWs08LFKKj15.svg" width="572px" height="412px"/></a></p>

## Usage & Examples

### Interactive Shell

Start an interactive session and make changes, optionally committing specific files back to the host:

```bash
sudo sketch shell
(sketch) $ vim /etc/app.conf
(sketch) $ apt install experimental-package
(sketch) $ # Test everything out...
(sketch) $ sketch commit /etc/app.conf
(sketch) $ exit
# Result: Only /etc/app.conf persisted; package not installed on host
```

### Non-Interactive (Scripts & CI)

Run a command or script in an isolated session:

```bash
sudo sketch run -- ./test-script.sh
sudo sketch run --timeout 60 -- npm test
sudo sketch run --name "my-task" -- bash -c "apt update && apt install pkg"
```

### Other Useful Commands

```bash
sketch list              # Show active sessions
sketch clean           # Cleanup orphaned overlay mounts
```

## Documentation

- **[User Guide](docs/usage.md)** — How to use Sketch
- **[Architecture](docs/architecture.md)** — How it works internally
- **[Installation](docs/installation.md)** — System requirements and setup
- **[Safety](docs/safety.md)** — Isolation guarantees and limitations
- **[Using LLMs](docs/llm_usage.md)** — Tips for running LLMs inside sessions
- **[Troubleshooting](docs/troubleshooting.md)** — Common errors and fixes
- **[Contributing](docs/contributing.md)** — Development guide

## How It Works

Sketch uses Linux **OverlayFS** and **namespaces** to create an isolated filesystem view:

1. **Overlay Mounts** — Each filesystem (root, /home, /var, etc.) gets an overlay layer
2. **Mount Namespace** — Changes don't propagate to the host
3. **UTS Namespace** — Used for hostname change
3. **Pivot Root** — Process sees only the overlayed filesystem
4. **Cleanup** — All overlays unmounted when session ends

For details, see [docs/architecture.md](docs/architecture.md).

## Requirements

- **Linux** with OverlayFS support (Linux 3.18+, most modern kernels)
- **Root access** OR user namespace support (Linux 5.11+)
- Sufficient space in `/tmp`

Check compatibility:

```bash
sketch status
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

## What's Next

See [docs/todo.md](docs/todo.md) for planned features and improvements.

## Contributing

Contributions welcome! See [contributing.md](docs/contributing.md) for development setup and guidelines. We have a wishlist of features available in [todo.md](docs/todo.md).

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
A: Sketch uses `/tmp`. Only modified data counts toward disk usage. Run `sketch clean` to remove orphaned sessions.

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

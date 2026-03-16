# Usage Guide

## Privileges

All sketch commands must be run with sudo because OverlayFS mounting, mount namespace isolation, and pivot_root require CAP_SYS_ADMIN:

```bash
sudo sketch                    # Interactive shell
sudo sketch exec ./script.sh   # Run a command
sudo sketch --clean            # Clean orphaned sessions
```

The `--help` and `--version` flags work without sudo. The `--clean` command also works without root (though it may not be able to unmount all orphaned overlays).

## Basic Usage

### Start an Interactive Session

```bash
sudo sketch
```

This drops you into a shell where the entire filesystem is writable but ephemeral. Your prompt changes to `(sketch)` to indicate you're in a session.

Type `exit` or press `Ctrl-D` to end the session. All changes are discarded.

### Run a Single Command

```bash
sudo sketch exec <command> [args...]
```

Runs a command inside an ephemeral environment and exits when it completes.

Examples:

```bash
# Install and test a package without affecting your system
sudo sketch exec apt install -y nginx

# Run a build in a clean environment
sudo sketch exec make -C /path/to/project

# Test a destructive script safely
sudo sketch exec bash risky-script.sh
```

### Clean Up Orphaned Sessions

If sketch was interrupted (e.g., power loss, `kill -9`), temporary directories may remain in `/tmp`:

```bash
sudo sketch --clean
```

## Command Reference

```
sketch [OPTIONS] [COMMAND]

OPTIONS:
    -h, --help       Show help message
    -v, --version    Show version
    --verbose        Enable verbose output (shows mount operations)
    --clean          Clean up orphaned overlay mounts

COMMANDS:
    shell            Start interactive shell session (default)
    exec <command>   Execute a single command in an ephemeral session
```

## Common Workflows

### Try a Package Before Installing It

```bash
sudo sketch
(sketch) apt install -y some-package
(sketch) some-package --help
# Looks good? Exit and install for real
(sketch) exit
sudo apt install -y some-package
```

### Test Configuration Changes

```bash
sudo sketch
(sketch) vim /etc/nginx/nginx.conf
(sketch) nginx -t
(sketch) systemctl restart nginx
# If it breaks, just exit — nothing changed
(sketch) exit
```

### Debug a Build in a Clean Environment

```bash
sudo sketch exec bash -c "cd /home/user/project && make clean && make"
```

### Experiment with System Files

```bash
sudo sketch
(sketch) rm -rf /usr/lib/*  # this is fine
(sketch) exit               # system is untouched
```

## Package Management

Package managers work normally inside sketch sessions. All installations, removals, and cache updates happen in the ephemeral overlay and are discarded on exit.

Sketch auto-detects your system's package manager and supports:

| Distribution | Package Manager |
|---|---|
| Debian / Ubuntu | apt |
| Fedora | dnf |
| RHEL / CentOS | yum |
| Arch Linux | pacman |
| openSUSE | zypper |
| Alpine | apk |

Example workflows:

```bash
# Test a package before committing to install it
sudo sketch
(sketch) apt update && apt install -y nginx
(sketch) nginx -v
(sketch) exit  # nginx gone, host unchanged

# Try a different version of a library
sudo sketch
(sketch) apt install -y libssl-dev=3.0.0-1
(sketch) make -C /home/user/project
(sketch) exit
```

Package caches are stored in the overlay (e.g., `/var/cache/apt/archives`), so they don't consume space on the host.

## Environment Variables

Inside a sketch session, the following environment variables are set:

- `SKETCH_SESSION=1` — indicates you're inside a sketch session
- `SKETCH_ORIGINAL_UID` — the UID of the user who invoked sketch
- `SKETCH_ORIGINAL_GID` — the GID of the user who invoked sketch

Host environment variables like `HOME`, `USER`, `SHELL`, `TERM`, `PATH`, `LANG`, `EDITOR`, and `VISUAL` are preserved.

You can use these in scripts:

```bash
if [ "$SKETCH_SESSION" = "1" ]; then
    echo "Running inside sketch (original user UID: $SKETCH_ORIGINAL_UID)"
fi
```

## Verbose Mode

Use `--verbose` to see what sketch is doing internally:

```bash
sudo sketch --verbose
```

Output:
```
sketch: session dir: /tmp/sketch-a1b2c3d4-...
sketch: creating mount namespace...
sketch: mounting overlay filesystem...
sketch: mounting virtual filesystems...
sketch: pivoting root...
sketch: spawning: /bin/bash
```

This is useful for debugging mount issues.

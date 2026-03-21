# Sketch User Guide

Sketch creates **ephemeral, disposable machine sessions** where you can test changes safely. All modifications exist only in a temporary overlay and are discarded when you exit — the host system remains completely unchanged.

## Quick Start

### Interactive Shell Session
Start a disposable shell where any changes you make will be discarded:

```bash
sketch shell
# or simply:
sketch
```

### Execute a Command
Run a command in an isolated session and get the output:

```bash
sketch exec apt update
sketch exec pip install some-package
sketch exec rm -rf /etc/important-config  # won't affect host
```

### Non-Interactive Run
Run commands for scripting/CI with optional timeout and environment variables:

```bash
sketch run -- bash script.sh
sketch run --timeout 30 -- long-running-process
sketch run -e ENV_VAR=value -- command
```

## Features

### 1. Complete Filesystem Isolation

All filesystems are overlayed — changes don't persist to the host:

```bash
(sketch) # Create a file
$ echo "test" > /home/user/test.txt

(sketch) # Modify system files
$ echo "modified" > /etc/config

(sketch) # Install packages
$ apt install new-package

(sketch) # Exit the session
$ exit

# Back on host — NOTHING has changed
$ ls /home/user/test.txt  # File doesn't exist
$ cat /etc/config        # Original content unchanged
$ apt list --installed | grep new-package  # Package not installed
```

### 2. Commit Files Back (Selective Persistence)

Need to keep some changes? Use `sketch commit` inside the session:

```bash
(sketch) $ curl https://important-file > /etc/app.conf
(sketch) $ sketch commit /etc/app.conf
sketch: marked for commit: /etc/app.conf

(sketch) $ exit
sketch: committed 1 file(s)

# Back on host
$ cat /etc/app.conf  # File is now persisted!
```

### 3. Resume Disconnected Sessions

If a session dies but you need to recover files:

```bash
# Check orphaned sessions
$ sketch list
SESSION ID                            STATUS   AGE      COMMAND
590b91cc-694a-48e5-8cc9-a9be16803b9d stale    15m      shell

# Reattach to inspect/commit files
$ sketch attach 590b91cc-694a-48e5-8cc9-a9be16803b9d
(sketch) $ ls  # See files from previous session
(sketch) $ sketch commit /etc/recovered-file
(sketch) $ exit

# Clean up
$ sketch --clean
sketch: cleaned up 1 orphaned session(s)
```

## Common Use Cases

### Testing Package Installations

```bash
sketch shell
# Try installing without affecting your system
(sketch) $ apt install experimental-package
(sketch) $ some-command
# If it works, exit and install normally on host
(sketch) $ exit
```

### Destructive Operations

```bash
sketch exec rm -rf /var/log/*
# Logged nothing and affected nothing on the host!
```

### Configuration Testing

```bash
sketch shell
(sketch) $ cp /etc/nginx/nginx.conf /etc/nginx/nginx.conf.backup
(sketch) $ vim /etc/nginx/nginx.conf
(sketch) $ nginx -t  # Test syntax
(sketch) $ exit

# If test passed, manually apply the change to host
```

### Development & Experimentation

```bash
sketch shell
# Make a bunch of changes, compile, test
(sketch) $ vim Makefile
(sketch) $ make
(sketch) $ make test
(sketch) $ sketch commit Makefile  # Keep the Makefile changes
(sketch) $ exit
# Clean changes are discarded, Makefile is persisted
```

### CI/CD Pipelines

```bash
sketch run --timeout 600 -- ./build-and-test.sh
```

## Command Reference

### sketch [OPTIONS] [COMMAND]

**OPTIONS:**
- `-h, --help` — Show help message
- `-v, --version` — Show version
- `--verbose` — Enable verbose output
- `--clean` — Clean up orphaned sessions

**COMMANDS:**

#### shell
Start an interactive shell in an ephemeral session.

```bash
sketch shell
```

#### exec <COMMAND>
Execute a command in an ephemeral session.

```bash
sketch exec apt update
sketch exec ls /tmp
sketch exec bash -c "echo test > /etc/file"
```

#### run [OPTIONS] -- COMMAND
Run a command non-interactively (for scripting/CI).

```bash
# Basic run
sketch run -- ./test.sh

# With timeout (kills after 30 seconds)
sketch run --timeout 30 -- ./long-task.sh

# With environment variables
sketch run -e DEBUG=1 -e LOG_LEVEL=trace -- ./app

# With name (for listing/debugging)
sketch run --name "build-job" -- make
```

**Run Options:**
- `--name NAME` — Label the session
- `--timeout SECONDS` — Kill after timeout
- `-e, --env KEY=VALUE` — Set environment variable (repeatable)

#### commit [FILE...]
Mark files to be persisted to the host when the session ends.

```bash
sketch commit /etc/config
sketch commit /home/user/.bashrc /etc/app.conf
```

Only works inside an active session. Files are persisted when you exit.

#### attach <SESSION_ID>
Resume a disconnected session.

```bash
sketch attach 590b91cc-694a-48e5-8cc9-a9be16803b9d
```

#### list [--json]
Show active and stale sessions.

```bash
sketch list
sketch list --json
```

#### status
Show system diagnostics (kernel, overlayfs, disk space, etc.).

```bash
sketch status
```

## Environment Variables

Inside a sketch session:

- `SKETCH_SESSION` — Set to "1" (you're inside a session)
- `SKETCH_SESSION_DIR` — Path to session directory (for commit support)
- `SKETCH_ORIGINAL_UID` — Original user ID (before entering session)
- `SKETCH_ORIGINAL_GID` — Original group ID
- Other standard vars (`HOME`, `USER`, `SHELL`, `PATH`, etc.) are preserved

Example — detect if running in a session:

```bash
if [ "$SKETCH_SESSION" = "1" ]; then
  echo "Running in a sketch session!"
fi
```

## Requirements

- Linux kernel with OverlayFS support (most modern kernels)
- Root privileges (or user namespaces enabled on kernel 5.11+)
- Sufficient disk space in `/tmp`

Check system readiness:

```bash
sketch status
```

## Troubleshooting

### "must be run as root"

You need root privileges or user namespace support:

```bash
sudo sketch shell
```

Or check if user namespaces are enabled:

```bash
sketch status
```

### No space left on device

Too much data written to `/tmp`:

```bash
sketch --clean  # Remove orphaned sessions
df -h /tmp
```

### Files not persisting with commit

- Are you calling `sketch commit` inside the session? ✓
- Did you provide the full path? ✓
- Did the file actually exist in the overlay? ✓

```bash
(sketch) $ sketch commit /etc/file  # Full path required
```

### Permission denied when committing

The file may require elevated privileges to write. The session runs as your user, so it can only persist files that your user can write:

```bash
(sketch) $ sudo sketch commit /etc/sudoers  # Won't work - needs root
```

## Tips & Tricks

### Backup Before Testing
```bash
sketch shell
(sketch) $ cp /etc/important /etc/important.backup
(sketch) $ # make changes
(sketch) $ sketch commit /etc/important  # keep original + backup
```

### Test Shell Scripts
```bash
sketch exec bash -s < /path/to/script.sh
```

### Generate Test Data
```bash
sketch shell
(sketch) $ ./generate-large-dataset.sh  # Doesn't use real disk
(sketch) $ exit  # All test data discarded
```

### Diff Before Committing
```bash
sketch shell
(sketch) $ cp /etc/config /etc/config.new
(sketch) $ vim /etc/config
(sketch) $ diff -u /etc/config.new /etc/config  # Preview changes
(sketch) $ sketch commit /etc/config
```

## Performance Notes

- First session startup: ~1 second (creating overlays)
- File access overhead: minimal, overlayfs is very efficient
- Memory: minimal overhead beyond what processes use
- Disk: uses `/tmp` space proportional to changed data

Cleanup happens automatically on exit, freeing all temporary data.

## Security Notes

- **Isolation is filesystem-level only** — processes can still access host network, IPC, devices (depending on kernel and options)
- **Not a container** — use Docker/podman for more comprehensive isolation
- **Root session privilege** — entering as root gives full filesystem access
- **Files in upper directory** — changes are stored unencrypted in `/tmp/sketch-*/upper`

Use `sketch` for safe experimentation, not for running untrusted code.

## See Also

- `sketch status` — Check system compatibility
- `sketch --help` — Command reference
- Developer guide — For contributing to sketch

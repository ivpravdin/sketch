# Sketch Commit Command Guide

## Overview

The `sketch commit` command allows you to **selectively persist changes** from an ephemeral session to the host filesystem. This is the key feature that makes Sketch practical for real work — you get complete isolation by default, but can choose to keep specific changes.

**Basic concept:**
- By default: All changes in a session are **discarded** when you exit
- With `sketch commit`: Specific files are **persisted** to the host before cleanup

## Quick Start

```bash
# Start an isolated session
sketch shell

# Make changes
(sketch) $ echo "new config" > /etc/myapp.conf
(sketch) $ apt install -y useful-package
(sketch) $ vim ~/.bashrc

# Mark files to keep
(sketch) $ sketch commit /etc/myapp.conf ~/.bashrc

# Exit session
(sketch) $ exit

# Result: Only /etc/myapp.conf and ~/.bashrc are on the host
# Everything else is discarded (useful-package is NOT installed)
```

## How It Works

### The Commit Process

1. **During Session:** `sketch commit <files>` marks file paths in `.sketch-commit`
2. **On Exit:** Session reads the list and copies marked files to host
3. **Before Cleanup:** Files are persisted before overlay unmounting
4. **Isolation Preserved:** Only marked files persist; other changes stay isolated

### What Gets Committed

Files are committed from the session's **merged view** (the overlay):
- **Modified files:** Changed existing files
- **New files:** Files created in the session
- **Directory trees:** Parent directories are created as needed
- **Preserved metadata:** File permissions, ownership (if you have permission to set them)

### What Doesn't Get Committed

- **Installed packages:** Apt/pip installs only modify session (not committed)
- **Removed files:** Deleted files can't be committed
- **Created directories (without files):** Empty directories aren't committed
- **Process state:** Running services, open connections, etc.

## Command Syntax

```bash
sketch commit <file> [<file> ...]
```

**Arguments:**
- `<file>` — Path to file to commit (can use multiple)
  - Absolute paths: `/etc/config`, `/home/user/.bashrc`
  - Relative paths: `config.txt`, `./src/main.rs`
  - Glob patterns: **NOT supported** (use multiple arguments instead)

**Constraints:**
- Must be run **inside** an active sketch session
- Set by environment: `SKETCH_SESSION=1`, `SKETCH_SESSION_DIR=<path>`
- Files must exist in the session (can't commit deleted files)

## Usage Examples

### Example 1: Edit Configuration Files

```bash
$ sketch shell
(sketch) $ vim /etc/nginx/nginx.conf
(sketch) $ vim /etc/nginx/conf.d/ssl.conf
(sketch) $ nginx -t   # Test the config

# If tests pass, commit both config files
(sketch) $ sketch commit /etc/nginx/nginx.conf /etc/nginx/conf.d/ssl.conf
(sketch) $ exit

# Both files now updated on the host, everything else unchanged
```

### Example 2: Update Shell Configuration

```bash
$ sketch shell
(sketch) $ vim ~/.bashrc
(sketch) $ vim ~/.bash_profile
(sketch) $ source ~/.bashrc     # Test it
(sketch) $ echo $MYVAR          # Verify changes work

# Only commit if happy with changes
(sketch) $ sketch commit ~/.bashrc ~/.bash_profile
(sketch) $ exit
```

### Example 3: Create New Configuration File

```bash
$ sketch shell
(sketch) $ cat > /etc/app/settings.conf << 'EOF'
DEBUG=true
TIMEOUT=30
EOF

(sketch) $ /opt/app --config /etc/app/settings.conf  # Test it
(sketch) $ sketch commit /etc/app/settings.conf
(sketch) $ exit

# /etc/app/settings.conf now exists on host (with your content)
```

### Example 4: Merge Multiple Test Scenarios

```bash
# Test multiple configurations, commit the best one
$ sketch shell
(sketch) $ vim /etc/myapp.conf
(sketch) $ ./test-with-config.sh
(sketch) $ # Bad results, try again

(sketch) $ vim /etc/myapp.conf      # Edit again
(sketch) $ ./test-with-config.sh
(sketch) $ # Better! Commit this version

(sketch) $ sketch commit /etc/myapp.conf
(sketch) $ exit
```

### Example 5: Batch Create Config Files

```bash
$ sketch shell
(sketch) $ mkdir -p /etc/myapp/configs
(sketch) $ echo "setting1=value1" > /etc/myapp/configs/app.conf
(sketch) $ echo "cache_size=1000" > /etc/myapp/configs/cache.conf
(sketch) $ echo "max_workers=4" > /etc/myapp/configs/workers.conf

(sketch) $ # Test application with new configs
(sketch) $ /opt/myapp --config-dir /etc/myapp/configs

(sketch) $ # Commit all config files
(sketch) $ sketch commit \
    /etc/myapp/configs/app.conf \
    /etc/myapp/configs/cache.conf \
    /etc/myapp/configs/workers.conf

(sketch) $ exit
# All three config files now on host
```

## Common Patterns

### Pattern 1: Edit-Test-Commit Loop

```bash
sketch shell
(sketch) $ while true; do
  vim /etc/config.conf        # Make changes
  ./validate-config            # Test changes
  if [ $? -eq 0 ]; then
    sketch commit /etc/config.conf
    break
  fi
done
(sketch) $ exit
```

### Pattern 2: Commit on Success

```bash
sketch shell
(sketch) $ if ./run-all-tests.sh; then
  echo "Tests passed! Committing..."
  sketch commit test-results.json
else
  echo "Tests failed. Not committing results."
fi
(sketch) $ exit
```

### Pattern 3: Safe Exploratory Work

```bash
# Explore changes safely, keep only the good ones
sketch shell
(sketch) $ # Make many changes
(sketch) $ # Run tests/validations
(sketch) $ # Review what changed
(sketch) $ # Selectively commit the best changes

(sketch) $ sketch commit \
    /path/to/good-change-1 \
    /path/to/good-change-2

(sketch) $ exit
```

### Pattern 4: Scripted Commit

```bash
#!/bin/bash
sketch shell << 'EOF'
  # Auto-configure system
  cp /etc/template.conf /etc/app.conf
  sed -i 's/HOSTNAME/myhost/' /etc/app.conf
  ./validate-config

  # Commit if validation passed
  if [ $? -eq 0 ]; then
    sketch commit /etc/app.conf
    echo "Config committed"
  else
    echo "Validation failed, config not committed"
  fi
EOF
```

## Paths: Absolute vs Relative

### Absolute Paths (Recommended)

```bash
(sketch) $ sketch commit /etc/config.conf
# Clear which files are committed, works from any directory
```

**Advantages:**
- Unambiguous — no confusion about paths
- Works from any directory
- Matches the actual file location

### Relative Paths

```bash
(sketch) $ cd /home/user
(sketch) $ sketch commit .bashrc
# Same as: sketch commit /home/user/.bashrc
```

**Advantages:**
- Shorter to type when already in directory
- Can use `.` for current directory

**Note:** Relative paths are resolved from current working directory within session.

## Multiple Files

Commit multiple files in one command:

```bash
# Multiple arguments
(sketch) $ sketch commit file1 file2 file3

# Many files (use shell expansion where possible)
(sketch) $ sketch commit /etc/*.conf

# Or commit separately
(sketch) $ sketch commit /etc/config1.conf
(sketch) $ sketch commit /etc/config2.conf
# Both are added to commit list
```

## Checking Commits

You can see which files have been marked for commit:

```bash
# Inside session, view the commit list
(sketch) $ cat $SKETCH_SESSION_DIR/.sketch-commit
/etc/myconfig.conf
/home/user/.bashrc

# Or check if a session dir exists (outside session)
$ ls -la /tmp/sketch-*/
$ cat /tmp/sketch-<uuid>/.sketch-commit
```

## Edge Cases & Gotchas

### Issue: "File not found"

```bash
(sketch) $ sketch commit /nonexistent/file
# Error: file not found: /nonexistent/file
```

**Solution:** File must exist in the session:
```bash
(sketch) $ touch /nonexistent/file
(sketch) $ sketch commit /nonexistent/file  # Now works
```

### Issue: Committing Before Creating File

```bash
(sketch) $ sketch commit /etc/newfile.conf
# Error: file not found

# Create file first:
(sketch) $ echo "config" > /etc/newfile.conf
(sketch) $ sketch commit /etc/newfile.conf  # Now works
```

### Issue: Empty Directories Not Committed

```bash
(sketch) $ mkdir -p /etc/myapp/config
(sketch) $ sketch commit /etc/myapp/config
# Directory created, but no files in it

# Solution: Commit files, not directories:
(sketch) $ echo "settings" > /etc/myapp/config/app.conf
(sketch) $ sketch commit /etc/myapp/config/app.conf
# Parent directories are created automatically
```

### Issue: Permissions Not Preserved

```bash
(sketch) $ chmod 600 /etc/secret.conf
(sketch) $ sketch commit /etc/secret.conf
# File is committed with your user's default permissions

# You may need to:
# 1. Run sketch as root: sudo sketch
# 2. Or manually fix permissions after committing
```

### Issue: Deleted Files Can't Be Uncommitted

```bash
(sketch) $ sketch commit /etc/config.conf
(sketch) $ rm /etc/config.conf
(sketch) $ exit
# /etc/config.conf still persisted to host!
```

**Note:** Deleting a file after marking it doesn't prevent commitment. Mark carefully.

### Issue: Same File Committed Multiple Times

```bash
(sketch) $ sketch commit /etc/config.conf
(sketch) $ sketch commit /etc/config.conf  # Again
# File appears twice in .sketch-commit list

# This is fine — system handles duplicates
```

## Troubleshooting

### Q: How do I know which files were committed?

**During session:**
```bash
(sketch) $ cat $SKETCH_SESSION_DIR/.sketch-commit
```

**After session (if error occurs):**
```bash
# Session dir still exists with commit list
$ ls /tmp/sketch-*/
$ cat /tmp/sketch-<uuid>/.sketch-commit
$ sketch attach <uuid>  # Reconnect to see state
```

### Q: Can I commit files I didn't modify?

Yes! You can commit any file that exists in the session:

```bash
(sketch) $ sketch commit /etc/passwd  # Even if unchanged
# File is copied to host
```

### Q: What if commit fails?

Failures print to stderr:

```bash
(sketch) $ sketch commit /root/.bashrc
# Error: failed to write file: Permission denied

# Causes:
# - No write permission to target location
# - Parent directory doesn't exist
# - Disk full
```

**Solution:** Check permissions and disk space, then try again.

### Q: Can I commit to directories I can't write?

No, you can only commit to directories you have write permission for:

```bash
(sketch) $ sketch commit /root/.bashrc
# Error: Permission denied

# If running as root:
(sketch) $ sudo sketch
(inside as root) $ sketch commit /root/.bashrc  # OK
```

## Performance Notes

- **Commit is fast:** ~10ms per file for typical files
- **Large files:** Copy time depends on file size
- **Many files:** Committing 100+ files is fine (sequential)
- **Disk I/O:** One copy per file from overlay to host

## Security Considerations

### Files Are Copied As-Is

```bash
(sketch) $ echo "database_password=secret123" > /etc/db.conf
(sketch) $ sketch commit /etc/db.conf
# Password is now persisted in plain text on host!
```

**Be careful with:**
- Passwords, API keys, credentials
- Personal data
- Sensitive configuration

### Permissions Are Your Responsibility

```bash
(sketch) $ echo "secret" > /etc/secret.conf
(sketch) $ chmod 600 /etc/secret.conf  # Make it readable only by you
(sketch) $ sketch commit /etc/secret.conf
```

Commit runs with your permissions, so:
- Only commit files you can read
- Ensure target permissions are correct
- Don't commit files with world-readable secrets

## Advanced Usage

### Commit in Scripts/CI

```bash
#!/bin/bash
sketch run --name "build-test" -- bash << 'INNER'
  ./build.sh
  if [ $? -eq 0 ]; then
    # Build succeeded, persist artifact
    sketch commit /tmp/build/output.tar.gz
  fi
INNER
```

### Conditional Commit

```bash
(sketch) $ ./tests.sh && sketch commit ./test-results.json || echo "Tests failed"
```

### Commit to Temporary Location

Create a temp file in session, test it, then commit:

```bash
(sketch) $ # Create test version
(sketch) $ cp /etc/original /tmp/test-config

(sketch) $ # Test it thoroughly
(sketch) $ /app --config /tmp/test-config

(sketch) $ # If good, persist to real location
(sketch) $ sketch commit /tmp/test-config
(sketch) $ # Handle moving/renaming outside session as needed
```

## When NOT to Use Commit

Some scenarios don't benefit from `sketch commit`:

1. **Testing only:** If you just want to test, don't commit anything
2. **Package installation:** Committing `apt install` requires special handling
3. **Large file changes:** If you modify many files, consider committing the whole directory
4. **System state:** Committing individual files may leave system in inconsistent state

**Better approaches:**
- Create a complete configuration file and commit that
- Use version control to manage changes
- Snapshot the whole session if major changes

## Integration with Version Control

Commit works well with git/version control:

```bash
sketch shell
(sketch) $ cd /home/user/myproject
(sketch) $ # Make changes to code
(sketch) $ git add .
(sketch) $ git commit -m "Changes"
(sketch) $ sketch commit /home/user/myproject/.git
(sketch) $ exit

# Changes are persisted (git repo shows new commits on host)
```

## Summary

The `sketch commit` command gives you the best of both worlds:

| Feature | Without Commit | With Commit |
|---------|---------|---------|
| **Safety** | ✅ Complete isolation | ✅ Complete isolation |
| **Flexibility** | ❌ All-or-nothing | ✅ Selective persistence |
| **Use Cases** | Testing, exploration | Development, configuration |
| **Complexity** | Simple | Minimal (one command) |

**Key takeaway:** Use `sketch commit` to selectively keep changes while maintaining the safety of complete isolation.


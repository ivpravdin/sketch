# Sketch Commit Cheat Sheet

Quick reference for the `sketch commit` command.

## The Basics

```bash
# Start session
sketch shell

# Make changes (everything is isolated)
(sketch) $ vim /etc/config.conf
(sketch) $ ./install-package.sh
(sketch) $ mkdir -p /home/user/project

# Mark files to keep
(sketch) $ sketch commit /etc/config.conf /home/user/project/file.txt

# Exit (marked files persisted, everything else discarded)
(sketch) $ exit
```

## Common Commands

| Task | Command |
|------|---------|
| Single file | `sketch commit /etc/config.conf` |
| Multiple files | `sketch commit file1 file2 file3` |
| All in directory | `sketch commit /etc/app/*` |
| With wildcards | `sketch commit /home/user/.config/*.conf` |
| View commit list | `cat /.sketch-commit` |

**Note:** The `.sketch-commit` file is created at `/.sketch-commit` inside the session,
which writes to the overlay upper directory.

## Before You Commit

```bash
# ✅ Make sure file exists
(sketch) $ test -f /etc/myfile.conf && echo "exists"

# ✅ Test your changes worked
(sketch) $ /app --config /etc/myfile.conf

# ✅ Check what you're about to commit
(sketch) $ cat /etc/myfile.conf

# ✅ Make sure you have permission to write
(sketch) $ touch /test-write.tmp && rm /test-write.tmp
```

## Common Mistakes & Fixes

| Problem | Fix |
|---------|-----|
| "file not found" | Create file first: `touch /path/file` |
| Can't commit empty dir | Create a file in it first: `echo "" > dir/file` |
| Wrong file committed | Delete after exit and recommit next session |
| Permission denied | Run session as root: `sudo sketch shell` |
| Too many files | Use absolute paths or glob: `sketch commit /etc/*.conf` |

## Examples in 30 Seconds

### Config File Edit
```bash
sketch shell
(sketch) $ vim /etc/nginx/nginx.conf
(sketch) $ nginx -t           # Verify
(sketch) $ sketch commit /etc/nginx/nginx.conf
(sketch) $ exit
```

### Shell Config Update
```bash
sketch shell
(sketch) $ vim ~/.bashrc      # Add your customizations
(sketch) $ source ~/.bashrc   # Test
(sketch) $ sketch commit ~/.bashrc
(sketch) $ exit
```

### Create New Config
```bash
sketch shell
(sketch) $ cat > /etc/app.conf << 'EOF'
DEBUG=true
TIMEOUT=30
EOF
(sketch) $ /app --config /etc/app.conf  # Test it
(sketch) $ sketch commit /etc/app.conf
(sketch) $ exit
```

### Multiple Edits
```bash
sketch shell
(sketch) $ vim /etc/file1.conf
(sketch) $ vim /etc/file2.conf
(sketch) $ vim /etc/file3.conf
(sketch) $ # Test all...
(sketch) $ sketch commit /etc/file1.conf /etc/file2.conf /etc/file3.conf
(sketch) $ exit
```

## What Gets Committed

| Type | Result |
|------|--------|
| Modified files | ✅ Saved to host |
| New files | ✅ Created on host |
| Deleted files | ❌ Can't be committed |
| Empty directories | ❌ Not committed |
| Parent directories | ✅ Auto-created |
| Ownership | 🤔 Depends on permissions |
| Permissions | 🤔 Your user's defaults |

## What Gets Discarded

| Type | Result |
|------|--------|
| Packages (`apt install`) | ❌ Not in session only |
| Services (running) | ❌ Stopped on exit |
| Process changes | ❌ Lost |
| Env variables | ❌ Lost |
| Created files (uncommitted) | ❌ Lost |

## Path Format

```bash
# Absolute (recommended)
sketch commit /etc/config.conf          # Always works

# Relative (from current dir)
cd /etc && sketch commit config.conf    # Same as above

# With variables
sketch commit $HOME/.bashrc             # Variable expanded
sketch commit ~/project/file.txt        # Tilde expanded
```

## Inside vs Outside Session

```bash
# INSIDE session (these work)
(sketch) $ sketch commit /etc/config.conf  ✅ Works
(sketch) $ echo $SKETCH_SESSION             ✅ "1"
(sketch) $ cat $SKETCH_SESSION_DIR/.sketch-commit  ✅ Works

# OUTSIDE session (these DON'T work)
$ sketch commit /etc/config.conf        ❌ Not in session!
$ echo $SKETCH_SESSION                  ❌ Empty
```

## Troubleshooting

| Error | Cause | Solution |
|-------|-------|----------|
| "Not in session" | Ran outside session | Run inside `sketch shell` |
| "File not found" | File doesn't exist | Create it first |
| "Permission denied" | Can't write to path | Check directory permissions |
| "Not found" on exit | File didn't exist at exit | Commit before deleting |

## Pro Tips

💡 **Use glob patterns for multiple files:**
```bash
sketch commit /etc/*.conf /home/user/.config/*.yaml
```

💡 **Check what will be committed:**
```bash
cat $SKETCH_SESSION_DIR/.sketch-commit
```

💡 **Commit frequently:**
```bash
(sketch) $ sketch commit /etc/step1.conf
(sketch) $ # More changes...
(sketch) $ sketch commit /etc/step2.conf
(sketch) $ # Multiple commits = flexible
```

💡 **Use in scripts:**
```bash
sketch shell << 'EOF'
  ./configure
  ./build
  sketch commit config.h
  ./test
  sketch commit test-results.json
EOF
```

💡 **Test before committing:**
```bash
(sketch) $ vim /etc/config.conf
(sketch) $ app --config /etc/config.conf  # Verify
(sketch) $ sketch commit /etc/config.conf # Only if test passed
```

## Related Commands

```bash
sketch shell              # Start interactive session (any changes isolated)
sketch exec <cmd>         # Run command in session
sketch commit <files>     # Mark files to persist
sketch list               # Show active sessions
sketch attach <id>        # Reconnect to session
sketch --clean            # Remove orphaned sessions
```

## Learn More

- **Full guide:** See [COMMIT_GUIDE.md](COMMIT_GUIDE.md)
- **General usage:** See [README.md](README.md)
- **All commands:** See [USER_GUIDE.md](USER_GUIDE.md)

---

**Remember:** Everything in a session is discarded by default. Use `sketch commit` to keep what you want.

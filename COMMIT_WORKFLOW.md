# Sketch Commit Workflow Diagrams

Visual guides for understanding how the commit feature works.

## Basic Workflow

```
┌─────────────────────────────────────────────────────────────────┐
│ Session Starts                                                  │
│ Everything isolated in overlay filesystem                       │
└──────────────────────┬──────────────────────────────────────────┘
                       │
                       ▼
        ┌──────────────────────────────┐
        │ Changes Made                 │
        │ - Edit files                 │
        │ - Install packages           │
        │ - Create directories         │
        │ - Modify configurations      │
        └──────────────────────────────┘
                       │
        ┌──────────────┴──────────────┐
        │                             │
        ▼                             ▼
   ┌──────────────┐          ┌──────────────┐
   │   Discard    │          │   Commit     │
   │   (No mark)  │          │   (marked)   │
   └──────┬───────┘          └──────┬───────┘
          │                        │
          ▼                        ▼
    ┌──────────────┐        ┌──────────────┐
    │ Lost on exit │        │ Persisted    │
    │              │        │ to host      │
    └──────────────┘        └──────────────┘
```

## File States During Session

```
┌─────────────────────────────────────────────────────────────────┐
│                      Overlay Filesystem                         │
│                                                                 │
│  /etc/config.conf (MODIFIED)                                    │
│  /home/user/.bashrc (MODIFIED)                                  │
│  /etc/newfile.conf (CREATED)                                    │
│  /opt/app/data (MODIFIED)                                       │
│                                                                 │
│  ✓ sketch commit /etc/config.conf                               │
│  ✗ /home/user/.bashrc (not marked)                              │
│  ✓ sketch commit /etc/newfile.conf                              │
│  ✗ /opt/app/data (not marked)                                   │
└─────────────────────────────────────────────────────────────────┘
                            │
                    On Session Exit
                            │
        ┌───────────────────┼───────────────────┐
        │                   │                   │
        ▼                   ▼                   ▼
   ┌─────────────┐  ┌──────────────┐  ┌─────────────┐
   │ Committed   │  │  Discarded   │  │ Committed   │
   │             │  │              │  │             │
   │ config.conf │  │ .bashrc      │  │ newfile.conf│
   └─────────────┘  │              │  └─────────────┘
                    │ /opt/app/data│
                    └──────────────┘
```

## Commit List Processing

```
Step 1: Inside session, create commit list
┌────────────────────────────────┐
│ Child process writes           │
│ to: /.sketch-commit            │
│                                │
│ Content:                       │
│ /etc/config.conf               │
│ /etc/newfile.conf              │
│ /home/user/.bashrc             │
│                                │
│ Goes to overlay:               │
│ /tmp/sketch-<uuid>/upper/      │
│   └── .sketch-commit           │
└────────────────────────────────┘

Step 2: On exit, parent reads and processes
┌────────────────────────────────┐
│ Parent reads:                  │
│ overlay.upper_dir/.sketch-commit│
│                                │
│ For each line:                 │
│ 1. Find in overlay upper       │
│ 2. Copy to host filesystem     │
│ 3. Mark as committed           │
└────────────────────────────────┘

Step 3: Result on host
┌────────────────────────────────┐
│ Host Filesystem                │
│ ────────────────────────────── │
│ /etc/config.conf      (updated)│
│ /etc/newfile.conf     (created)│
│ /home/user/.bashrc    (updated)│
│ /opt/app/data         (unchanged)
└────────────────────────────────┘
```

## Use Case: Configuration Testing

```
Configuration Testing Workflow
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

START: sketch shell
│
├─ Edit configuration file
│  │ vim /etc/nginx/nginx.conf
│  └─ FILE STATE: modified in overlay
│
├─ Test configuration
│  │ nginx -t
│  └─ TEST RESULT: ✓ Valid
│
├─ Deploy configuration (optional)
│  │ sketch commit /etc/nginx/nginx.conf
│  └─ MARKED: file will persist
│
└─ Exit session
   │ exit
   │
   ├─ IF COMMITTED: /etc/nginx/nginx.conf updated on host ✓
   └─ IF NOT: changes discarded ✓
```

## Example: Multi-Step Deployment

```
START: sketch shell
│
STEP 1: Create configuration
│  ├─ cat > /etc/app.conf << 'EOF'
│  │   SETTING=value
│  │ EOF
│  └─ sketch commit /etc/app.conf
│     (persists config)
│
STEP 2: Create data directory
│  ├─ mkdir -p /opt/app/data
│  ├─ echo "sample" > /opt/app/data/init.txt
│  └─ sketch commit /opt/app/data/init.txt
│     (creates directory on host + file)
│
STEP 3: Create startup script
│  ├─ cat > /usr/local/bin/myapp << 'EOF'
│  │   #!/bin/bash
│  │   /opt/app/start.sh
│  │ EOF
│  ├─ chmod +x /usr/local/bin/myapp
│  └─ sketch commit /usr/local/bin/myapp
│     (persists script + makes executable)
│
STEP 4: Test everything
│  ├─ /usr/local/bin/myapp --version
│  └─ # Verify all components work
│
EXIT: exit
└─ Result: All three components persisted to host ✓
```

## Data Flow: Commit to Host

```
Session Interior
┌────────────────────────────┐
│ Overlay Merged View        │
│                            │
│ /etc/config.conf (content: │
│  "KEY=VALUE")              │
│                            │
│ Modified in session        │
└────────────────────────────┘
           │
           │ sketch commit /etc/config.conf
           │
           ▼
┌────────────────────────────┐
│ .sketch-commit List        │
│ ────────────────────────── │
│ /etc/config.conf           │
└────────────────────────────┘
           │
           │ (on exit)
           │
           ▼
┌────────────────────────────┐
│ Session Exit Handler       │
│                            │
│ 1. Read .sketch-commit     │
│ 2. For each file:          │
│    - Copy from session     │
│    - Write to host         │
│ 3. Unmount overlays        │
│ 4. Cleanup temp dirs       │
└────────────────────────────┘
           │
           ▼
┌────────────────────────────┐
│ Host Filesystem            │
│                            │
│ /etc/config.conf (content: │
│  "KEY=VALUE")              │
│                            │
│ Persisted! ✓               │
└────────────────────────────┘
```

## Comparison: With vs Without Commit

```
WITHOUT COMMIT (Default)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Session Start
    │
    ├─ /etc/app.conf (host: v1)
    │
    ▼
Changes in overlay
    │
    ├─ /etc/app.conf (session: v2)  ◄─ Modified
    │
    └─ (NOT marked)

Exit
    │
    └─ Host: /etc/app.conf = v1  ✓ Unchanged
       Session overlay: DISCARDED


WITH COMMIT
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Session Start
    │
    ├─ /etc/app.conf (host: v1)
    │
    ▼
Changes in overlay
    │
    ├─ /etc/app.conf (session: v2)  ◄─ Modified
    │
    └─ sketch commit /etc/app.conf  ◄─ MARKED

Exit
    │
    ├─ Persist marked files
    │
    └─ Host: /etc/app.conf = v2  ✓ Updated
       Session overlay: DISCARDED
```

## Decision Tree: Should I Commit?

```
Made a change in sketch session?
│
├─ YES, I want to keep it
│  │
│  └─► Run: sketch commit <file>
│      │
│      └─► Exit session
│          │
│          └─► Change is on host ✓
│
└─ NO, I don't want it
   │
   └─► Just exit session
       │
       └─► Change is discarded ✓
```

## Error Handling Flow

```
User runs: sketch commit /etc/config.conf
│
├─ Check if in session
│  │
│  ├─ YES: Continue
│  │
│  └─ NO: Error "not in session" ✗
│
├─ Check if file exists in overlay
│  │
│  ├─ YES: Add to .sketch-commit
│  │       │
│  │       └─► Print: "marked for commit"
│  │
│  └─ NO: Error "file not found" ✗
│
└─ On exit: commit files
   │
   ├─ Can write to host
   │  │
   │  └─► File persisted ✓
   │
   └─ Can't write (permission/disk)
      │
      └─► Error printed, not committed ✗
```

## Session States

```
SESSION LIFECYCLE
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

[Created]
   │
   │ Session created at /tmp/sketch-<uuid>/
   │ ├─ merged/   (what user sees)
   │ ├─ upper/    (writable layer)
   │ ├─ work/     (overlay working dir)
   │ └─ .sketch-commit (metadata - NOT in merged/)
   │
   ▼
[Running]  ◄──────────────┐
   │                      │
   ├─ User makes changes  │
   │  └─ Everything in    │
   │     overlay/merged   │
   │                      │
   ├─ User commits files  │
   │  └─ Marked in        │
   │     .sketch-commit   │
   │     (session root)   │
   │                      │
   └─ More changes OK   ──┘

   │
   │ User exits (exit, Ctrl+D, etc.)
   │
   ▼
[Exiting]
   │
   ├─ Read .sketch-commit from session root
   ├─ Copy marked files to host
   ├─ Unmount overlays
   ├─ Clean /tmp/sketch-<uuid>/
   │
   ▼
[Cleaned]
   │
   └─ Session dir deleted, changes persisted
```

**Important:** The `.sketch-commit` file is stored in `/tmp/sketch-<uuid>/`
(the session root), NOT in `/tmp/sketch-<uuid>/merged/`.
It's metadata about what to persist, not part of the overlay view.

## Key Takeaway

```
┌────────────────────────────────────┐
│ Sketch = Safe by Default           │
│                                    │
│ No commit? Changes discarded ✓     │
│                                    │
│ With commit? Changes saved ✓       │
│                                    │
│ You decide what to keep!           │
└────────────────────────────────────┘
```

## See Also

- **Full guide:** [COMMIT_GUIDE.md](COMMIT_GUIDE.md)
- **Quick reference:** [COMMIT_CHEATSHEET.md](COMMIT_CHEATSHEET.md)
- **General docs:** [README.md](README.md)

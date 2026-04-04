# Using LLMs Inside Sketch Sessions

Sketch is a perfect sandbox for running LLM tools and agents. Since everything in a sketch
session is ephemeral and isolated, LLMs can write files, install packages, run scripts, and
experiment freely without any risk to your host system.

**Quick benefit**: If an LLM makes a mistake or goes off-rails, just `exit` — nothing persists
to your host unless you explicitly commit it. This enables a new workflow: tell an LLM to be
bold, try things, break things — because the damage is contained.

## Getting Started

### Launching an LLM Tool

Start a sketch session with your LLM tool:

```bash
sudo sketch shell
(sketch) $ claude        # or: code, cursor, copilot, etc.
```

Or run a specific task non-interactively:

```bash
sudo sketch run -- llm-tool --prompt "write a fibonacci function"
```

### Passing API Keys

API keys are inherited from your outer shell environment:

```bash
# Keys from the host shell are visible inside the session
export ANTHROPIC_API_KEY=sk-...
sudo sketch shell
(sketch) $ claude  # Can access ANTHROPIC_API_KEY
```

To pass keys explicitly without setting them on the host:

```bash
sudo sketch shell -e ANTHROPIC_API_KEY=sk-... -e OTHER_KEY=value
```

### Naming Sessions

Label sessions for easy identification when running multiple in parallel:

```bash
sudo sketch --name llm-codegen shell
sudo sketch --name llm-refactor shell

# List them
sketch list
# Output:
#   llm-codegen (pid: 1234)
#   llm-refactor (pid: 5678)
```

## Use Cases

### Safe Code Generation

Let an LLM write and test code. If the generated code breaks things, discard the session.
Only commit files you want to keep:

```bash
sudo sketch shell
(sketch) $ claude
# [Claude writes Python code, installs dependencies, runs tests...]
(sketch) $ # Review the changes
(sketch) $ sketch commit solution.py
(sketch) $ exit
# Result: Only solution.py persisted to the host
```

### Package Exploration

Have an LLM install and experiment with unfamiliar packages without polluting your host:

```bash
sudo sketch shell
(sketch) $ apt install experimental-ml-library
(sketch) $ python3
# [Try the library in a Python REPL...]
(sketch) $ exit
# Result: The package is NOT installed on your host
```

### Config File Iteration

Let an LLM tweak configuration, test it, then review the changes:

```bash
sudo sketch shell
(sketch) $ claude
# [Claude edits /etc/nginx/nginx.conf and validates syntax...]
(sketch) $ nginx -t  # Validate
(sketch) $ sketch commit /etc/nginx/nginx.conf
(sketch) $ exit
```

### Destructive Refactors

For large-scale file changes (renaming functions, moving code between modules), let an LLM
handle it in isolation:

```bash
sudo sketch shell
(sketch) $ claude
# [Claude refactors your codebase...]
(sketch) $ # Review the diffs inside the session before committing
(sketch) $ git diff | less
(sketch) $ sketch commit src/  # Commit the whole refactored folder
(sketch) $ exit
```

### GUI-Based LLM Agents

If you use an LLM agent that needs a browser or X11 GUI apps (e.g., for screenshot-based
tasks or GUI automation):

```bash
sudo sketch --x11 shell
(sketch) $ agent-tool --use-browser
# [Agent opens browser, takes screenshots, interacts with UI...]
```

## Committing LLM Work

The `sketch commit` command lets you selectively persist files back to the host. This is
how LLM-generated changes become real.

### Commit Patterns

Single file:

```bash
sketch commit /etc/nginx/nginx.conf
```

Multiple files:

```bash
sketch commit src/main.rs src/lib.rs Cargo.toml
```

Glob patterns:

```bash
sketch commit /etc/nginx/*.conf
```

Directories:

```bash
sketch commit src/  # Commits all files in src/ recursively
```

### Best Practice: Ask the LLM to Commit

Instead of you managing commit commands, instruct the LLM explicitly:

```
"When you're done, run `sketch commit` on only the files you intentionally changed."
```

This keeps the changeset auditable and minimal — the LLM commits only what it knows it
modified, not everything.

## Long-Running Sessions & Timeouts

### Set a Time Limit

Prevent runaway LLM processes from consuming resources indefinitely:

```bash
# Run for at most 10 minutes (600 seconds)
sudo sketch run --timeout 600 -- llm-agent --task "refactor the entire codebase"
```

The session terminates automatically when the timeout is reached.

### Reconnecting to Disconnected Sessions

The `sketch attach` command (currently in development) will allow you to reconnect to a
sketch session if your SSH connection drops or your terminal closes:

```bash
sudo sketch attach <session-id>
```

This is particularly useful for long-running LLM agent tasks. Check the project's status
for when this feature is released.

## Running Multiple Sessions

Run several LLM tasks in parallel, each in its own isolated session:

```bash
# Terminal 1
sudo sketch --name llm-task-1 shell

# Terminal 2
sudo sketch --name llm-task-2 shell

# Check what's running
sketch list --json
```

Each session has its own overlay filesystem — changes in one don't affect the other. This
is great for A/B testing different LLM approaches or running multiple agents concurrently.

## Tips & Tricks

### Review Before Committing

Always inspect what the LLM changed before running `sketch commit`:

```bash
(sketch) $ git diff | less        # See all changes
(sketch) $ diff -u original.txt new.txt  # Compare specific files
(sketch) $ ls -la src/            # Review file state
```

### Set Realistic Timeouts

Use `--timeout` to match the expected task duration. Too short and the LLM gets killed mid-work;
too long and you waste resources on bugs:

```bash
# Quick tasks: 30–60 seconds
sudo sketch run --timeout 60 -- claude "write a unit test"

# Medium tasks: 5–10 minutes
sudo sketch run --timeout 600 -- claude "refactor this module"

# Long tasks: 30+ minutes
sudo sketch run --timeout 1800 -- llm-agent "investigate and fix the performance issue"
```

### Keep API Keys Out of Scripts

Don't hardcode API keys in scripts or config files that live in the sketch session. Instead,
pass them via environment variables from the host:

```bash
# Bad: Keys end up in the session
sudo sketch shell
(sketch) $ echo "ANTHROPIC_API_KEY=sk-..." >> ~/.bashrc

# Good: Keys stay on the host
sudo sketch shell -e ANTHROPIC_API_KEY=sk-...
(sketch) $ # ANTHROPIC_API_KEY is available but not persisted
```

### Start Clean

Every new `sketch shell` or `sketch run` is a clean slate. There's no leftover state from
previous sessions. This is useful if the LLM got into a bad state — just exit and try again.

### Use X11 for GUI Agents

If your LLM agent needs a display (browser, GUI app):

```bash
sudo sketch --x11 shell
(sketch) $ DISPLAY=:0 firefox &  # GUI app is visible
(sketch) $ agent-tool --use-browser
```

## Security Caveats & Limitations

### Filesystem Isolation ≠ Privacy

**Important:** Just because the LLM can't write to the host filesystem (by default) doesn't
mean it can't read your data.

- The LLM inside the session can read **all the same files** as it would on the host: home
  directories, SSH keys, config files, credentials in environment variables, git history, etc.
- Sketch does **not restrict which files the agent can access or exfiltrate**
- Sketch does **not prevent prompt injection**: If a file on your filesystem contains
  malicious content (e.g., a specially crafted JSON config or log file), the LLM could
  read it and have its behavior manipulated.

### Other Shared Resources

- **Network** — The LLM can make outbound requests and download files (including exfiltrating
  data to external servers)
- **Processes** — The LLM can see all host processes and potentially interact with them
- **Devices** — Block devices, character devices, and other resources are shared with the host

### Bottom Line

**Sketch protects the host filesystem from _accidental writes_, but it does not protect
_your data_ from the agent.**

If you don't trust the LLM tool, its dependencies, or files on your filesystem, sketch is
**not sufficient protection**. For untrusted workloads, use a full container (Docker, Podman)
or virtual machine instead.

## Next Steps

- Read the [User Guide](usage.md) for general sketch usage
- Check [Architecture](architecture.md) to understand how sketch isolation works
- See [Troubleshooting](troubleshooting.md) if something goes wrong

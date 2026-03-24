#!/usr/bin/env bash
set -euo pipefail

echo "[+] Starting overlay root experiment"

# Re-exec inside namespace
if [[ "${IN_NS:-}" != "1" ]]; then
    echo "[+] Entering user namespace (root mapped)..."
    SCRIPT_PATH="$(readlink -f "$0")"

    exec unshare --user --map-root-user --mount --pid --fork \
        --mount-proc env IN_NS=1 "$SCRIPT_PATH"
fi

echo "[+] Inside namespace as UID=$(id -u)"

WORKDIR=$(mktemp -d)
echo "[+] Using workdir: $WORKDIR"

LOWER="/"
UPPER="$WORKDIR/upper"
WORK="$WORKDIR/work"
MERGED="$WORKDIR/merged"

mkdir -p "$UPPER" "$WORK" "$MERGED"

echo "[+] Mounting overlay with / as lowerdir..."

mount -t overlay overlay \
    -o lowerdir="$LOWER",upperdir="$UPPER",workdir="$WORK",userxattr \
    "$MERGED"

echo "[+] Overlay mounted at $MERGED"

# Make essential mounts inside new root
mkdir -p "$MERGED/proc" "$MERGED/dev" "$MERGED/sys"

mount -t proc proc "$MERGED/proc"
mount --rbind /dev "$MERGED/dev"
mount --rbind /sys "$MERGED/sys"

echo "[+] Entering chroot environment"
echo "[+] This is a FAKE writable root (changes go to upperdir)"

cd "$MERGED"
exec chroot "$MERGED" /bin/bash
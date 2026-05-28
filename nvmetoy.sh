#!/usr/bin/env bash
# Runs on swissknife. Assumes the device's resting state: the NVMe at $BDF is bound to the
# kernel nvme driver, with its XFS NOT mounted. Other users of the device take precedence,
# so this only borrows the device from that state and returns it. It mounts the XFS read-only
# just long enough to resolve the target file's device blocks via FIEMAP, unmounts,
# unbinds the kernel driver, and drops into the repl confined to those blocks. On exit it
# rebinds the kernel driver and leaves the XFS unmounted, restoring the resting state no
# matter how the repl ends. <file> is a path relative to the XFS root.
set -euo pipefail

FILE="${1:?usage: nvmetoy.sh <file> [bdf] [xfs-device]}"
BDF="${2:-0000:01:00.0}"
XFS_DEVICE="${3:-/dev/nvme0n1}"
# The namespace id is the trailing nN of the device name (whole-namespace device, no
# partition), e.g. /dev/nvme0n1 -> 1. The repl issues its I/O against this namespace.
NSID="${XFS_DEVICE##*n}"

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DRIVER_PATH="/sys/bus/pci/drivers/nvme"
DEV_PATH="/sys/bus/pci/devices/$BDF"
MAP=/tmp/nvmetoy.map
MOUNT_DIR=""

cd "$REPO_DIR"
. "$HOME/.cargo/env"

if [ ! -e "$DEV_PATH" ]; then
    echo "ERROR: no PCI device at $BDF" >&2
    exit 1
fi

cleanup() {
    # Restore the resting state: kernel driver bound, XFS unmounted.
    if [ -n "$MOUNT_DIR" ]; then
        mountpoint -q "$MOUNT_DIR" && sudo umount "$MOUNT_DIR" || true
        rmdir "$MOUNT_DIR" 2>/dev/null || true
    fi
    if [ -d "$DRIVER_PATH" ] && [ ! -e "$DRIVER_PATH/$BDF" ]; then
        echo "Rebinding $BDF to kernel nvme driver..."
        echo "$BDF" | sudo tee "$DRIVER_PATH/bind" >/dev/null 2>&1 || true
    fi
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

# A live mount means another user holds the device. They take precedence, so refuse rather
# than disturb it.
if findmnt --source "$XFS_DEVICE" >/dev/null 2>&1; then
    echo "ERROR: $XFS_DEVICE is mounted; another user holds it." >&2
    echo "       nvmetoy expects the resting state (kernel-bound, unmounted). Aborting." >&2
    exit 1
fi
if [ ! -b "$XFS_DEVICE" ]; then
    echo "ERROR: $XFS_DEVICE does not exist; expected $BDF bound to the kernel nvme driver." >&2
    echo "       The resting state is bound + unmounted. Rebind it first:" >&2
    echo "         echo $BDF | sudo tee $DRIVER_PATH/bind" >&2
    exit 1
fi

emit_block_map() {
    local file="$1" lba_size
    lba_size="$(cat "/sys/block/$(basename "$XFS_DEVICE")/queue/logical_block_size")"
    echo "nvmetoy-block-map 1"
    echo "device_lba_size $lba_size"
    sudo filefrag -e -b"$lba_size" "$file" | awk '
        /^[[:space:]]*[0-9]+:/ {
            line = $0
            gsub(/\.\./, " "); gsub(/:/, " ")
            unwritten = (line ~ /unwritten/) ? 1 : 0
            print $4, $6, unwritten
        }'
}

cargo build --release --bin nvmetoy_repl

# Mount the XFS read-only, just long enough to resolve the file to device blocks.
MOUNT_DIR="$(mktemp -d)"
echo
echo "=== mount $XFS_DEVICE read-only at $MOUNT_DIR to read FIEMAP ==="
sudo mount -o ro "$XFS_DEVICE" "$MOUNT_DIR"

TARGET="$MOUNT_DIR/${FILE#/}"
if ! sudo test -f "$TARGET"; then
    echo "ERROR: '$FILE' not found on the XFS (looked for $TARGET)" >&2
    exit 1
fi

echo
echo "=== resolve $FILE to device blocks (filefrag) ==="
emit_block_map "$TARGET" | tee "$MAP"

sudo umount "$MOUNT_DIR"
rmdir "$MOUNT_DIR"
MOUNT_DIR=""

echo
echo "Unbinding $BDF from kernel nvme driver..."
echo "$BDF" | sudo tee "$DRIVER_PATH/unbind" >/dev/null

echo
echo "=== interactive repl over $FILE (namespace $NSID; type 'help', Ctrl-D to quit) ==="
sudo ./target/release/nvmetoy_repl "$BDF" "$MAP" "$NSID"

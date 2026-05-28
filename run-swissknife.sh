#!/usr/bin/env bash
# Runs on the laptop. Syncs the project to swissknife, runs the test suite, and
# delegates to nvmetoy.sh there over an ssh -t session (rustyline needs a TTY).
set -euo pipefail

HOST=swissknife
REMOTE_DIR=nvmetoy
FILE="${1:-nvmetoy-scratch.bin}"
BDF="${2:-0000:01:00.0}"
XFS_DEVICE="${3:-/dev/nvme0n1}"
LOCAL_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

rsync -az --delete --exclude target --exclude .git "$LOCAL_DIR/" "$HOST:~/$REMOTE_DIR/"

ssh -t "$HOST" "source ~/.cargo/env \
    && cd ~/$REMOTE_DIR \
    && echo '=== cargo test ===' && cargo test \
    && ./nvmetoy.sh '$FILE' '$BDF' '$XFS_DEVICE'"

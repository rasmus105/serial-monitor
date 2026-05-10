#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BASE="${SCRIPT_DIR}/common.sh"
source "${BASE}"
cleanup
# Start deterministic ASCII serial data. Keep it running through capture so the
# app does not switch to disconnected/error styling before the screenshot.
start_serial_test ascii --seed 1 --startup-delay-ms 5000 --interval-ms 0 --lines 120 --hold-after-lines

# Start TUI in tmux
start_tui

# Wait for TUI to render its first frame
sleep 1
# Open command palette and connect
"${SCRIPT_DIR}/../tmux/send" ':' "connect ${PTY}" Enter
sleep 1
"${SCRIPT_DIR}/../tmux/send" C-g

# Wait for connection to establish, transient toasts to expire, and the delayed burst to finish.
sleep 4
# Open config panel
"${SCRIPT_DIR}/../tmux/send" c
sleep 0.3
capture_and_freeze traffic-config.png

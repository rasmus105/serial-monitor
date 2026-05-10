#!/usr/bin/env bash
# Screenshot 2: Graph view with ~40 points of sensor data parsed
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BASE="${SCRIPT_DIR}/common.sh"
source "${BASE}"

cleanup
prepare_graph_fonts

# Start sensor test with a delayed burst so the TUI is connected before data arrives.
start_serial_test sensor --seed 1 --startup-delay-ms 5000 --interval-ms 20 --lines 160 --hold-after-lines

# Start TUI
start_tui

sleep 1

# Connect
"${SCRIPT_DIR}/../tmux/send" ':' "connect ${PTY}" Enter
sleep 1
"${SCRIPT_DIR}/../tmux/send" C-g

# Wait for connection to establish, transient toasts to expire, and buffered data to arrive.
sleep 4

# Switch to graph tab and enable parsing
"${SCRIPT_DIR}/../tmux/send" '2'
sleep 0.5
"${SCRIPT_DIR}/../tmux/send" g
sleep 1

# Open config panel to show parser sections
"${SCRIPT_DIR}/../tmux/send" c
sleep 0.3
capture_and_freeze graph-sensor.png

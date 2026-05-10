#!/usr/bin/env bash
# Screenshot 3: File sender in mid-transmission
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BASE="${SCRIPT_DIR}/common.sh"
source "${BASE}"
cleanup

# Create sample data file
SAMPLE_FILE="${TMPDIR:-/tmp}/serial-monitor-sample-data.txt"
cat > "${SAMPLE_FILE}" << 'SAMPLEEOF'
AT
OK
AT+CGMI
Manufacturer
AT+CGMM
Model
AT+CGMR
Revision 2.1.0
AT+CSQ
+CSQ: 28,0
AT+CREG?
+CREG: 0,1
AT+COPS?
+COPS: 0,0,"Test Network"
AT+CGATT?
+CGATT: 1
AT+CGPADDR
+CGPADDR: 1,10.0.0.100
AT+CMGS="+1234567890"
> Hello from serial device
+CMGS: 42
AT+CNMI=2,1,0,0,0
OK
AT+CMGL="ALL"
+CMGL: 1,"REC UNREAD","+1234567890","","25/05/26,12:30:00+00"
Test message content here
AT+CPIN?
+CPIN: READY
AT+CFUN=1
OK
AT+CMEE=2
OK
AT+CMGF=1
OK
AT+CGDCONT=1,"IP","internet"
OK
AT+CGPADDR=1
+CGPADDR: 1,10.0.0.100
AT+COPS=0
OK
AT+CSQ
+CSQ: 30,99
AT+CREG?
+CREG: 0,5
AT+CGACT=1,1
OK
AT+CGDATA="PPP",1
CONNECT
SAMPLEEOF

for i in $(seq 1 400); do
    printf 'AT+PING=%03d\n+PING: %03d,OK\n' "${i}" "${i}" >> "${SAMPLE_FILE}"
done

# Start echo serial test
start_serial_test echo --seed 1

# Start TUI
start_tui

# Wait for TUI to render its first frame
sleep 1

# Connect
"${SCRIPT_DIR}/../tmux/send" ':' "connect ${PTY}" Enter
sleep 2
"${SCRIPT_DIR}/../tmux/send" C-g
sleep 1

# Switch to file sender
"${SCRIPT_DIR}/../tmux/send" '3'
sleep 0.5

# Open the config panel and activate the file path input.
"${SCRIPT_DIR}/../tmux/send" c C-l Enter
sleep 0.5

# Type the sample file path and confirm
"${SCRIPT_DIR}/../tmux/send" "${SAMPLE_FILE}"
sleep 0.2
"${SCRIPT_DIR}/../tmux/send" Enter
sleep 0.5

# Move focus back to main
"${SCRIPT_DIR}/../tmux/send" C-h
sleep 0.3

# Start sending
"${SCRIPT_DIR}/../tmux/send" Enter

# Capture after the start toast expires while the longer fixture is still sending.
sleep 3.2
CAPTURE_DELAY=0
capture_and_freeze file-sender.png
rm -f "${SAMPLE_FILE}"

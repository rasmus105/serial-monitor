# Serial Monitor

A cross-platform "serial monitor" TUI for receiving and transmitting
serial data through serial ports, supporting standard data encodings, as well
as parsing data (as UTF-8) into graph points and visualizing them.

## Download

```bash
brew ...

```

## Previews

TODO: Add screenshots here

## Features

- Multiple encodings: UTF-8, Ascii, Hex, Binary (data is stored once as raw bytes, and converted on-demand to desired encoding)
- Searching: Search for any literal string or regex pattern.
- Filtering: Input a literal string or regex pattern for which lines to show in raw traffic view.
- File saving: Save the received (and/or transmitted) data to a file in your desired encoding (UTF-8 by default)
- Bounded buffer: Data will is stored in a ringbuffer-like manner to avoid hogging up memory on long captures (configurable; can be disabled)
- Graph: Parse the received data (using UTF-8 encoding) as points to plot on a graph.
- File sending: You can select files of data to send, either in chunks or continuously (this can for example be used to simulate GPS chip output)
- Multiple sessions: Open as many serial port connections at once as you would like (not really sure about the use case for this, could also just open another instance of the application)
- Automatic file saving: By default (can be turned off) the received data for
  the last 10 sessions is automatically stored on-disk (at
  `$HOME/.cache/serial-monitor/` on Linux, `???` on Windows, or `$HOME/.cache/serial-monitor` on
  MacOS) to avoid data loss, in case of crashes (should not crash, but can't guarantee I don't make mistakes) or unintentionally closing the application.
- GUI and TUI front end: All core features and functionality is available in a GUI and TUI application (GUI strives to be user friendly, TUI focuses more on being intuitive for vim-users)
- Decent Performance: I've generally tried keeping a solid performance, but I've made some deliberate decisions that sacrifice memory usage for features (for example, data received is stored once as a "single source of truth", and once in the selected encoding, to allow for switching without data loss). 

### Building from Source

Install [Rust](https://doc.rust-lang.org/book/ch01-01-installation.html) and then build the application like so:
```bash
# Build TUI
cargo build --release --package serial-tui 
```

## Why Another Serial Monitor

There's 1 critical missing feature in all serial monitors I've tried out. That
is the ability to, in real-time, filter lines based on some pattern. A common 
scenario I've run into in the past, is having to debug an embedded device that would
output 10+ lines of logs each second. For example:
```txt
[BLUETOOTH] bluetooth initialized (ret=0)
[FLASH] write ok (addr=0x0012A000, bytes=256)
[IMU] captured imu data (time=1767815935, ax=-0.03g, ay=0.01g, az=0.99g, gx=0.2dps, gy=-0.1dps, gz=0.0dps)
[MODEM] received frame 'AT+CSQ' (len=41)
[APP] state transition (BOOT -> IDLE)
[IMU] fifo watermark reached (samples=32)
[BLUETOOTH] connected (peer=E4:7A:2C:11:9F:02, handle=0x0001)
[UART] baudrate changed (from=9600, to=115200)
[MODEM] parsed response '+CSQ: 17,99' (rssi=-79dBm)
[GPS] no fix (mode=0, sats=2)
[GPS] fix acquired (mode=3D, sats=10, hdop=0.9, lat=56.2383, lon=12.3471)
```
If you're receiving this much data each second, it is practically impossible to keep 
track of what is going on with the device, and therefore debugging specific modules
becomes extremely hard.

With the filtering implemented in this serial monitor, you simple press 'f', write some pattern like "[GPS]", and you will immediately only see lines containing the pattern "[GPS]" (note you can change search and filter patterns to be regex instead of simple string matching)

> See [rs-serial-monitor](github.com/rasmus105/rs-serial-monitor) for GUI serial monitor that does mostly the same, though incomplete.

## Similar Applications

- [Arduino Serial Monitor](https://docs.arduino.cc/software/ide-v2/tutorials/ide-v2-serial-monitor/)
- [CoolTerm](https://freeware.the-meiers.org/)
- [Docklight](https://docklight.de)
- [HTerm](https://www.der-hammer.info/pages/terminal.html)
- [PuTTY](https://www.putty.org)
- [rs-serial-monitor](https://github.com/rasmus105/rs-serial-monitor) (my first attempt at writing a serial monitor)
- [serial-monitor-rust](https://github.com/hacknus/serial-monitor-rust)
- [Serial Studio](https://github.com/Serial-Studio/Serial-Studio)
- [SerialTest](https://github.com/wh201906/SerialTest)

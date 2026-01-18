# Serial Monitor

A cross-platform "serial monitor" application for receiving and transmitting
serial data through serial ports, supporting standard data encodings, as well
as parsing data (as UTF-8) into graph points and visualizing them.

The serial monitor exists with a GUI and TUI front end. Both contain the same
core functionality, but with some minor differences. The GUI is the most user
friendly, while the TUI allow full control from the keyboard, with vim-like bindings.

> [!WARNING]
> Work in progress, must be manually built and the main branch isn't guaranteed
> to be compilable. 

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
  `$HOME/.cache/serial-monitor/` on Linux, `...` on Windows, or `...` on
  MacOS) to avoid data loss, in case of crashes (should not crash, but can't guarantee I don't make mistakes) or unintentionally closing the application.
- GUI and TUI front end: All core features and functionality is available in a GUI and TUI application (GUI strives to be user friendly, TUI focuses more on being intuitive for vim-users)
- Decent Performance: I've generally tried keeping a solid performance, but I've made some deliberate decisions that sacrifice memory usage for features (for example, data received is stored once as a "single source of truth", and once in the selected encoding, to allow for switching without data loss). However, it should be noted that, since it is written in Rust with Ratatui (for TUI) and Iced (for GUI), at least compared to, say, an electron app, the performance is quite good.

## Previews

TODO: Add screenshots here

## Installation (GUI front end)

Installing the GUI (recommended, most user friendly):
```bash
yay -S ... # Arch Linux (AUR)
brew install ... # Linux, MacOS, and Windows (Homebrew)
cargo ... # Linux, MacOS, and Windows
```

Installing the TUI:

TODO NixOS
TODO scoop? windows?
TODO AppImage (github releases)

### Building from Source

Install [Rust](https://doc.rust-lang.org/book/ch01-01-installation.html) and then build the application like so:
```bash
# Build GUI
cargo build --release --package serial-gui 

# Build TUI
cargo build --release --package serial-tui 
```

## Why Another Serial Monitor

There's 1 critical missing feature for debugging embedded devices that rely on
UART for logging, that I haven't been able to find in any other serial monitor,
which ultimately led me to writing this. That feature is real-time filtering.

Lets take an example. If you have an MCU running a RTOS, you could imagine
receiving these logs from the device, within 1 second:
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

I've seen this being solved by changing the enabled logging modules at compile time, 
but this has led to 2 new issues:
- Some extremely rare bug happens while saving UART output, but logs from the
  specific module where this bug occurs is disabled (have experienced this 1
  time, which is enough times)
- I have to recompile each time I want to see logs from a different module.
This serial monitor solves this problem, by allowing you to write patterns in real time,
immediately changing what data is shown. In the previous example, if you were debugging the GPS,
you could (in the TUI), press 'f', and write "\[GPS\]", and 'Enter'. Now you'll only see data chunks
containing "[GPS]".

Lastly, I also haven't been able to find a serial monitor that doesn't have
one of the issues below:
- Doesn't include graph visualisation.
- Freemium; some features are behind a paywall.
- Missing reliability features (bounded buffer missing or not configurable, no saving to file, etc.)
- Unable to display data in different encodings (UTF-8, Hex, Binary)
- No search or filtering

However, I would also like to mention, there are several serial monitor
applications that do some stuff better than this application. Please see [this
section](#similar-applications) for other serial monitors, that might suit your
use case better.

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

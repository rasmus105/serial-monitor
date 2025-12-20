# Serial Monitor

A serial monitor application written in Rust with a focus on clean architecture and separation of concerns.

## Features

- **Traffic View:** Send/receive data with multiple encoding options (UTF-8, ASCII, Hex, Binary)
- **Graph View:** Parse and visualize numeric data over time
- **File Sending:** Send files with configurable chunking and delays
- **Search/Filter:** Find and filter data in the traffic view
- **Vim-like Keybindings:** Navigate efficiently in the TUI

## Architecture

The project is split into two crates:

- **serial-core:** Frontend-agnostic library handling serial I/O, data storage, encoding, and parsing
- **serial-tui:** Terminal UI using ratatui with vim-style navigation

See [AGENTS.md](AGENTS.md) for detailed architecture documentation.

## Building

```bash
cargo build --release
```

## Running

```bash
cargo run -p serial-tui
```

## License

MIT

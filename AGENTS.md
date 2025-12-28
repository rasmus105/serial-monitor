# AGENTS.md - Serial Monitor Architecture Guide

This document outlines the architecture, design decisions, and implementation guidelines for this serial monitor project. It serves as the primary reference for AI agents and developers working on this codebase.

## Project Overview

A serial monitor application, that should contain these crates before v1.0.0:
- **TUI frontend** (using ratatui) with vim-like keybindings
- **GUI frontend** (using iced) with more friendly UI
- **Core library** that is frontend-agnostic

### Key Design Principle

**The previous attempt failed due to coupling.** A high priority is maintaining strict separation between the core library and UI frontends. The core must have ZERO knowledge of any UI framework.
However, it should still offer utilities to make development of frontends easier (such as search, file-saving, etc.)

---

## Architecture

### High-Level Structure

```
serial-monitor/
├── crates/
│   ├── serial-core/          # Frontend-agnostic library
│   └── serial-tui/           # TUI frontend
│   └── serial-gui/           # GUI frontend (not yet created)
├── Cargo.toml                # Workspace root
└── AGENTS.md
```

### Core + Adapter Pattern

The core library exposes a clean API that any frontend can consume. Each frontend owns its event loop and UI state.

```
┌─────────────────────────────────────────┐
│              serial-core                │
│  - Exposes clean, non-blocking API      │
│  - Handles async I/O internally         │
│  - Sends events via channels            │
│  - Has NO UI dependencies               │
└─────────────────────────────────────────┘
              ▲           ▲
              │           │
    ┌─────────┴───┐   ┌───┴─────────┐
    │  TUI App    │   │  GUI App    │
    │  Owns its   │   │  Owns its   │
    │  event loop │   │  event loop │
    └─────────────┘   └─────────────┘
```

Each frontend:
- Owns its event loop
- Manages its own UI state (scroll position, selections, etc.)
- Calls core API methods
- Receives events from core via channels

---

## Data Model

### Session-Based Design

Each session represents one serial port connection. Multi-port support = multiple sessions.

### Raw Bytes as Source of Truth

**CRITICAL:** Store raw bytes in core, convert and cache on-demand. The user selects an encoding (UTF-8, ASCII, Hex, Binary) and the display is converted accordingly.

### Data Chunk Structure

Each chunk of data includes metadata:

```rust
struct DataChunk {
    timestamp: Instant,       // When received/sent
    direction: Direction,     // TX or RX
    data: Vec<u8>,           // Raw bytes
}

enum Direction {
    Tx,  // Sent by user
    Rx,  // Received from device
}
```

This enables:
- Visual differentiation of sent vs received data
- Time-based graph X-axis
- Accurate log exports with timestamps

### Buffer Management

- **Strategy:** Append new data, truncate oldest when buffer limit reached
- **Behavior:** When buffer is full, drop oldest chunks to make room
- **User-configurable:** Buffer size limit (in bytes or chunk count)
- **Interaction with auto-save:** Data is persisted before being dropped, so no data loss occurs

---

## Async Architecture

### Channel-Based Communication

Serial I/O runs on a dedicated async runtime, communicating via channels:

```
┌──────────────────┐         ┌──────────────────┐
│   Serial I/O     │         │    Core API      │
│   (async task)   │         │   (sync calls)   │
│                  │         │                  │
│  ┌────────────┐  │ channel │  ┌────────────┐  │
│  │ Read loop  │──┼────────►│  │  Session   │  │
│  └────────────┘  │  data   │  └────────────┘  │
│                  │         │                  │
│  ┌────────────┐  │ channel │                  │
│  │ Write task │◄─┼─────────│  send(bytes)    │
│  └────────────┘  │ command │                  │
└──────────────────┘         └──────────────────┘
```

**Rules:**
- UI thread NEVER blocks on serial operations
- Serial I/O NEVER waits on rendering

---

## Feature Implementation Guidelines

### 1. Traffic View (Main View)

Displays sent/received data with sent data visually differentiated.

**Search/Filter:**
- Search operates on the DISPLAYED representation (encoded UTF-8 strings, not raw bytes)
    For example: In hex mode, the data will displayed like, e.g.: "DE AD BE EF". Searching "E" should therefore have 3 matches.
- Filter (UTF-8/ASCII): show only lines matching pattern (regex support)

### 2. Graph View

**Lazy initialization:** Graph data parsing only starts when user first enables graph view.

**On-demand then on-the-fly:**
1. User enables graph view
2. Core parses all existing buffered data (may take a moment)
3. From then on, new data is parsed as it arrives

Features:
- Multiple data series on same chart
- Real-time updating
- Historical data navigation
- Generic parser that extracts numbers (e.g., "temperature: 41.3" -> 41.3)
- Custom user scripted parsing (future)

### 3. File Sending

Send entire files with configurable:
- Chunk size (bytes per transmission)
- Delay between chunks
- Continuous mode (loop file forever)

### 4. Logging/Error Handling

**Core sends log events to UI.** The UI decides how to display them.

**Error handling philosophy:**
- Port disconnection: Send error event, set port state to disconnected
- Invalid UTF-8: Replace with replacement character, optionally log
- Parse failures in graph mode: Skip the chunk, optionally log at debug level

### 5. Data Persistence

**Auto-save to temp location:**
- Linux: `/tmp/serial-monitor/` or `~/.local/share/serial-monitor/`
- Write periodically (every N seconds or N bytes)
- On clean exit, optionally move to permanent location

**Explicit save:**
- User triggers save to chosen path
- Support multiple formats: raw binary, hex dump, decoded text

**Buffer size:** User-configurable to control memory usage during long recordings.

---

## TUI-Specific Guidelines

### Vim Keybindings (Basic)

```
Navigation:
  j/k       - Scroll down/up
  h/l       - Scroll left/right (if content wider than terminal)
  g/G       - Go to top/bottom
  Ctrl-d/u  - Page down/up

Search:
  /         - Start search
  n/N       - Next/previous match

Views:
  1         - Traffic view
  2         - Graph view
  3         - Settings
```

---

## Development Guidelines

### Dependency Rules

```
serial-core:
  - NO ratatui, crossterm, egui, or any UI crate
  
serial-tui:
  - Depends on serial-core
```

### Adding New Features

1. **Ask: Does this belong in core or UI?**
   - Data processing, serial I/O, persistence → core
   - Keybindings, visual representation, user interaction → UI

2. **If adding to core:**
   - Expose via clean API
   - Use channels for async events
   - Emit appropriate log events for errors/warnings

3. **If adding to UI:**
   - Keep UI state in the UI crate
   - Call core API for data operations
   - Never store raw serial data in UI state (reference core's buffer)

---

## Anti-Patterns to Avoid

1. **UI state in core:** Core should not know about scroll positions, selections, or view modes.

2. **Core types with UI dependencies:** No `impl Widget for CoreStruct`.

3. **Blocking the UI on I/O:** Always use channels for serial communication.

4. **Monolithic files:** If a file exceeds ~1000 lines, consider splitting or even extracting into
   a decoupled library.

5. **Avoid `new` implementations:** Often `new` is not necessary. Should ONLY
   be added when providing some sort of logic, NOT when `Struct { ... }` can be
   used instead.

6. **Dumb functions that can be auto-generated:** Usage of crates such as
   `strum`, `thiserror`, etc. is incentivised when it can simplify the codebase
   and remove the need for boilerplate code.

# AGENTS.md - Serial Monitor Guide

Agent-facing architecture and verification notes for this repository.

## Project Shape

This is a serial monitor workspace with a strict core/frontend split:

- `crates/serial-core`: frontend-agnostic library for serial I/O, buffering, parsing, persistence helpers, settings primitives, and reusable data operations.
- `crates/serial-tui`: ratatui/crossterm frontend with vim-like keybindings.
- `crates/serial-test`: fake serial device utility for manual and tmux-driven testing.

The core must have zero UI framework dependencies. `serial-tui` may depend on `serial-core`; `serial-core` must not depend on ratatui, crossterm, egui, iced, or any frontend crate.

## Architecture Rules

- Each serial connection is a session. Multi-port support means multiple sessions.
- Core owns raw serial bytes as the source of truth. Encodings such as UTF-8, ASCII, hex, and binary are display/export representations.
- Data chunks include timestamp, direction (`Tx` or `Rx`), and raw bytes.
- UI state stays in frontends: scroll position, selected tab, focus, modal state, search cursor, view mode, etc.
- Serial I/O must not block rendering. Use async tasks and channels/events for serial work.
- Core emits events/loggable errors; frontends decide how to display them.

When adding a feature, decide ownership first:

- Core: serial I/O, chunking, buffering, search/filter primitives, graph parsing, file saving, persistence utilities.
- TUI: keybindings, layout, colors, widget state, focus, modal behavior, user interaction.

## Data Behavior

- Buffering appends new chunks and drops oldest data when configured limits are reached.
- Auto-save should persist data before it is dropped from memory.
- Search operates on the displayed representation, not raw bytes. In hex mode, searching `E` in `DE AD BE EF` should find displayed `E` characters.
- UTF-8 decoding should replace invalid bytes rather than crashing.
- Graph parsing is lazy: parse buffered data when graph view is first enabled, then parse new data as it arrives.

## TUI Keybinding Baseline

```text
Navigation: j/k, h/l, g/G, Ctrl-d/Ctrl-u
Search:     /, n/N, ? (backwards)
Views:      1 traffic, 2 graph, 3 file sender
Traffic:    s (send), f (filter), v (visual), y (yank), c (toggle config panel)
Connected:  d (disconnect), Ctrl+b (lock bottom)
Modals:     Ctrl+g (confirm), Esc (cancel)
Commands:   :connect, :disconnect, :save, :help, :sessions, :settings, :quit
Panels:     Ctrl+h/Ctrl+l
```

**Modal flow:** `:connect <port>` Enter opens a settings modal. Press `Ctrl+g` to confirm connection, `Esc` to cancel.

Prefer preserving existing TUI visual language and interaction patterns unless the task explicitly asks for redesign.

## Testing Strategy

- Use Rust unit tests for deterministic core behavior: parsers, buffer truncation, chunking, search, settings mapping, file saving.
- Use focused ratatui render tests only when exact widget/layout output matters.
- Use the tmux harness for full TUI behavior: keybindings, focus, modals, colors, terminal resizing, and real terminal rendering.
- Use `serial-test` when a test needs fake serial data or a real PTY.

Avoid tests that only prove Rust basics or compilation, such as setting a boolean and asserting `!value`. Test behavior, edge cases, and integration points.

## tmux Harness

Helper scripts live in `scripts/tmux/` and are intentionally composable rather than scenario-specific.

Default session: `serial-monitor-test`.

Useful environment overrides:

- `SERIAL_MONITOR_TMUX_SESSION`
- `SERIAL_MONITOR_TMUX_WIDTH`
- `SERIAL_MONITOR_TMUX_HEIGHT`
- `SERIAL_MONITOR_TMUX_HOME`

Basic flow:

```bash
scripts/tmux/start
scripts/tmux/capture-text
scripts/tmux/send q
scripts/tmux/stop
```

Custom command:

```bash
scripts/tmux/start cargo run -p serial-tui
```

Drive keys with tmux key names. `C-` prefix means Ctrl, so `C-g` is Ctrl+g, `C-l` is Ctrl+l. Quoting rules follow `tmux send-keys`: wrap arguments containing spaces in quotes, use literal key names like `Enter`, `Space`, `Escape`.

```bash
scripts/tmux/send ':' 'help' Enter
scripts/tmux/send C-l j j Space
scripts/tmux/send q
```

Cleanup after tests — orphaned `socat` processes can block reconnection on the same PTY path:

```bash
scripts/tmux/stop
pkill -x socat  # kill leftover socat processes from serial-test
```

Capture output:

- `scripts/tmux/capture-text`: plain visible text, best for broad behavior checks.
- `scripts/tmux/capture-ansi`: preserves styling, best for colors/modifiers.

For color checks, verify semantic context and style together. For example, inspect ANSI escapes around `Available Ports`, `Configuration`, or `[Ctrl+g]`; do not assert only on a standalone color code.

Always stop tmux sessions after verification:

```bash
scripts/tmux/stop
```

## serial-test Fake Device

`serial-test` creates a PTY pair with `socat`; the TUI connects to the printed or ready-file PTY.

Modes:

- `hex`: random bytes.
- `ascii`: readable lines.
- `sensor`: `temp`, `humidity`, and `pressure` lines.
- `echo`: echoes bytes sent by the TUI.
- `utf8`: wide Unicode/emoji/combining-character stress data.
- `flood`: high-speed line flood.

Harness-friendly flags:

- `--ready-file <path>` writes the TUI-connectable PTY path.
- `--seed <n>` makes generated data deterministic.
- `--interval-ms <n>` controls write cadence where supported.
- `--lines <n>` exits after a finite number of chunks/lines where supported.

Typical TUI + fake serial workflow:

```bash
cargo run -p serial-test -- ascii --ready-file /tmp/serial-monitor-pty --seed 1 --interval-ms 50
PTY=$(cat /tmp/serial-monitor-pty)
scripts/tmux/start
scripts/tmux/send ':' "connect ${PTY}" Enter C-g
scripts/tmux/capture-text
scripts/tmux/stop
```

Use `echo` mode to verify TX paths: connect the TUI, send bytes, and confirm echoed RX appears.

```bash
cargo run -p serial-test -- echo --ready-file /tmp/serial-monitor-pty --seed 1
PTY=$(cat /tmp/serial-monitor-pty)
scripts/tmux/start
scripts/tmux/send ':' "connect ${PTY}" Enter C-g
sleep 2
scripts/tmux/send s 'Hello from TUI' Enter
sleep 1
scripts/tmux/capture-text
scripts/tmux/stop
pkill -x socat
```

Note: In echo mode the RX data matches TX exactly. The traffic pane shows `filter: 0/N` when data is filtered by the current delimiter/chunking setting. If connection stats show RX/TX bytes but the traffic pane says "No data yet", check `Show TX`/`Show RX` toggles and delimiter settings. Scrolling with `g`/`G` or pressing `Ctrl+b` (lock to bottom) may also reveal hidden content.

## Development Preferences

- Keep changes small and local.
- Prefer public fields for crate-internal state over trivial getters/setters.
- Add methods only when they enforce invariants or do real work.
- Avoid unnecessary `new` constructors when struct literals are clearer.
- Use crates like `thiserror`, `strum`, and builders where they remove boilerplate.
- Split files that become difficult to navigate, especially large UI modules.
- Comments should explain intent, invariants, or non-obvious side effects; avoid narrating the code.
- Do not add compatibility shims unless there is persisted data, released behavior, external consumers, or an explicit requirement.

## Common Anti-Patterns

1. **UI state in core:** Core should not know about scroll positions, selections, focus, modals, selected tabs, or view modes.

2. **Core types with UI dependencies:** Do not add ratatui/crossterm/egui/iced imports to `serial-core`, and do not implement widgets for core types.

3. **Blocking the UI on I/O:** Serial operations must not run on the UI/render loop. Use async tasks, channels, and events.

4. **Duplicating raw serial data in frontends:** Frontends should render from core buffers rather than maintaining their own source-of-truth byte storage.

5. **Monolithic files:** If a file becomes hard to navigate, especially large UI modules, split it by view/widget/state responsibility.

6. **Unnecessary `new` constructors:** Prefer struct literals when construction has no logic. Add `new` only when it enforces defaults, invariants, or meaningful setup.

7. **Trivial getters/setters:** Avoid methods like `fn show_tx(&self) -> bool { self.show_tx }`. Prefer public crate-internal fields unless a method protects invariants or performs real work.

8. **Manual boilerplate when crates help:** Use crates like `thiserror`, `strum`, builders, and derives when they remove repetitive, low-value code.

9. **Tests that prove Rust basics:** Avoid tests that only verify assignment, boolean negation, or compilation. Test behavior, edge cases, and integration points.

10. **Narrating comments:** Comments should explain intent, invariants, constraints, or non-obvious side effects. Do not add comments that merely restate the code.

11. **Compatibility shims without a reason:** Do not add backward-compatibility code unless there is persisted data, released behavior, external consumers, or an explicit requirement.

12. **Rigid TUI harness scripts:** Keep tmux helpers composable. Avoid scenario-specific scripts when a few `start`, `send`, and `capture` steps are enough.

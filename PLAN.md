# PLAN.md - Serial Monitor Implementation Plan

## Implementation Order

Recommended order to build incrementally:

1. **serial-core: Basic session and buffer** - Store raw bytes with timestamps and direction
2. **serial-core: Serial port enumeration** - List available ports
3. **serial-core: Serial port connection** - Connect, read, write with configurable parameters (baud, parity, stop bits, flow control)
4. **serial-tui: Minimal traffic view** - Display raw hex, basic scrolling
5. **serial-tui: Port selection UI** - List ports, connect/disconnect
6. **serial-core: Encoding system** - Add UTF-8, ASCII, Hex, Binary conversion utilities
7. **serial-tui: Encoding switching** - Keybinding to cycle encodings (e.g., `e`)
8. **serial-tui: Vim navigation** - j/k/g/G/Ctrl-d/u
9. **serial-tui: Search** - /pattern, n/N navigation (operates on displayed encoding)
10. **serial-core: File sending** - With chunking and delays
11. **serial-tui: File send UI** - Progress display
12. **serial-core: Graph data parsing** - Generic number extractor
13. **serial-tui: Graph view** - Line charts with ratatui
14. **serial-core: Persistence** - Auto-save, explicit save
15. **serial-tui: Command mode** - :commands (including :set encoding=hex)
16. **Polish:** Error handling, edge cases, configuration

---

## Open Questions / Future Considerations

- **Custom parsing scripts:** Lua? WASM? Defer until core features stable.
- **Multi-port UI:** Tabs? Split panes? Decide when implementing.
- **Plugin system:** Consider after v1.0 if demand exists.
- **Cross-platform paths:** Use `dirs` crate for XDG-compliant paths on Linux, appropriate paths on Windows/macOS.

# Display Module - Implementation Plan

## Overview

The `display` module provides a unified pipeline from raw serial data bytes to searchable, filterable display content. It replaces the standalone `search` module and integrates encoding, filtering, and searching into a single coherent system.

## Purpose

- **Encoding**: Convert raw bytes to display strings (UTF-8, ASCII, Hex, Binary) with configurable formatting
- **Filtering**: Pattern-based and direction-based (TX/RX) filtering of visible chunks
- **Searching**: Find matches within the current view with navigation support
- **Caching**: Maintain encoded representations to avoid re-encoding on every render

## Architecture

```
Raw DataChunks (VecDeque<DataChunk>)
         │
         ▼ sync()
┌─────────────────────────────────────────────────────────────┐
│                      DisplayBuffer                           │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  all_chunks: VecDeque<Rc<DisplayChunk>>               │  │
│  │  (encoded 1:1 with raw data)                          │  │
│  └───────────────────────────────────────────────────────┘  │
│         │                                                    │
│         ▼ filter (when active)                               │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  filtered_chunks: VecDeque<Rc<DisplayChunk>>          │  │
│  │  (Rc clones - shares data, not copies)                │  │
│  └───────────────────────────────────────────────────────┘  │
│         │                                                    │
│         ▼ unified accessor                                   │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  chunks() -> &VecDeque<Rc<DisplayChunk>>              │  │
│  │  Returns filtered_chunks if filter active,            │  │
│  │  otherwise all_chunks                                 │  │
│  └───────────────────────────────────────────────────────┘  │
│         │                                                    │
│         ▼ search (on chunks() view)                          │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  SearchState                                          │  │
│  │  - pattern matching                                   │  │
│  │  - matches: Vec<SearchMatch>                          │  │
│  │  - navigation (current, next, prev)                   │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
         │
         ▼
    Frontend renders chunks() with highlighted matches
```

## Key Design Decisions

### Rc-based Filtering

We use `Rc<DisplayChunk>` for memory-efficient filtering:

- `all_chunks: VecDeque<Rc<DisplayChunk>>` - source of truth
- `filtered_chunks: VecDeque<Rc<DisplayChunk>>` - Rc clones (cheap, shares data)

Benefits:
1. **Decoupled filtering**: `filtered_chunks` is standalone - consumers don't know about indices
2. **Fast toggling**: Rebuilding filter just clones Rcs (cheap pointer copies)
3. **Simple API**: `chunks()` returns same type regardless of filter state
4. **Clean truncation**: Use `Rc::strong_count()` to detect stale entries

### Unified Accessor

The `chunks()` method provides a single view:

```rust
impl DisplayBuffer {
    pub fn chunks(&self) -> &VecDeque<Rc<DisplayChunk>> {
        if self.filter.is_active() {
            &self.filtered_chunks
        } else {
            &self.all_chunks
        }
    }
}
```

Search operates on `chunks()` - it doesn't know or care about filtering.

### Reference-Count Based Truncation

When chunks are dropped from `all_chunks`, we clean `filtered_chunks` using ref counts:

```rust
// After dropping from all_chunks, clean filtered_chunks
while let Some(front) = self.filtered_chunks.front() {
    if Rc::strong_count(front) == 1 {
        // Only exists in filtered_chunks - was dropped from all_chunks
        self.filtered_chunks.pop_front();
    } else {
        break; // Still referenced by all_chunks
    }
}
```

This works because:
- Truncation happens in order (oldest first)
- `Rc::strong_count()` is O(1)
- No need for IDs or sequence numbers

## Module Structure

```
display/
├── PLAN.md         # This file
├── mod.rs          # Public exports
├── buffer.rs       # DisplayBuffer - main orchestrator
├── chunk.rs        # DisplayChunk type
├── encoding.rs     # Encoding enum + format options
├── filter.rs       # FilterState (internal)
├── search.rs       # SearchState + SearchMatch
└── pattern.rs      # PatternMatcher + PatternMode
```

## Key Types

### DisplayChunk
Represents a single encoded chunk ready for display.
- `content: String` - encoded representation
- `direction: Direction` - TX or RX

### Encoding
Enum with variants for different display modes:
- `Utf8` - UTF-8 with replacement for invalid sequences
- `Ascii` - ASCII with dots for non-printable
- `Hex(HexFormat)` - Hexadecimal with grouping options
- `Binary(BinaryFormat)` - Binary with grouping options

### HexFormat / BinaryFormat
Configurable formatting:
- `group_size` - bytes/bits per group (0 = no grouping)
- `separator` - character between groups
- `uppercase` (hex only) - uppercase hex digits

### SearchMatch
Position of a match within display content:
- `chunk_index: usize` - index in current view (`chunks()`)
- `byte_start: usize` - start offset in chunk content
- `byte_end: usize` - end offset in chunk content

### PatternMatcher / PatternMode
Reused pattern matching with literal (SIMD memchr) and regex modes.

## DisplayBuffer API

### Construction
- `DisplayBuffer { encoding, ..Default::default() }` - struct initialization

### Encoding
- `encoding() -> Encoding`
- `set_encoding(encoding: Encoding)` - marks for re-encode on next sync

### Data Synchronization
- `sync(raw_chunks: &VecDeque<DataChunk>, dropped: usize)`
  - Encodes new chunks
  - Handles buffer truncation
  - Updates filter and search incrementally

### Filtering
- `set_filter_pattern(pattern, mode) -> Result`
- `set_filter_mode(mode) -> Result`
- `clear_filter_pattern()`
- `filter_pattern() -> Option<&str>`
- `filter_mode() -> PatternMode`
- `filter_error() -> Option<&str>`
- `set_show_tx(bool)` / `set_show_rx(bool)`
- `show_tx() -> bool` / `show_rx() -> bool`

### Searching
- `set_search_pattern(pattern, mode) -> Result`
- `set_search_mode(mode) -> Result`
- `clear_search()`
- `search_pattern() -> Option<&str>`
- `search_mode() -> PatternMode`
- `search_error() -> Option<&str>`
- `matches() -> &[SearchMatch]`
- `match_count() -> usize`
- `goto_next_match() -> Option<usize>` - returns chunk_index in `chunks()`
- `goto_prev_match() -> Option<usize>`
- `current_match_index() -> Option<usize>`
- `current_match() -> Option<&SearchMatch>`
- `matches_in_chunk(chunk_index) -> impl Iterator`
- `is_current_match(&SearchMatch) -> bool`
- `search_status() -> String`

### Chunk Access
- `chunks() -> &VecDeque<Rc<DisplayChunk>>` - unified view (filtered or all)
- `len() -> usize` - count of current view
- `is_empty() -> bool`

## Invalidation Rules

Changes cascade through the system:

| Action | Re-encode | Rebuild Filter | Invalidate Search |
|--------|-----------|----------------|-------------------|
| `set_encoding()` | Yes (on sync) | Yes | Yes |
| `sync()` - new data | Only new | Evaluate new | Search new |
| `sync()` - truncation | No | Ref-count cleanup | Adjust matches |
| `set_filter_*()` | No | Yes | Yes |
| `set_show_tx/rx()` | No | Yes | Yes |
| `clear_filter_pattern()` | No | Yes | Yes |
| `set_search_*()` | No | No | Yes |
| `clear_search()` | No | No | Yes |

## Incremental Processing

### On `sync()` with new data:
1. Encode new chunks → push as `Rc<DisplayChunk>` to `all_chunks`
2. If filter active: check each new chunk, push Rc clone to `filtered_chunks` if passes
3. Search: search new chunks in `chunks()`, append matches

### On `sync()` with truncation (`dropped > 0`):
1. Pop `dropped` chunks from front of `all_chunks`
2. Clean `filtered_chunks` using ref-count check (see above)
3. Search: remove matches with `chunk_index < removed_count`, adjust remaining indices

### On filter change:
1. Clear `filtered_chunks`
2. If filter active: rebuild by iterating `all_chunks`, cloning Rcs that pass
3. Invalidate search (will re-search on next `matches()` call)

### On search change:
1. Clear matches
2. Next `matches()` call triggers full search of `chunks()`

## Implementation Order

1. **pattern.rs** - Already complete
2. **encoding.rs** - Implement encode functions (todo stubs exist)
3. **chunk.rs** - Simple struct (complete, remove `new()`)
4. **filter.rs** - FilterState with Rc-based filtered view
5. **search.rs** - SearchState operating on unified `chunks()` view
6. **buffer.rs** - DisplayBuffer orchestrating all components
7. **mod.rs** - Public exports

## Testing Strategy

Each component should have unit tests:
- `pattern.rs` - Already tested
- `encoding.rs` - Test each encoding, grouping options, edge cases
- `filter.rs` - Pattern filtering, direction filtering, Rc cleanup, rebuild
- `search.rs` - Match finding, navigation, incremental updates, truncation
- `buffer.rs` - Integration tests for full pipeline, invalidation cascades

## Migration Notes

After implementation:
1. Update `lib.rs` to export `display` module
2. Remove `search` module entirely
3. Remove old `encoding.rs` (functionality moved to display/encoding.rs)
4. Update TUI to use `DisplayBuffer`

## Dependencies

- `memchr` - SIMD-accelerated literal search
- `regex` - Regular expression support
- `strum` - Enum utilities for PatternMode, Encoding

## Open Questions (Resolved)

- ~~Should search operate on filtered or all data?~~ **Unified `chunks()` view**
- ~~Where does direction filtering belong?~~ **In FilterState**
- ~~Encoding ownership?~~ **Frontend sets it, module stores and uses it**
- ~~Indices vs Rc for filtering?~~ **Rc for decoupled, efficient filtering**
- ~~How to handle truncation with Rc?~~ **Ref-count check on `filtered_chunks` front**

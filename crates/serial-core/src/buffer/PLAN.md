# Buffer Module Architecture

Central data management for serial monitor. Handles storage, encoding, filtering, searching, graphing, and file operations.

## Core Design Principle

**Raw bytes are hidden from frontends.** Frontends only see encoded strings via `ChunkView`.

## Data Flow

```
Serial I/O
    │
    ▼ push()
┌─────────────────────────────────────────────────────┐
│                    DataBuffer                        │
│                                                      │
│  ┌─────────────────┐    ┌─────────────────────────┐ │
│  │ raw_chunks      │    │ encoded                 │ │
│  │ VecDeque<Raw>   │───►│ VecDeque<String>        │ │
│  │ (pub(crate))    │    │ (1:1 with raw)          │ │
│  └─────────────────┘    └─────────────────────────┘ │
│           │                        │                 │
│           │              ┌─────────┴─────────┐      │
│           │              ▼                   ▼      │
│           │    ┌──────────────┐    ┌─────────────┐  │
│           │    │ filtered_idx │    │ SearchState │  │
│           │    │ Vec<usize>   │    │ matches     │  │
│           │    └──────────────┘    └─────────────┘  │
│           │                                         │
│           ▼                                         │
│    ┌─────────────┐                                  │
│    │ GraphEngine │  (lazy init, uses raw as UTF-8) │
│    └─────────────┘                                  │
│                                                      │
└─────────────────────────────────────────────────────┘
    │
    ▼ chunks() -> Iterator<ChunkView>
Frontend (only sees encoded + metadata)
```

## Key Types

### RawChunk (internal)
```rust
pub(crate) struct RawChunk {
    data: Vec<u8>,
    direction: Direction,
    timestamp: SystemTime,
}
```

### ChunkView (public, borrowed)
```rust
pub struct ChunkView<'a> {
    pub encoded: &'a str,
    pub direction: Direction,
    pub timestamp: SystemTime,
}
```

### DataBuffer (public)
```rust
pub struct DataBuffer {
    // Storage
    raw_chunks: VecDeque<RawChunk>,
    encoded: VecDeque<String>,
    
    // View state
    filtered_indices: Vec<usize>,
    encoding: Encoding,
    filter: FilterState,
    search: SearchState,
    
    // Size management
    current_size: usize,
    max_size: usize,
    
    // Optional features
    graph: Option<GraphEngine>,
}
```

## Module Structure

```
buffer/
├── mod.rs           # Public API, DataBuffer
├── chunk.rs         # RawChunk, ChunkView, Direction
├── encoding.rs      # Encoding enum, encode functions
├── filter.rs        # FilterState
├── search.rs        # SearchState, SearchMatch
├── pattern.rs       # PatternMatcher, PatternMode
└── graph/           # Graph engine (lazy init)
    ├── mod.rs
    ├── engine.rs
    └── parser.rs
```

## Key Operations

### Push (internal, called by Session I/O)
```rust
pub(crate) fn push(&mut self, data: Vec<u8>, direction: Direction)
```
1. Create RawChunk with timestamp
2. Encode to string, append to `encoded`
3. Update size, truncate if needed
4. Check filter, maybe add to `filtered_indices`
5. Feed to graph if enabled

### Truncation
When `current_size > max_size`:
1. Pop oldest from `raw_chunks` and `encoded`
2. Adjust `filtered_indices` (subtract 1 from all, remove index 0 if present)
3. Invalidate affected search matches

### Encoding Change
When encoding changes:
1. Re-encode all chunks
2. Rebuild `filtered_indices`
3. Invalidate search

### Chunks Access (public)
```rust
pub fn chunks(&self) -> impl Iterator<Item = ChunkView<'_>>
```
Returns filtered view if filter active, otherwise all chunks.
Frontend doesn't need to know about filtering.

## Graph Engine

- Lives in `buffer/graph/`
- Lazy initialization: only created when frontend enables it
- Parses raw bytes as UTF-8 (bypasses encoding setting)
- Maintains own parsed data cache

## File Operations

Future: file saving will be part of this module since it needs:
- Raw bytes for binary export
- Encoded strings for text export
- Access to filter state for "save filtered only"

//! Graph engine
//!
//! The [`GraphEngine`] manages graph data parsing and storage.
//! It supports lazy initialization: historical data is parsed on first request,
//! then new data is parsed incrementally.

use strum::{AsRefStr, Display, EnumIter};

use crate::buffer::DataChunk;

use super::data::{GraphBuffer, GraphDataPoint, PacketRateData};
use super::parser::{GraphParser, GraphParserConfig, ParsedValue};

/// Graph visualization mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Display, AsRefStr, EnumIter)]
pub enum GraphMode {
    /// Show packet rate over time (no parsing needed)
    #[default]
    #[strum(serialize = "Packet Rate")]
    PacketRate,
    /// Show parsed data values
    #[strum(serialize = "Parsed Data")]
    ParsedData,
}

/// Configuration for the graph engine
#[derive(Debug, Clone)]
pub struct GraphEngineConfig {
    /// Current graph mode
    pub mode: GraphMode,
    /// Parser configuration (for ParsedData mode)
    pub parser_config: GraphParserConfig,
    /// Maximum points per series
    pub max_points_per_series: usize,
    /// Maximum packet rate samples
    pub max_rate_samples: usize,
}

impl Default for GraphEngineConfig {
    fn default() -> Self {
        Self {
            mode: GraphMode::PacketRate,
            parser_config: GraphParserConfig::default(),
            max_points_per_series: GraphBuffer::DEFAULT_MAX_POINTS,
            max_rate_samples: PacketRateData::DEFAULT_MAX_SAMPLES,
        }
    }
}

/// Main graph engine that manages parsing and data storage
///
/// The engine supports two modes:
/// - **PacketRate**: Counts packets per time window (no parsing)
/// - **ParsedData**: Extracts numeric values using a configurable parser
///
/// # Lazy Initialization
///
/// The engine is designed for lazy initialization. When first created, it has
/// no data. Call [`initialize`](Self::initialize) with historical chunks to
/// populate it with existing data, then call [`process_chunk`](Self::process_chunk)
/// for new data.
#[derive(Debug)]
pub struct GraphEngine {
    /// Current configuration
    config: GraphEngineConfig,
    /// Packet rate data (always tracked)
    packet_rate: PacketRateData,
    /// Parsed data buffer
    parsed_data: GraphBuffer,
    /// The active parser instance
    parser: Box<dyn GraphParser>,
    /// Number of chunks processed (for tracking)
    chunks_processed: usize,
}

impl std::fmt::Debug for Box<dyn GraphParser> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GraphParser")
            .field("type", &self.parser_type().to_string())
            .finish()
    }
}

impl GraphEngine {
    /// Create a new graph engine with default configuration
    pub fn new() -> Self {
        Self::with_config(GraphEngineConfig::default())
    }

    /// Create a new graph engine with custom configuration
    pub fn with_config(config: GraphEngineConfig) -> Self {
        let parser = config.parser_config.create_parser();
        let packet_rate = PacketRateData::with_config(
            PacketRateData::DEFAULT_WINDOW_SIZE,
            config.max_rate_samples,
        );
        let parsed_data = GraphBuffer::with_max_points(config.max_points_per_series);

        Self {
            config,
            packet_rate,
            parsed_data,
            parser,
            chunks_processed: 0,
        }
    }

    /// Initialize the engine with historical data
    ///
    /// Call this once when first enabling graph view to process all
    /// existing buffered data.
    pub fn initialize<'a>(&mut self, chunks: impl Iterator<Item = &'a DataChunk>) {
        for chunk in chunks {
            self.process_chunk_internal(chunk);
        }
    }

    /// Process a new data chunk
    ///
    /// Call this for each new chunk received after initialization.
    pub fn process_chunk(&mut self, chunk: &DataChunk) {
        self.process_chunk_internal(chunk);
    }

    /// Internal chunk processing
    fn process_chunk_internal(&mut self, chunk: &DataChunk) {
        self.chunks_processed += 1;

        // Always update packet rate data
        self.packet_rate
            .record(chunk.timestamp, chunk.direction, chunk.data.len());

        // Parse data for the parsed data buffer
        let values = self.parser.parse(chunk);
        for ParsedValue { series, value } in values {
            self.parsed_data.push(
                &series,
                GraphDataPoint::new(chunk.timestamp, value, chunk.direction),
            );
        }
    }

    /// Get the packet rate data
    pub fn packet_rate(&self) -> &PacketRateData {
        &self.packet_rate
    }

    /// Get the parsed data buffer
    pub fn parsed_data(&self) -> &GraphBuffer {
        &self.parsed_data
    }

    /// Get a mutable reference to the parsed data buffer
    pub fn parsed_data_mut(&mut self) -> &mut GraphBuffer {
        &mut self.parsed_data
    }

    /// Get the current mode
    pub fn mode(&self) -> GraphMode {
        self.config.mode
    }

    /// Set the graph mode
    pub fn set_mode(&mut self, mode: GraphMode) {
        self.config.mode = mode;
    }

    /// Get the current parser configuration
    pub fn parser_config(&self) -> &GraphParserConfig {
        &self.config.parser_config
    }

    /// Set a new parser configuration
    ///
    /// This creates a new parser instance. Existing parsed data is NOT cleared -
    /// call [`clear_parsed_data`](Self::clear_parsed_data) if you want to reparse.
    pub fn set_parser_config(&mut self, config: GraphParserConfig) {
        self.parser = config.create_parser();
        self.config.parser_config = config;
    }

    /// Change the parser and reparse all data
    ///
    /// This is a convenience method that changes the parser and then
    /// reparses all provided chunks from scratch.
    pub fn reparse_with_config<'a>(
        &mut self,
        config: GraphParserConfig,
        chunks: impl Iterator<Item = &'a DataChunk>,
    ) {
        self.set_parser_config(config);
        self.clear_parsed_data();

        // Re-parse all chunks (packet rate is already populated)
        for chunk in chunks {
            let values = self.parser.parse(chunk);
            for ParsedValue { series, value } in values {
                self.parsed_data.push(
                    &series,
                    GraphDataPoint::new(chunk.timestamp, value, chunk.direction),
                );
            }
        }
    }

    /// Get series names from parsed data
    pub fn series_names(&self) -> Vec<&str> {
        self.parsed_data.series_names().collect()
    }

    /// Toggle visibility of a series
    pub fn toggle_series_visibility(&mut self, name: &str) {
        self.parsed_data.toggle_visibility(name);
    }

    /// Get the number of chunks processed
    pub fn chunks_processed(&self) -> usize {
        self.chunks_processed
    }

    /// Clear parsed data (keeps packet rate data)
    pub fn clear_parsed_data(&mut self) {
        self.parsed_data.clear();
    }

    /// Clear all data
    pub fn clear(&mut self) {
        self.packet_rate.clear();
        self.parsed_data.clear();
        self.chunks_processed = 0;
    }

    /// Check if there is any data
    pub fn is_empty(&self) -> bool {
        self.chunks_processed == 0
    }
}

impl Default for GraphEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Direction;

    fn make_chunk(data: &str) -> DataChunk {
        DataChunk::new(Direction::Rx, data.as_bytes().to_vec())
    }

    #[test]
    fn test_engine_basic() {
        let mut engine = GraphEngine::new();
        assert!(engine.is_empty());

        engine.process_chunk(&make_chunk("temp=25.5"));
        assert!(!engine.is_empty());
        assert_eq!(engine.chunks_processed(), 1);
    }

    #[test]
    fn test_engine_packet_rate() {
        let mut engine = GraphEngine::new();

        engine.process_chunk(&make_chunk("data1"));
        engine.process_chunk(&make_chunk("data2"));
        engine.process_chunk(&make_chunk("data3"));

        let rate = engine.packet_rate();
        let samples: Vec<_> = rate.samples().collect();

        // All should be in the same time window (test runs fast)
        assert!(!samples.is_empty());
        assert!(samples[0].rx_count >= 1);
    }

    #[test]
    fn test_engine_parsed_data() {
        let mut engine = GraphEngine::new();

        engine.process_chunk(&make_chunk("temp=25.5, humidity=60"));
        engine.process_chunk(&make_chunk("temp=26.0, humidity=58"));

        let parsed = engine.parsed_data();
        assert_eq!(parsed.series_count(), 2);

        let temp = parsed.series("temp").unwrap();
        assert_eq!(temp.len(), 2);

        let humidity = parsed.series("humidity").unwrap();
        assert_eq!(humidity.len(), 2);
    }

    #[test]
    fn test_engine_initialize() {
        let mut engine = GraphEngine::new();

        let chunks = vec![
            make_chunk("temp=20"),
            make_chunk("temp=21"),
            make_chunk("temp=22"),
        ];

        engine.initialize(chunks.iter());

        assert_eq!(engine.chunks_processed(), 3);
        assert_eq!(engine.parsed_data().series("temp").unwrap().len(), 3);
    }

    #[test]
    fn test_engine_series_visibility() {
        let mut engine = GraphEngine::new();

        engine.process_chunk(&make_chunk("temp=25, humidity=60"));

        assert!(engine.parsed_data().series("temp").unwrap().visible);

        engine.toggle_series_visibility("temp");
        assert!(!engine.parsed_data().series("temp").unwrap().visible);

        engine.toggle_series_visibility("temp");
        assert!(engine.parsed_data().series("temp").unwrap().visible);
    }
}

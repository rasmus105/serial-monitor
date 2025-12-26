use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, SystemTime},
};

use crate::{DataChunk, Direction, GraphParser};

use super::parser::GraphParserType;

// ============================================================================
// Graph Mode
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GraphMode {
    /// Parse serial data and display as points on a graph.
    #[default]
    ParsedData,
    /// Display the incoming/outcoming packet rates over time.
    PacketRate,
}

// ============================================================================
// Graph Data Point
// ============================================================================

/// A single data point for graphing
#[derive(Debug, Clone, PartialEq)]
pub struct GraphDataPoint {
    /// When this value was recorded
    pub timestamp: SystemTime,
    /// The numeric value
    pub value: f64,
    /// Which direction the source chunk came from
    pub direction: Direction,
}

// ============================================================================
// Graph Series
// ============================================================================

#[derive(Debug, Clone)]
pub struct GraphSeries {
    /// Name of the series (e.g., "temperature", "humidity")
    pub name: String,
    /// Data points in chronological order
    pub points: VecDeque<GraphDataPoint>,
    /// Optional color hint (index into a color palette) - mostly for frontend
    /// (mostly intended to allow frontend to store color)
    pub color: u8,
    /// Whether this series is visible in the UI
    /// (mostly intended to allow frontend to store visibility bool)
    pub visible: bool,
}

// ============================================================================
// Packet Rate Tracking
// ============================================================================

/// A single time window sample for packet rate visualization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PacketRateSample {
    pub window_start: SystemTime,
    pub rx_count: u32,
    pub tx_count: u32,
    pub rx_bytes: usize,
    pub tx_bytes: usize,
}

/// Packet rate tracking data
///
/// Tracks RX and TX packet counts over time windows for rate visualization.
/// This doesn't require any parsing - it just counts chunks.
#[derive(Debug, Clone)]
pub struct PacketRateData {
    /// Time-windowed packet counts
    pub samples: VecDeque<PacketRateSample>,
    /// Size of each time window
    pub window_size: Duration,
    /// Maximum number of samples to keep
    pub max_samples: usize,
}

// ============================================================================
// Graph Engine Config
// ============================================================================

#[derive(Debug)]
pub struct GraphEngineConfig {
    /// Parse incoming serial data as points to be displayed on a graph.
    pub parser: Box<dyn GraphParser>,
    /// Keep track of incoming/outcoming packes rates over time (for `PacketRate` mode)
    pub packet_rate: PacketRateData,
}

// ============================================================================
// Graph Engine
// ============================================================================

/// Main entry struct for usage of graph parsing.
/// Allows lazy-initialization.
#[derive(Debug)]
pub struct GraphEngine {
    /// configuration for *how* data should be parsed.
    pub config: GraphEngineConfig,

    /// All different graph series and their data
    pub series: HashMap<String, GraphSeries>,

    pub chunks_processed: usize,
}

impl GraphEngine {
    pub fn from_parser(_parser: GraphParserType) -> Self {
        todo!()
    }

    pub fn reparse_with_parser(&mut self, _parser: GraphParserType) {
        todo!()
    }

    /// Initialize the engine with historical data
    ///
    /// Call this once when first enabling graph view to process all
    /// existing buffered data.
    pub fn initialize<'a>(&mut self, chunks: impl Iterator<Item = &'a DataChunk>) {
        for chunk in chunks {
            self.process_chunk(chunk);
        }
    }

    pub fn process_chunk(&mut self, _chunk: &DataChunk) {
        todo!()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph_v2::parser::{Csv, Json, KeyValue, RawNumbers, Regex};

    fn chunk(data: &str) -> DataChunk {
        DataChunk::new(Direction::Rx, data.as_bytes().to_vec())
    }

    fn engine(parser: impl Into<GraphParserType>) -> GraphEngine {
        GraphEngine::from_parser(parser.into())
    }

    // -------------------------------------------------------------------------
    // KeyValue Parser Tests
    // -------------------------------------------------------------------------

    #[test]
    fn key_value_simple() {
        let mut engine = engine(KeyValue);
        engine.process_chunk(&chunk("temp=25.5"));

        assert_eq!(engine.series["temp"].points[0].value, 25.5);
    }

    #[test]
    fn key_value_multiple_pairs() {
        let mut engine = engine(KeyValue);
        engine.process_chunk(&chunk("temp=25.5, humidity=60"));

        assert_eq!(engine.series["temp"].points[0].value, 25.5);
        assert_eq!(engine.series["humidity"].points[0].value, 60.0);
    }

    #[test]
    fn key_value_colon_separator() {
        let mut engine = engine(KeyValue);
        engine.process_chunk(&chunk("temperature: 41.3"));

        assert_eq!(engine.series["temperature"].points[0].value, 41.3);
    }

    #[test]
    fn key_value_negative_number() {
        let mut engine = engine(KeyValue);
        engine.process_chunk(&chunk("offset=-12.5"));

        assert_eq!(engine.series["offset"].points[0].value, -12.5);
    }

    // -------------------------------------------------------------------------
    // CSV Parser Tests
    // -------------------------------------------------------------------------

    #[test]
    fn csv_simple() {
        let mut engine = engine(Csv::default());
        engine.process_chunk(&chunk("1.0,2.0,3.0"));

        assert_eq!(engine.series["col0"].points[0].value, 1.0);
        assert_eq!(engine.series["col1"].points[0].value, 2.0);
        assert_eq!(engine.series["col2"].points[0].value, 3.0);
    }

    #[test]
    fn csv_with_column_names() {
        let parser = Csv {
            delimiter: ',',
            column_names: vec!["time".into(), "temp".into(), "humidity".into()],
        };
        let mut engine = engine(parser);
        engine.process_chunk(&chunk("1000,25.5,60"));

        assert_eq!(engine.series["time"].points[0].value, 1000.0);
        assert_eq!(engine.series["temp"].points[0].value, 25.5);
        assert_eq!(engine.series["humidity"].points[0].value, 60.0);
    }

    #[test]
    fn csv_semicolon_delimiter() {
        let parser = Csv {
            delimiter: ';',
            column_names: Vec::new(),
        };
        let mut engine = engine(parser);
        engine.process_chunk(&chunk("1.0;2.0;3.0"));

        assert_eq!(engine.series["col0"].points[0].value, 1.0);
        assert_eq!(engine.series["col1"].points[0].value, 2.0);
    }

    // -------------------------------------------------------------------------
    // JSON Parser Tests
    // -------------------------------------------------------------------------

    #[test]
    fn json_simple_object() {
        let mut engine = engine(Json);
        engine.process_chunk(&chunk(r#"{"temp": 25.5, "humidity": 60}"#));

        assert_eq!(engine.series["temp"].points[0].value, 25.5);
        assert_eq!(engine.series["humidity"].points[0].value, 60.0);
    }

    #[test]
    fn json_nested_object() {
        let mut engine = engine(Json);
        engine.process_chunk(&chunk(r#"{"sensor": {"temp": 25.5}}"#));

        assert_eq!(engine.series["sensor.temp"].points[0].value, 25.5);
    }

    #[test]
    fn json_array_of_numbers() {
        let mut engine = engine(Json);
        engine.process_chunk(&chunk(r#"{"values": [1, 2, 3]}"#));

        assert_eq!(engine.series["values[0]"].points[0].value, 1.0);
        assert_eq!(engine.series["values[1]"].points[0].value, 2.0);
        assert_eq!(engine.series["values[2]"].points[0].value, 3.0);
    }

    // -------------------------------------------------------------------------
    // RawNumbers Parser Tests
    // -------------------------------------------------------------------------

    #[test]
    fn raw_numbers_simple() {
        let mut engine = engine(RawNumbers);
        engine.process_chunk(&chunk("Reading: 25.5 degrees"));

        assert_eq!(engine.series["0"].points[0].value, 25.5);
    }

    #[test]
    fn raw_numbers_multiple() {
        let mut engine = engine(RawNumbers);
        engine.process_chunk(&chunk("Values: 10, 20.5, 30"));

        assert_eq!(engine.series["0"].points[0].value, 10.0);
        assert_eq!(engine.series["1"].points[0].value, 20.5);
        assert_eq!(engine.series["2"].points[0].value, 30.0);
    }

    #[test]
    fn raw_numbers_negative() {
        let mut engine = engine(RawNumbers);
        engine.process_chunk(&chunk("Temp: -15.3"));

        assert_eq!(engine.series["0"].points[0].value, -15.3);
    }

    // -------------------------------------------------------------------------
    // Regex Parser Tests
    // -------------------------------------------------------------------------

    #[test]
    fn regex_named_capture() {
        let parser = Regex {
            pattern: r"T:(?P<temp>\d+\.?\d*)".into(),
        };
        let mut engine = engine(parser);
        engine.process_chunk(&chunk("T:25.5"));

        assert_eq!(engine.series["temp"].points[0].value, 25.5);
    }

    #[test]
    fn regex_multiple_captures() {
        let parser = Regex {
            pattern: r"T:(?P<temp>\d+\.?\d*)\s+H:(?P<humidity>\d+\.?\d*)".into(),
        };
        let mut engine = engine(parser);
        engine.process_chunk(&chunk("T:25.5 H:60"));

        assert_eq!(engine.series["temp"].points[0].value, 25.5);
        assert_eq!(engine.series["humidity"].points[0].value, 60.0);
    }

    // -------------------------------------------------------------------------
    // Engine Behavior Tests
    // -------------------------------------------------------------------------

    #[test]
    fn engine_multiple_chunks_same_series() {
        let mut engine = engine(KeyValue);
        engine.process_chunk(&chunk("temp=20"));
        engine.process_chunk(&chunk("temp=21"));
        engine.process_chunk(&chunk("temp=22"));

        let series = &engine.series["temp"];
        assert_eq!(series.points.len(), 3);
        assert_eq!(series.points[0].value, 20.0);
        assert_eq!(series.points[1].value, 21.0);
        assert_eq!(series.points[2].value, 22.0);
    }
}

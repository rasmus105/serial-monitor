use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, SystemTime},
};

use super::super::chunk::{Direction, RawChunk};
use super::parser::{GraphParser, GraphParserType};

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
    /// Window start time as nanoseconds since UNIX epoch
    pub window_start_nanos: u64,
    pub rx_count: u32,
    pub tx_count: u32,
    pub rx_bytes: usize,
    pub tx_bytes: usize,
}

impl PacketRateSample {
    /// Get the window start time as a `SystemTime`
    pub fn window_start(&self) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_nanos(self.window_start_nanos)
    }
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

impl PacketRateData {
    /// Record a packet in the appropriate time window.
    pub fn record(&mut self, timestamp: SystemTime, direction: Direction, bytes: usize) {
        let window_nanos = self.window_bucket_nanos(timestamp);

        // Check if we need a new sample
        let needs_new_sample = self
            .samples
            .back()
            .map(|s| s.window_start_nanos != window_nanos)
            .unwrap_or(true);

        if needs_new_sample {
            // Trim old samples if at capacity
            while self.samples.len() >= self.max_samples {
                self.samples.pop_front();
            }
            self.samples.push_back(PacketRateSample {
                window_start_nanos: window_nanos,
                rx_count: 0,
                tx_count: 0,
                rx_bytes: 0,
                tx_bytes: 0,
            });
        }

        // Record in the current sample
        if let Some(sample) = self.samples.back_mut() {
            match direction {
                Direction::Rx => {
                    sample.rx_count += 1;
                    sample.rx_bytes += bytes;
                }
                Direction::Tx => {
                    sample.tx_count += 1;
                    sample.tx_bytes += bytes;
                }
            }
        }
    }

    /// Round timestamp down to window boundary, returning nanoseconds since epoch.
    fn window_bucket_nanos(&self, timestamp: SystemTime) -> u64 {
        let nanos = timestamp
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_nanos() as u64;
        let window_nanos = self.window_size.as_nanos() as u64;
        (nanos / window_nanos) * window_nanos
    }
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

    /// Maximum points per series (oldest points are trimmed when exceeded)
    pub max_points_per_series: usize,

    /// Counter for assigning colors to new series
    next_color: u8,

    pub chunks_processed: usize,
}

impl GraphEngine {
    /// Default max points per series
    pub const DEFAULT_MAX_POINTS: usize = 10000;

    pub fn from_parser(parser: GraphParserType) -> Self {
        Self {
            config: GraphEngineConfig {
                parser: Box::new(parser),
                packet_rate: PacketRateData {
                    samples: VecDeque::new(),
                    window_size: Duration::from_millis(100),
                    max_samples: 6000, // 10 minutes at 100ms windows
                },
            },
            series: HashMap::new(),
            max_points_per_series: Self::DEFAULT_MAX_POINTS,
            next_color: 0,
            chunks_processed: 0,
        }
    }
}

impl Default for GraphEngine {
    fn default() -> Self {
        Self::from_parser(GraphParserType::KeyValue(super::parser::KeyValue))
    }
}

impl GraphEngine {
    /// Process a raw chunk (internal, called by DataBuffer)
    pub(crate) fn process_raw_chunk(&mut self, chunk: &RawChunk) {
        self.chunks_processed += 1;

        // Update packet rate tracking
        self.config
            .packet_rate
            .record(chunk.timestamp, chunk.direction, chunk.data.len());

        // Parse data as UTF-8 and store data points
        let text = String::from_utf8_lossy(&chunk.data);
        let values = self.config.parser.parse_str(&text, chunk.timestamp, chunk.direction);
        for value in values {
            self.next_color = self.next_color.wrapping_add(1);
            let entry = self.series.entry(value.series).or_insert(GraphSeries {
                points: VecDeque::new(),
                color: self.next_color,
                visible: true,
            });

            // Trim oldest points if at capacity
            while entry.points.len() >= self.max_points_per_series {
                entry.points.pop_front();
            }

            entry.points.push_back(GraphDataPoint {
                timestamp: chunk.timestamp,
                value: value.value,
                direction: chunk.direction,
            });
        }
    }

    /// Process raw bytes directly (convenience for DataBuffer)
    pub(crate) fn process_chunk(&mut self, data: &[u8]) {
        // Create a temporary RawChunk for processing
        let chunk = RawChunk {
            data: data.to_vec(),
            direction: Direction::Rx, // Default, will be overridden by caller if needed
            timestamp: SystemTime::now(),
        };
        self.process_raw_chunk(&chunk);
    }

    /// Clear all parsed data
    pub fn clear(&mut self) {
        self.series.clear();
        self.config.packet_rate.samples.clear();
        self.chunks_processed = 0;
        self.next_color = 0;
    }

    /// Trim data points older than the given timestamp
    ///
    /// Called by DataBuffer when old chunks are truncated to keep graph
    /// data in sync with the buffer's time window.
    pub(crate) fn trim_before(&mut self, cutoff: SystemTime) {
        // Trim series data points
        for series in self.series.values_mut() {
            // Remove points from the front while they're older than cutoff
            while let Some(front) = series.points.front() {
                if front.timestamp < cutoff {
                    series.points.pop_front();
                } else {
                    break;
                }
            }
        }

        // Remove empty series
        self.series.retain(|_, series| !series.points.is_empty());

        // Trim packet rate samples
        let cutoff_nanos = cutoff
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_nanos() as u64;

        while let Some(front) = self.config.packet_rate.samples.front() {
            // Keep samples whose window ends after the cutoff
            // Window end = window_start + window_size
            let window_end_nanos =
                front.window_start_nanos + self.config.packet_rate.window_size.as_nanos() as u64;
            if window_end_nanos < cutoff_nanos {
                self.config.packet_rate.samples.pop_front();
            } else {
                break;
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::parser::{Csv, Json, KeyValue, RawNumbers, Regex};

    fn raw_chunk(data: &str) -> RawChunk {
        RawChunk {
            data: data.as_bytes().to_vec(),
            direction: Direction::Rx,
            timestamp: SystemTime::now(),
        }
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
        engine.process_raw_chunk(&raw_chunk("temp=25.5"));

        assert_eq!(engine.series["temp"].points[0].value, 25.5);
    }

    #[test]
    fn key_value_multiple_pairs() {
        let mut engine = engine(KeyValue);
        engine.process_raw_chunk(&raw_chunk("temp=25.5, humidity=60"));

        assert_eq!(engine.series["temp"].points[0].value, 25.5);
        assert_eq!(engine.series["humidity"].points[0].value, 60.0);
    }

    #[test]
    fn key_value_colon_separator() {
        let mut engine = engine(KeyValue);
        engine.process_raw_chunk(&raw_chunk("temperature: 41.3"));

        assert_eq!(engine.series["temperature"].points[0].value, 41.3);
    }

    #[test]
    fn key_value_colon_separator_multiple() {
        let mut engine = engine(KeyValue);
        engine.process_raw_chunk(&raw_chunk("temperature: 41.3, hum: 13.3, pre:9"));

        assert_eq!(engine.series["temperature"].points[0].value, 41.3);
    }

    #[test]
    fn key_value_negative_number() {
        let mut engine = engine(KeyValue);
        engine.process_raw_chunk(&raw_chunk("offset=-12.5"));

        assert_eq!(engine.series["offset"].points[0].value, -12.5);
    }

    // -------------------------------------------------------------------------
    // CSV Parser Tests
    // -------------------------------------------------------------------------

    #[test]
    fn csv_simple() {
        let mut engine = engine(Csv::default());
        engine.process_raw_chunk(&raw_chunk("1.0,2.0,3.0"));

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
        engine.process_raw_chunk(&raw_chunk("1000,25.5,60"));

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
        engine.process_raw_chunk(&raw_chunk("1.0;2.0;3.0"));

        assert_eq!(engine.series["col0"].points[0].value, 1.0);
        assert_eq!(engine.series["col1"].points[0].value, 2.0);
    }

    // -------------------------------------------------------------------------
    // JSON Parser Tests
    // -------------------------------------------------------------------------

    #[test]
    fn json_simple_object() {
        let mut engine = engine(Json);
        engine.process_raw_chunk(&raw_chunk(r#"{"temp": 25.5, "humidity": 60}"#));

        assert_eq!(engine.series["temp"].points[0].value, 25.5);
        assert_eq!(engine.series["humidity"].points[0].value, 60.0);
    }

    #[test]
    fn json_nested_object() {
        let mut engine = engine(Json);
        engine.process_raw_chunk(&raw_chunk(r#"{"sensor": {"temp": 25.5}}"#));

        assert_eq!(engine.series["sensor.temp"].points[0].value, 25.5);
    }

    #[test]
    fn json_array_of_numbers() {
        let mut engine = engine(Json);
        engine.process_raw_chunk(&raw_chunk(r#"{"values": [1, 2, 3]}"#));

        assert_eq!(engine.series["values.0"].points[0].value, 1.0);
        assert_eq!(engine.series["values.1"].points[0].value, 2.0);
        assert_eq!(engine.series["values.2"].points[0].value, 3.0);
    }

    // -------------------------------------------------------------------------
    // RawNumbers Parser Tests
    // -------------------------------------------------------------------------

    #[test]
    fn raw_numbers_simple() {
        let mut engine = engine(RawNumbers);
        engine.process_raw_chunk(&raw_chunk("Reading: 25.5 degrees"));

        assert_eq!(engine.series["0"].points[0].value, 25.5);
    }

    #[test]
    fn raw_numbers_multiple() {
        let mut engine = engine(RawNumbers);
        engine.process_raw_chunk(&raw_chunk("Values: 10, 20.5, 30"));

        assert_eq!(engine.series["0"].points[0].value, 10.0);
        assert_eq!(engine.series["1"].points[0].value, 20.5);
        assert_eq!(engine.series["2"].points[0].value, 30.0);
    }

    #[test]
    fn raw_numbers_negative() {
        let mut engine = engine(RawNumbers);
        engine.process_raw_chunk(&raw_chunk("Temp: -15.3"));

        assert_eq!(engine.series["0"].points[0].value, -15.3);
    }

    // -------------------------------------------------------------------------
    // Regex Parser Tests
    // -------------------------------------------------------------------------

    #[test]
    fn regex_named_capture() {
        let parser = Regex::new(r"T:(?P<temp>\d+\.?\d*)").unwrap();
        let mut engine = engine(parser);
        engine.process_raw_chunk(&raw_chunk("T:25.5"));

        assert_eq!(engine.series["temp"].points[0].value, 25.5);
    }

    #[test]
    fn regex_multiple_captures() {
        let parser = Regex::new(r"T:(?P<temp>\d+\.?\d*)\s+H:(?P<humidity>\d+\.?\d*)").unwrap();
        let mut engine = engine(parser);
        engine.process_raw_chunk(&raw_chunk("T:25.5 H:60"));

        assert_eq!(engine.series["temp"].points[0].value, 25.5);
        assert_eq!(engine.series["humidity"].points[0].value, 60.0);
    }

    // -------------------------------------------------------------------------
    // Engine Behavior Tests
    // -------------------------------------------------------------------------

    #[test]
    fn engine_multiple_chunks_same_series() {
        let mut engine = engine(KeyValue);
        engine.process_raw_chunk(&raw_chunk("temp=20"));
        engine.process_raw_chunk(&raw_chunk("temp=21"));
        engine.process_raw_chunk(&raw_chunk("temp=22"));

        let series = &engine.series["temp"];
        assert_eq!(series.points.len(), 3);
        assert_eq!(series.points[0].value, 20.0);
        assert_eq!(series.points[1].value, 21.0);
        assert_eq!(series.points[2].value, 22.0);
    }

    #[test]
    fn engine_auto_color_assignment() {
        let mut engine = engine(KeyValue);
        engine.process_raw_chunk(&raw_chunk("temp=25"));
        engine.process_raw_chunk(&raw_chunk("humidity=60"));
        engine.process_raw_chunk(&raw_chunk("pressure=1013"));

        assert_eq!(engine.series["temp"].color, 1);
        assert_eq!(engine.series["humidity"].color, 2);
        assert_eq!(engine.series["pressure"].color, 3);
    }

    #[test]
    fn packet_rate_recording() {
        let mut engine = engine(KeyValue);
        engine.process_raw_chunk(&raw_chunk("temp=25"));
        engine.process_raw_chunk(&raw_chunk("temp=26"));
        engine.process_raw_chunk(&raw_chunk("temp=27"));

        let samples: Vec<_> = engine.config.packet_rate.samples.iter().collect();
        assert!(!samples.is_empty());
        // All chunks processed in same time window (test runs fast)
        assert!(samples[0].rx_count >= 1);
    }
}

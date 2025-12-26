use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, SystemTime},
};

use crate::{DataChunk, Direction, PacketRateSample};

use super::parser::{GraphParser, GraphParserType};

pub enum GraphMode {
    /// Parse serial data and display as points on a graph.
    ParsedData,
    /// Display the incoming/outcoming packet rates over time.
    PacketData,
}

pub struct GraphEngineConfig {
    /// Parse incoming serial data as points to be displayed on a graph.
    parser: Box<dyn GraphParser>,
    /// Keep track of incoming/outcoming packes rates over time (for `PacketData` mode)
    packet_rate: PacketRateData,
}

/// A single data point for graphing
#[derive(Debug, Clone)]
pub struct GraphDataPoint {
    /// When this value was recorded
    pub timestamp: SystemTime,
    /// The numeric value
    pub value: f64,
    /// Which direction the source chunk came from
    pub direction: Direction,
}

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

/// Packet rate tracking data
///
/// Tracks RX and TX packet counts over time windows for rate visualization.
/// This doesn't require any parsing - it just counts chunks.
#[derive(Debug, Clone)]
pub struct PacketRateData {
    /// Time-windowed packet counts
    samples: VecDeque<PacketRateSample>,
    /// Size of each time window
    window_size: Duration,
    /// Maximum number of samples to keep
    max_samples: usize,
}

/// Main entry struct for usage of graph parsing.
/// Allows lazy-initialization.
pub struct GraphEngine {
    /// configuration for *how* data should be parsed.
    config: GraphEngineConfig,

    /// All different graph series and their data
    series: HashMap<String, GraphSeries>,

    chunks_processed: usize,
}

impl GraphEngine {
    pub fn set_parser(&mut self, parser: GraphParserType) {}

    pub fn parse(&self, data: &DataChunk) {}
}

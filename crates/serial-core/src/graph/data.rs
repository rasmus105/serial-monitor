//! Graph data structures
//!
//! Core types for storing and managing graph data.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, SystemTime};

use crate::Direction;

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

impl GraphDataPoint {
    /// Create a new data point
    pub fn new(timestamp: SystemTime, value: f64, direction: Direction) -> Self {
        Self {
            timestamp,
            value,
            direction,
        }
    }
}

/// A named series of data points
#[derive(Debug, Clone)]
pub struct GraphSeries {
    /// Name of the series (e.g., "temperature", "humidity")
    pub name: String,
    /// Data points in chronological order
    pub points: VecDeque<GraphDataPoint>,
    /// Optional color hint (index into a color palette)
    pub color_hint: Option<u8>,
    /// Whether this series is visible in the UI
    pub visible: bool,
}

impl GraphSeries {
    /// Create a new empty series
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            points: VecDeque::new(),
            color_hint: None,
            visible: true,
        }
    }

    /// Create a new series with a color hint
    pub fn with_color(name: impl Into<String>, color_hint: u8) -> Self {
        Self {
            name: name.into(),
            points: VecDeque::new(),
            color_hint: Some(color_hint),
            visible: true,
        }
    }

    /// Add a data point to the series
    pub fn push(&mut self, point: GraphDataPoint) {
        self.points.push_back(point);
    }

    /// Get the number of points in the series
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// Check if the series is empty
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Get the min and max values in the series
    pub fn value_range(&self) -> Option<(f64, f64)> {
        self.points
            .iter()
            .map(|p| p.value)
            .fold(None, |acc, val| match acc {
                Some((min, max)) => Some((min.min(val), max.max(val))),
                None => Some((val, val)),
            })
    }

    /// Get the time range of the series
    pub fn time_range(&self) -> Option<(SystemTime, SystemTime)> {
        let first = self.points.front()?;
        let last = self.points.back()?;
        Some((first.timestamp, last.timestamp))
    }

    /// Get points within a time window
    pub fn points_in_range(
        &self,
        start: SystemTime,
        end: SystemTime,
    ) -> impl Iterator<Item = &GraphDataPoint> {
        self.points
            .iter()
            .filter(move |p| p.timestamp >= start && p.timestamp <= end)
    }

    /// Remove points older than the given timestamp
    pub fn truncate_before(&mut self, cutoff: SystemTime) {
        while let Some(front) = self.points.front() {
            if front.timestamp < cutoff {
                self.points.pop_front();
            } else {
                break;
            }
        }
    }
}

/// A sample of packet rate data at a point in time
#[derive(Debug, Clone)]
pub struct PacketRateSample {
    /// Start of the time window
    pub window_start: SystemTime,
    /// Number of RX packets in this window
    pub rx_count: u32,
    /// Number of TX packets in this window
    pub tx_count: u32,
    /// Total RX bytes in this window
    pub rx_bytes: usize,
    /// Total TX bytes in this window
    pub tx_bytes: usize,
}

impl PacketRateSample {
    /// Create a new empty sample
    pub fn new(window_start: SystemTime) -> Self {
        Self {
            window_start,
            rx_count: 0,
            tx_count: 0,
            rx_bytes: 0,
            tx_bytes: 0,
        }
    }

    /// Record a packet in this sample
    pub fn record(&mut self, direction: Direction, bytes: usize) {
        match direction {
            Direction::Rx => {
                self.rx_count += 1;
                self.rx_bytes += bytes;
            }
            Direction::Tx => {
                self.tx_count += 1;
                self.tx_bytes += bytes;
            }
        }
    }
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

impl PacketRateData {
    /// Default window size: 100ms
    pub const DEFAULT_WINDOW_SIZE: Duration = Duration::from_millis(100);
    /// Default max samples: 10 minutes at 100ms windows = 6000 samples
    pub const DEFAULT_MAX_SAMPLES: usize = 6000;

    /// Create new packet rate data with default settings
    pub fn new() -> Self {
        Self::with_config(Self::DEFAULT_WINDOW_SIZE, Self::DEFAULT_MAX_SAMPLES)
    }

    /// Create with custom window size and max samples
    pub fn with_config(window_size: Duration, max_samples: usize) -> Self {
        Self {
            samples: VecDeque::new(),
            window_size,
            max_samples,
        }
    }

    /// Record a packet
    pub fn record(&mut self, timestamp: SystemTime, direction: Direction, bytes: usize) {
        let window_start = self.window_start_for(timestamp);

        // Check if we need a new sample
        let needs_new_sample = self
            .samples
            .back()
            .map(|s| s.window_start != window_start)
            .unwrap_or(true);

        if needs_new_sample {
            // Trim old samples if at capacity
            while self.samples.len() >= self.max_samples {
                self.samples.pop_front();
            }
            self.samples.push_back(PacketRateSample::new(window_start));
        }

        // Record in the current sample
        if let Some(sample) = self.samples.back_mut() {
            sample.record(direction, bytes);
        }
    }

    /// Get all samples
    pub fn samples(&self) -> impl Iterator<Item = &PacketRateSample> {
        self.samples.iter()
    }

    /// Get samples in a time range
    pub fn samples_in_range(
        &self,
        start: SystemTime,
        end: SystemTime,
    ) -> impl Iterator<Item = &PacketRateSample> {
        self.samples
            .iter()
            .filter(move |s| s.window_start >= start && s.window_start <= end)
    }

    /// Get the window size
    pub fn window_size(&self) -> Duration {
        self.window_size
    }

    /// Get the time range of available data
    pub fn time_range(&self) -> Option<(SystemTime, SystemTime)> {
        let first = self.samples.front()?;
        let last = self.samples.back()?;
        Some((first.window_start, last.window_start))
    }

    /// Calculate the window start time for a given timestamp
    fn window_start_for(&self, timestamp: SystemTime) -> SystemTime {
        // Round down to window boundary
        let since_epoch = timestamp
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO);
        let window_nanos = self.window_size.as_nanos() as u64;
        let rounded_nanos = (since_epoch.as_nanos() as u64 / window_nanos) * window_nanos;
        SystemTime::UNIX_EPOCH + Duration::from_nanos(rounded_nanos)
    }

    /// Clear all data
    pub fn clear(&mut self) {
        self.samples.clear();
    }
}

impl Default for PacketRateData {
    fn default() -> Self {
        Self::new()
    }
}

/// Storage for multiple graph series with size management
#[derive(Debug)]
pub struct GraphBuffer {
    /// Named series of data
    series: HashMap<String, GraphSeries>,
    /// Maximum points per series
    max_points_per_series: usize,
    /// Counter for assigning color hints
    next_color: u8,
}

impl GraphBuffer {
    /// Default max points per series: 10000
    pub const DEFAULT_MAX_POINTS: usize = 10000;

    /// Create a new graph buffer with default settings
    pub fn new() -> Self {
        Self::with_max_points(Self::DEFAULT_MAX_POINTS)
    }

    /// Create with custom max points per series
    pub fn with_max_points(max_points: usize) -> Self {
        Self {
            series: HashMap::new(),
            max_points_per_series: max_points,
            next_color: 0,
        }
    }

    /// Add a data point to a series (creating the series if it doesn't exist)
    pub fn push(&mut self, series_name: &str, point: GraphDataPoint) {
        let series = self
            .series
            .entry(series_name.to_string())
            .or_insert_with(|| {
                let color = self.next_color;
                self.next_color = self.next_color.wrapping_add(1);
                GraphSeries::with_color(series_name, color)
            });

        // Trim if at capacity
        while series.len() >= self.max_points_per_series {
            series.points.pop_front();
        }

        series.push(point);
    }

    /// Get a series by name
    pub fn series(&self, name: &str) -> Option<&GraphSeries> {
        self.series.get(name)
    }

    /// Get a mutable reference to a series
    pub fn series_mut(&mut self, name: &str) -> Option<&mut GraphSeries> {
        self.series.get_mut(name)
    }

    /// Get all series
    pub fn all_series(&self) -> impl Iterator<Item = &GraphSeries> {
        self.series.values()
    }

    /// Get all series names
    pub fn series_names(&self) -> impl Iterator<Item = &str> {
        self.series.keys().map(|s| s.as_str())
    }

    /// Get the number of series
    pub fn series_count(&self) -> usize {
        self.series.len()
    }

    /// Check if there is any data
    pub fn is_empty(&self) -> bool {
        self.series.is_empty() || self.series.values().all(|s| s.is_empty())
    }

    /// Get the combined time range across all series
    pub fn time_range(&self) -> Option<(SystemTime, SystemTime)> {
        let mut min_time: Option<SystemTime> = None;
        let mut max_time: Option<SystemTime> = None;

        for series in self.series.values() {
            if let Some((start, end)) = series.time_range() {
                min_time = Some(min_time.map_or(start, |t| t.min(start)));
                max_time = Some(max_time.map_or(end, |t| t.max(end)));
            }
        }

        min_time.zip(max_time)
    }

    /// Get the combined value range across all visible series
    pub fn value_range(&self) -> Option<(f64, f64)> {
        let mut min_val: Option<f64> = None;
        let mut max_val: Option<f64> = None;

        for series in self.series.values().filter(|s| s.visible) {
            if let Some((min, max)) = series.value_range() {
                min_val = Some(min_val.map_or(min, |v| v.min(min)));
                max_val = Some(max_val.map_or(max, |v| v.max(max)));
            }
        }

        min_val.zip(max_val)
    }

    /// Toggle visibility of a series
    pub fn toggle_visibility(&mut self, name: &str) {
        if let Some(series) = self.series.get_mut(name) {
            series.visible = !series.visible;
        }
    }

    /// Set visibility of a series
    pub fn set_visibility(&mut self, name: &str, visible: bool) {
        if let Some(series) = self.series.get_mut(name) {
            series.visible = visible;
        }
    }

    /// Clear all data
    pub fn clear(&mut self) {
        self.series.clear();
        self.next_color = 0;
    }
}

impl Default for GraphBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graph_series_basic() {
        let mut series = GraphSeries::new("temperature");
        assert!(series.is_empty());

        let now = SystemTime::now();
        series.push(GraphDataPoint::new(now, 25.0, Direction::Rx));
        series.push(GraphDataPoint::new(now, 30.0, Direction::Rx));

        assert_eq!(series.len(), 2);
        assert_eq!(series.value_range(), Some((25.0, 30.0)));
    }

    #[test]
    fn test_graph_buffer_auto_color() {
        let mut buffer = GraphBuffer::new();
        let now = SystemTime::now();

        buffer.push("temp", GraphDataPoint::new(now, 25.0, Direction::Rx));
        buffer.push("humidity", GraphDataPoint::new(now, 60.0, Direction::Rx));
        buffer.push("pressure", GraphDataPoint::new(now, 1013.0, Direction::Rx));

        assert_eq!(buffer.series("temp").unwrap().color_hint, Some(0));
        assert_eq!(buffer.series("humidity").unwrap().color_hint, Some(1));
        assert_eq!(buffer.series("pressure").unwrap().color_hint, Some(2));
    }

    #[test]
    fn test_packet_rate_data() {
        let mut rate_data = PacketRateData::with_config(Duration::from_millis(100), 100);
        let now = SystemTime::now();

        rate_data.record(now, Direction::Rx, 100);
        rate_data.record(now, Direction::Rx, 50);
        rate_data.record(now, Direction::Tx, 25);

        let samples: Vec<_> = rate_data.samples().collect();
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].rx_count, 2);
        assert_eq!(samples[0].tx_count, 1);
        assert_eq!(samples[0].rx_bytes, 150);
        assert_eq!(samples[0].tx_bytes, 25);
    }
}

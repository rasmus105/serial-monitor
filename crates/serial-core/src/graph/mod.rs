//! Graph visualization infrastructure
//!
//! This module provides data structures and parsing utilities for visualizing
//! serial data as graphs. It supports multiple visualization modes:
//!
//! - **Packet rate**: Simple counting of RX/TX packets over time (no parsing needed)
//! - **Parsed data**: Extract numeric values from data using configurable parsers
//!
//! # Architecture
//!
//! The graph system follows the same lazy-initialization pattern as the rest of
//! the core library. The [`GraphEngine`] is created on-demand when the UI first
//! requests graph data, at which point it parses all existing buffered data and
//! then processes new chunks incrementally.
//!
//! # Key Types
//!
//! - [`GraphDataPoint`]: A single parsed value with timestamp and metadata
//! - [`GraphSeries`]: A named series of data points
//! - [`GraphBuffer`]: Storage for graph data with size limits
//! - [`GraphParser`]: Trait for implementing custom parsers
//! - [`GraphEngine`]: Main engine that manages parsing and data storage

mod data;
mod engine;
mod parser;

pub use data::{GraphBuffer, GraphDataPoint, GraphSeries, PacketRateData, PacketRateSample};
pub use engine::{GraphEngine, GraphEngineConfig, GraphMode};
pub use parser::{
    GraphParser, GraphParserConfig, KeyValueParser, ParsedValue, ParserType, RegexParser,
    RegexParserConfig,
};

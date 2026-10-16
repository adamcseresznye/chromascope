// src/lib.rs
//! Chromascope - Mass spectrometry data visualization tool
//!
//! This library provides the core functionality for parsing MzML files,
//! extracting chromatograms, and processing mass spectrometry data.

/// Identification, Untargeted analysis and Statistics are compiled but only exposed
/// to users when built with `--features experimental`.
pub const EXPERIMENTAL: bool = cfg!(feature = "experimental");

pub mod annotation;
pub mod atomic_write;
pub mod chromatography;
pub mod confidence;
pub mod csv_text;
pub mod delivery;
pub mod domain;
pub mod engine;
pub mod error;
pub mod export;
#[cfg(feature = "gui")]
pub mod gui;
pub mod import;
pub mod integration;
pub mod jobs;
#[cfg(feature = "mcp")]
pub mod mcp;
pub mod parser;
pub mod plotting_parameters;
pub mod presets;
pub mod processing;
pub mod project;
pub mod proposals;
pub mod qc;
pub mod quant;
mod report_html;
pub mod sample_sheet;
mod source_validation;
pub mod spectral;
pub mod statistics;
pub mod targeted;
pub mod untargeted;
pub mod validation;

// Re-export commonly used types for convenience
pub use error::{ChromascopeError, Result};
pub use export::export_chromatogram_csv;
pub use parser::MzData;
pub use processing::{process_chromatogram, ProcessingParams};
pub use validation::XicParams;

#[cfg(feature = "mcp-headless")]
pub mod engine_mcp;

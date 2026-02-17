// src/lib.rs
//! Chromascope - Mass spectrometry data visualization tool
//!
//! This library provides the core functionality for parsing MzML files,
//! extracting chromatograms, and processing mass spectrometry data.

pub mod error;
pub mod export;
pub mod gui;
pub mod parser;
pub mod plotting_parameters;
pub mod processing;
pub mod validation;

// Re-export commonly used types for convenience
pub use error::{ChromascopeError, Result};
pub use export::export_chromatogram_csv;
pub use parser::MzData;
pub use processing::{process_chromatogram, ProcessingParams};
pub use validation::XicParams;

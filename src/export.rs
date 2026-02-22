//! Data export functionality for mass spectrometry chromatograms.
//!
//! This module provides exporters for various formats (currently CSV).
//! Export logic is independent of GUI concerns, making it reusable in
//! CLI tools, batch processing, or other contexts.

use crate::error::{ChromascopeError, Result};
use std::path::Path;

/// Exports chromatogram data to CSV format.
///
/// Writes retention time and intensity values to a CSV file with headers.
/// All numeric values are written with full precision.
///
/// # Format
/// ```text
/// Retention Time,Intensity
/// 1.23,456.78
/// 2.34,567.89
/// ...
/// ```
///
/// # Arguments
/// * `data` - Chromatogram data as (retention_time, intensity) pairs
/// * `path` - Destination file path
///
/// # Returns
/// * `Ok(())` - CSV successfully written
/// * `Err(ChromascopeError::IoError)` - File write failed
///
/// # Examples
/// ```no_run
/// use chromascope::export::export_chromatogram_csv;
/// use std::path::Path;
///
/// let data = vec![[1.0, 100.0], [2.0, 200.0]];
/// export_chromatogram_csv(&data, Path::new("output.csv"))?;
/// # Ok::<(), chromascope::error::ChromascopeError>(())
/// ```
///
/// # Errors
/// Returns `IoError` if the file cannot be created or written to.
pub fn export_chromatogram_csv(data: &[[f64; 2]], path: &Path) -> Result<()> {
    // Build CSV content in memory (more efficient than multiple write calls)
    let mut csv_content = String::from("Retention Time,Intensity\n");

    for [retention_time, intensity] in data.iter() {
        csv_content.push_str(&format!("{},{}\n", retention_time, intensity));
    }

    // Write to file in a single operation
    std::fs::write(path, csv_content).map_err(ChromascopeError::IoError)?;

    Ok(())
}

/// Exports a single mass spectrum to CSV format.
///
/// # Format
/// ```text
/// m/z,Intensity
/// 100.123,456.78
/// ```
///
/// # Arguments
/// * `spectrum` - Mass spectrum with equal-length mz and intensity vectors
/// * `path` - Destination file path
///
/// # Errors
/// * `ChromascopeError::NoPlotData` — spectrum has no data points
/// * `ChromascopeError::IoError` — file write failed
pub fn export_spectrum_csv(spectrum: &crate::parser::MassSpectrum, path: &Path) -> Result<()> {
    if spectrum.mz.is_empty() {
        return Err(ChromascopeError::NoPlotData);
    }

    let mut csv_content = String::from("m/z,Intensity\n");
    for (mz, intensity) in spectrum.mz.iter().zip(spectrum.intensity.iter()) {
        csv_content.push_str(&format!("{},{}\n", mz, intensity));
    }

    std::fs::write(path, csv_content).map_err(ChromascopeError::IoError)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_export_csv_creates_file() {
        let test_data = vec![[1.0, 100.0], [2.0, 200.0]];
        let temp_path = std::env::temp_dir().join("test_chromascope_export.csv");

        let result = export_chromatogram_csv(&test_data, &temp_path);

        assert!(result.is_ok());
        assert!(temp_path.exists());

        // Cleanup
        let _ = fs::remove_file(&temp_path);
    }

    #[test]
    fn test_export_csv_correct_format() {
        let test_data = vec![[1.5, 100.5], [2.5, 200.5]];
        let temp_path = std::env::temp_dir().join("test_chromascope_format.csv");

        export_chromatogram_csv(&test_data, &temp_path).unwrap();

        let contents = fs::read_to_string(&temp_path).unwrap();

        // Verify header
        assert!(contents.starts_with("Retention Time,Intensity\n"));

        // Verify data rows
        assert!(contents.contains("1.5,100.5"));
        assert!(contents.contains("2.5,200.5"));

        // Cleanup
        let _ = fs::remove_file(&temp_path);
    }

    #[test]
    fn test_export_empty_data() {
        let test_data: Vec<[f64; 2]> = vec![];
        let temp_path = std::env::temp_dir().join("test_chromascope_empty.csv");

        let result = export_chromatogram_csv(&test_data, &temp_path);

        assert!(result.is_ok());

        let contents = fs::read_to_string(&temp_path).unwrap();
        assert_eq!(contents, "Retention Time,Intensity\n");

        // Cleanup
        let _ = fs::remove_file(&temp_path);
    }

    #[test]
    fn test_export_large_dataset() {
        // Test with realistic chromatogram size
        let test_data: Vec<[f64; 2]> = (0..1000)
            .map(|i| [i as f64 * 0.1, (i as f64).sin() * 1000.0])
            .collect();

        let temp_path = std::env::temp_dir().join("test_chromascope_large.csv");

        let result = export_chromatogram_csv(&test_data, &temp_path);

        assert!(result.is_ok());

        let contents = fs::read_to_string(&temp_path).unwrap();
        let line_count = contents.lines().count();

        // 1 header + 1000 data rows
        assert_eq!(line_count, 1001);

        // Cleanup
        let _ = fs::remove_file(&temp_path);
    }

    #[test]
    fn test_export_to_invalid_path() {
        let test_data = vec![[1.0, 100.0]];
        // Path with invalid characters or nonexistent directory
        let invalid_path = Path::new("/nonexistent_directory_12345/output.csv");

        let result = export_chromatogram_csv(&test_data, invalid_path);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ChromascopeError::IoError(_)));
    }

    #[test]
    fn test_export_spectrum_csv_correct_format() {
        use crate::parser::MassSpectrum;
        let spectrum = MassSpectrum {
            mz: vec![100.0, 200.0],
            intensity: vec![1000.0, 500.0],
            index: 0,
            retention_time: 1.5,
        };
        let path = std::env::temp_dir().join("test_spectrum_format.csv");
        export_spectrum_csv(&spectrum, &path).unwrap();
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.starts_with("m/z,Intensity\n"));
        assert!(contents.contains("100"));
        assert!(contents.contains("1000"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_export_spectrum_csv_empty_returns_no_plot_data() {
        use crate::parser::MassSpectrum;
        let spectrum = MassSpectrum {
            mz: vec![],
            intensity: vec![],
            index: 0,
            retention_time: 0.0,
        };
        let path = std::env::temp_dir().join("test_spectrum_empty.csv");
        let result = export_spectrum_csv(&spectrum, &path);
        assert!(matches!(result.unwrap_err(), ChromascopeError::NoPlotData));
    }

    #[test]
    fn test_export_spectrum_csv_invalid_path() {
        use crate::parser::MassSpectrum;
        let spectrum = MassSpectrum {
            mz: vec![100.0],
            intensity: vec![500.0],
            index: 0,
            retention_time: 1.0,
        };
        let result = export_spectrum_csv(&spectrum, Path::new("/no_such_dir/out.csv"));
        assert!(matches!(result.unwrap_err(), ChromascopeError::IoError(_)));
    }
}

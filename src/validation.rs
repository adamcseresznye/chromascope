//! Input validation for chromatogram extraction parameters.
//!
//! This module provides validated parameter types that enforce business rules
//! at construction time, preventing invalid states from propagating through
//! the application.

use crate::error::{ChromascopeError, Result};
use mzdata::spectrum::ScanPolarity;

/// Represents the valid data ranges for a specific MS data file.
///
/// Extracted during file opening to enable intelligent validation
/// and user feedback about acceptable parameter ranges.
#[derive(Debug, Clone, Copy)]
pub struct DataBounds {
    /// Minimum m/z value in the file (across all spectra)
    pub min_mz: f64,
    /// Maximum m/z value in the file (across all spectra)
    pub max_mz: f64,
    /// Minimum retention time in minutes
    pub min_rt: f32,
    /// Maximum retention time in minutes
    pub max_rt: f32,
    /// Total number of scans in the file
    pub scan_count: usize,
}

impl DataBounds {
    /// Creates new bounds with no restrictions (for files not yet analyzed).
    ///
    /// This is used only for initialization. Never use unrestricted bounds
    /// for actual validation - fail fast instead if no bounds are available.
    pub fn unrestricted() -> Self {
        Self {
            min_mz: 0.0,
            max_mz: f64::MAX,
            min_rt: 0.0,
            max_rt: f32::MAX,
            scan_count: 0,
        }
    }

    /// Validates a mass value is within the file's m/z range.
    ///
    /// # Arguments
    /// * `mass` - The m/z value to validate
    ///
    /// # Returns
    /// * `Ok(())` - If mass is within bounds
    /// * `Err(ChromascopeError::MassOutOfRange)` - If mass is outside the file's range
    pub fn validate_mass(&self, mass: f64) -> Result<()> {
        if mass <= 0.0 {
            return Err(ChromascopeError::InvalidMass(mass));
        }
        if mass < self.min_mz || mass > self.max_mz {
            return Err(ChromascopeError::MassOutOfRange {
                mass,
                min: self.min_mz,
                max: self.max_mz,
            });
        }
        Ok(())
    }

    /// Validates smoothing window is appropriate for data size.
    ///
    /// Enforces that smoothing window should be:
    /// - No more than 10 (absolute maximum)
    /// - No more than 10% of total scan count
    ///
    /// # Arguments
    /// * `window` - The smoothing window size
    ///
    /// # Returns
    /// * `Ok(())` - If window size is appropriate
    /// * `Err(ChromascopeError::SmoothingWindowTooLarge)` - If window is too large
    pub fn validate_smoothing(&self, window: u8) -> Result<()> {
        if window > 10 {
            return Err(ChromascopeError::InvalidSmoothingWindow(window));
        }

        // Window should be smaller than 10% of data points
        let max_reasonable_window = (self.scan_count / 10).max(3) as u8;
        if window > max_reasonable_window {
            return Err(ChromascopeError::SmoothingWindowTooLarge {
                window,
                scan_count: self.scan_count,
                recommended_max: max_reasonable_window,
            });
        }
        Ok(())
    }
}

/// Validated parameters for Extracted Ion Chromatogram (XIC) extraction.
///
/// This type ensures that mass and mass tolerance values are within valid ranges
/// before they can be used for chromatogram extraction. Construction through
/// `new()` is the only way to create an instance, guaranteeing validation.
///
/// # Validation Rules
/// - `mass`: Must be within the file's m/z range
/// - `mass_tolerance`: Must be between 0.0 and 1000.0 ppm
///
/// # Example
/// ```
/// use chromascope::validation::{XicParams, DataBounds};
/// use mzdata::spectrum::ScanPolarity;
///
/// let bounds = DataBounds {
///     min_mz: 100.0,
///     max_mz: 1000.0,
///     min_rt: 0.0,
///     max_rt: 60.0,
///     scan_count: 1000,
/// };
///
/// // Valid parameters
/// let params = XicParams::new(524.3, ScanPolarity::Positive, 10.0, &bounds).unwrap();
///
/// // Invalid mass returns error
/// let invalid = XicParams::new(2000.0, ScanPolarity::Positive, 10.0, &bounds);
/// assert!(invalid.is_err());
/// ```
#[derive(Debug, Clone, Copy)]
pub struct XicParams {
    mass: f64,
    mass_tolerance: f64,
    polarity: ScanPolarity,
}

impl XicParams {
    /// Creates new validated XIC parameters using file-specific bounds.
    ///
    /// # Arguments
    /// * `mass` - Target m/z value
    /// * `polarity` - Ion polarity (Positive, Negative, or Unknown)
    /// * `mass_tolerance` - Mass tolerance in ppm (must be 0-1000)
    /// * `bounds` - Valid data ranges from the opened file
    ///
    /// # Returns
    /// * `Ok(XicParams)` - If all parameters are valid
    /// * `Err(ChromascopeError::MassOutOfRange)` - If mass is outside the file's m/z range
    /// * `Err(ChromascopeError::InvalidMassTolerance)` - If tolerance outside 0-1000 ppm
    ///
    /// # Example
    /// ```
    /// use chromascope::validation::{XicParams, DataBounds};
    /// use mzdata::spectrum::ScanPolarity;
    ///
    /// let bounds = DataBounds::unrestricted();
    /// let params = XicParams::new(524.3, ScanPolarity::Positive, 10.0, &bounds).unwrap();
    /// ```
    pub fn new(
        mass: f64,
        polarity: ScanPolarity,
        mass_tolerance: f64,
        bounds: &DataBounds,
    ) -> Result<Self> {
        // Validate against actual file data
        bounds.validate_mass(mass)?;

        // Validate mass tolerance (still uses hard-coded range)
        if !(0.0..=1000.0).contains(&mass_tolerance) {
            return Err(ChromascopeError::InvalidMassTolerance(mass_tolerance));
        }

        Ok(Self {
            mass,
            mass_tolerance,
            polarity,
        })
    }

    /// Returns the target mass value
    pub fn mass(&self) -> f64 {
        self.mass
    }

    /// Returns the mass tolerance in ppm
    pub fn mass_tolerance(&self) -> f64 {
        self.mass_tolerance
    }

    /// Returns the scan polarity
    pub fn polarity(&self) -> ScanPolarity {
        self.polarity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper function to create mock bounds for testing
    fn mock_bounds() -> DataBounds {
        DataBounds {
            min_mz: 100.0,
            max_mz: 1000.0,
            min_rt: 0.0,
            max_rt: 60.0,
            scan_count: 1000,
        }
    }

    #[test]
    fn test_valid_xic_params() {
        let bounds = mock_bounds();
        let params = XicParams::new(524.3, ScanPolarity::Positive, 10.0, &bounds);
        assert!(params.is_ok(), "Valid parameters should succeed");

        let params = params.unwrap();
        assert_eq!(params.mass(), 524.3);
        assert_eq!(params.mass_tolerance(), 10.0);
        assert_eq!(params.polarity(), ScanPolarity::Positive);
    }

    #[test]
    fn test_zero_mass_invalid() {
        let bounds = mock_bounds();
        let result = XicParams::new(0.0, ScanPolarity::Positive, 10.0, &bounds);
        assert!(result.is_err(), "Zero mass should fail");
        assert!(
            matches!(result.unwrap_err(), ChromascopeError::InvalidMass(_)),
            "Should return InvalidMass error"
        );
    }

    #[test]
    fn test_negative_mass_invalid() {
        let bounds = mock_bounds();
        let result = XicParams::new(-100.0, ScanPolarity::Negative, 5.0, &bounds);
        assert!(result.is_err(), "Negative mass should fail");
        assert!(
            matches!(result.unwrap_err(), ChromascopeError::InvalidMass(_)),
            "Should return InvalidMass error"
        );
    }

    #[test]
    fn test_negative_tolerance_invalid() {
        let bounds = mock_bounds();
        let result = XicParams::new(500.0, ScanPolarity::Positive, -1.0, &bounds);
        assert!(result.is_err(), "Negative tolerance should fail");
        assert!(
            matches!(
                result.unwrap_err(),
                ChromascopeError::InvalidMassTolerance(_)
            ),
            "Should return InvalidMassTolerance error"
        );
    }

    #[test]
    fn test_excessive_tolerance_invalid() {
        let bounds = mock_bounds();
        let result = XicParams::new(500.0, ScanPolarity::Positive, 1001.0, &bounds);
        assert!(result.is_err(), "Tolerance > 1000 ppm should fail");
        assert!(
            matches!(
                result.unwrap_err(),
                ChromascopeError::InvalidMassTolerance(_)
            ),
            "Should return InvalidMassTolerance error"
        );
    }

    #[test]
    fn test_boundary_tolerances() {
        let bounds = mock_bounds();

        // Test lower boundary (0.0 ppm)
        let min_tol = XicParams::new(500.0, ScanPolarity::Positive, 0.0, &bounds);
        assert!(min_tol.is_ok(), "0.0 ppm tolerance should be valid");

        // Test upper boundary (1000.0 ppm)
        let max_tol = XicParams::new(500.0, ScanPolarity::Positive, 1000.0, &bounds);
        assert!(max_tol.is_ok(), "1000.0 ppm tolerance should be valid");
    }

    #[test]
    fn test_various_polarities() {
        let bounds = mock_bounds();
        assert!(XicParams::new(500.0, ScanPolarity::Positive, 10.0, &bounds).is_ok());
        assert!(XicParams::new(500.0, ScanPolarity::Negative, 10.0, &bounds).is_ok());
        assert!(XicParams::new(500.0, ScanPolarity::Unknown, 10.0, &bounds).is_ok());
    }

    #[test]
    fn test_mass_validation_with_bounds() {
        let bounds = mock_bounds();

        // Valid mass within bounds
        assert!(bounds.validate_mass(500.0).is_ok());

        // Mass too low
        let result = bounds.validate_mass(50.0);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ChromascopeError::MassOutOfRange { .. }
        ));

        // Mass too high
        let result = bounds.validate_mass(2000.0);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ChromascopeError::MassOutOfRange { .. }
        ));
    }

    #[test]
    fn test_mass_exactly_at_boundaries() {
        let bounds = mock_bounds();

        // Exactly at min should be OK
        assert!(bounds.validate_mass(100.0).is_ok());

        // Exactly at max should be OK
        assert!(bounds.validate_mass(1000.0).is_ok());

        // Just below min should fail
        assert!(bounds.validate_mass(99.999).is_err());

        // Just above max should fail
        assert!(bounds.validate_mass(1000.001).is_err());
    }

    #[test]
    fn test_smoothing_relative_to_scan_count() {
        let small_file = DataBounds {
            min_mz: 100.0,
            max_mz: 1000.0,
            min_rt: 0.0,
            max_rt: 60.0,
            scan_count: 50,
        };

        // Window of 5 = 10% of 50 scans, should be OK
        assert!(small_file.validate_smoothing(5).is_ok());

        // Window of 6 = >10% of 50 scans, should fail
        assert!(small_file.validate_smoothing(6).is_err());

        // Window of 2 should be OK
        assert!(small_file.validate_smoothing(2).is_ok());
    }

    #[test]
    fn test_smoothing_absolute_max() {
        let large_file = DataBounds {
            min_mz: 100.0,
            max_mz: 1000.0,
            min_rt: 0.0,
            max_rt: 60.0,
            scan_count: 10000,
        };

        // Window of 10 should be OK
        assert!(large_file.validate_smoothing(10).is_ok());

        // Window of 11 should fail (exceeds absolute max)
        let result = large_file.validate_smoothing(11);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ChromascopeError::InvalidSmoothingWindow(_)
        ));
    }

    #[test]
    fn test_mass_out_of_range_includes_file_bounds() {
        let bounds = mock_bounds();
        let result = XicParams::new(2000.0, ScanPolarity::Positive, 10.0, &bounds);

        assert!(result.is_err());
        match result.unwrap_err() {
            ChromascopeError::MassOutOfRange { mass, min, max } => {
                assert_eq!(mass, 2000.0);
                assert_eq!(min, 100.0);
                assert_eq!(max, 1000.0);
            }
            _ => panic!("Expected MassOutOfRange error"),
        }
    }
}

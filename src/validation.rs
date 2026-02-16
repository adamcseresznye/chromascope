//! Input validation for chromatogram extraction parameters.
//!
//! This module provides validated parameter types that enforce business rules
//! at construction time, preventing invalid states from propagating through
//! the application.

use crate::error::{ChromascopeError, Result};
use mzdata::spectrum::ScanPolarity;

/// Validated parameters for Extracted Ion Chromatogram (XIC) extraction.
///
/// This type ensures that mass and mass tolerance values are within valid ranges
/// before they can be used for chromatogram extraction. Construction through
/// `new()` is the only way to create an instance, guaranteeing validation.
///
/// # Validation Rules
/// - `mass`: Must be positive (> 0.0)
/// - `mass_tolerance`: Must be between 0.0 and 1000.0 ppm
///
/// # Example
/// ```
/// use chromascope::validation::XicParams;
/// use mzdata::spectrum::ScanPolarity;
///
/// // Valid parameters
/// let params = XicParams::new(524.3, ScanPolarity::Positive, 10.0)?;
///
/// // Invalid mass returns error
/// let invalid = XicParams::new(-100.0, ScanPolarity::Positive, 10.0);
/// assert!(invalid.is_err());
/// ```
#[derive(Debug, Clone, Copy)]
pub struct XicParams {
    mass: f64,
    mass_tolerance: f64,
    polarity: ScanPolarity,
}

impl XicParams {
    /// Creates new validated XIC parameters.
    ///
    /// # Arguments
    /// * `mass` - Target m/z value (must be positive)
    /// * `polarity` - Ion polarity (Positive, Negative, or Unknown)
    /// * `mass_tolerance` - Mass tolerance in ppm (must be 0-1000)
    ///
    /// # Returns
    /// * `Ok(XicParams)` - If all parameters are valid
    /// * `Err(ChromascopeError::InvalidMass)` - If mass <= 0
    /// * `Err(ChromascopeError::InvalidMassTolerance)` - If tolerance outside 0-1000 ppm
    ///
    /// # Example
    /// ```
    /// let params = XicParams::new(524.3, ScanPolarity::Positive, 10.0)?;
    /// ```
    pub fn new(mass: f64, polarity: ScanPolarity, mass_tolerance: f64) -> Result<Self> {
        // Validate mass value
        if mass <= 0.0 {
            return Err(ChromascopeError::InvalidMass(mass));
        }

        // Validate mass tolerance
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

    #[test]
    fn test_valid_xic_params() {
        let params = XicParams::new(524.3, ScanPolarity::Positive, 10.0);
        assert!(params.is_ok(), "Valid parameters should succeed");

        let params = params.unwrap();
        assert_eq!(params.mass(), 524.3);
        assert_eq!(params.mass_tolerance(), 10.0);
        assert_eq!(params.polarity(), ScanPolarity::Positive);
    }

    #[test]
    fn test_zero_mass_invalid() {
        let result = XicParams::new(0.0, ScanPolarity::Positive, 10.0);
        assert!(result.is_err(), "Zero mass should fail");
        assert!(
            matches!(result.unwrap_err(), ChromascopeError::InvalidMass(0.0)),
            "Should return InvalidMass error"
        );
    }

    #[test]
    fn test_negative_mass_invalid() {
        let result = XicParams::new(-100.0, ScanPolarity::Negative, 5.0);
        assert!(result.is_err(), "Negative mass should fail");
        assert!(
            matches!(result.unwrap_err(), ChromascopeError::InvalidMass(_)),
            "Should return InvalidMass error"
        );
    }

    #[test]
    fn test_negative_tolerance_invalid() {
        let result = XicParams::new(500.0, ScanPolarity::Positive, -1.0);
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
        let result = XicParams::new(500.0, ScanPolarity::Positive, 1001.0);
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
        // Test lower boundary (0.0 ppm)
        let min_tol = XicParams::new(500.0, ScanPolarity::Positive, 0.0);
        assert!(min_tol.is_ok(), "0.0 ppm tolerance should be valid");

        // Test upper boundary (1000.0 ppm)
        let max_tol = XicParams::new(500.0, ScanPolarity::Positive, 1000.0);
        assert!(max_tol.is_ok(), "1000.0 ppm tolerance should be valid");
    }

    #[test]
    fn test_various_polarities() {
        assert!(XicParams::new(500.0, ScanPolarity::Positive, 10.0).is_ok());
        assert!(XicParams::new(500.0, ScanPolarity::Negative, 10.0).is_ok());
        assert!(XicParams::new(500.0, ScanPolarity::Unknown, 10.0).is_ok());
    }
}

//! How masses are shown on screen: one user-chosen number of m/z decimals, kept for the
//! session in an atomic and saved next to the other settings so it survives a restart.
//! Exports and stored data always keep full precision.

use crate::validation::MassTolerance;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};

/// Choices offered in the View menu.
pub(super) const MZ_DECIMAL_CHOICES: [u8; 5] = [2, 3, 4, 5, 6];
const DEFAULT_MZ_DECIMALS: u8 = 4;

static MZ_DECIMALS: AtomicU8 = AtomicU8::new(DEFAULT_MZ_DECIMALS);

fn clamp(decimals: u8) -> u8 {
    decimals.clamp(
        MZ_DECIMAL_CHOICES[0],
        MZ_DECIMAL_CHOICES[MZ_DECIMAL_CHOICES.len() - 1],
    )
}

fn path() -> Option<PathBuf> {
    crate::import::settings_path().map(|p| p.with_file_name("display-settings.json"))
}

/// Decimals currently used for every on-screen m/z.
pub(super) fn mz_decimals() -> u8 {
    MZ_DECIMALS.load(Ordering::Relaxed)
}

/// Format an m/z with the chosen number of decimals.
pub(super) fn fmt_mz(mz: f64) -> String {
    fmt_mz_with(mz, mz_decimals())
}

pub(super) fn fmt_mz_with(mz: f64, decimals: u8) -> String {
    format!("{mz:.*}", usize::from(decimals))
}

/// "10 ppm" or "5 mDa".
pub(super) fn fmt_tolerance(tolerance: &MassTolerance) -> String {
    format!("{} {}", tolerance.value, tolerance.unit.label())
}

/// Change the decimals for this session and remember the choice.
pub(super) fn set_mz_decimals(decimals: u8) {
    MZ_DECIMALS.store(clamp(decimals), Ordering::Relaxed);
    if let Some(path) = path() {
        let saved = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| {
                std::fs::write(
                    &path,
                    serde_json::json!({ "mz_decimals": mz_decimals() }).to_string(),
                )
            });
        if let Err(e) = saved {
            log::warn!("Cannot save display settings: {e}");
        }
    }
}

/// Read the saved choice once at startup; a missing or unreadable file keeps the default.
pub(super) fn load() {
    let saved = path()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|v| v["mz_decimals"].as_u64())
        .and_then(|n| u8::try_from(n).ok());
    if let Some(decimals) = saved {
        MZ_DECIMALS.store(clamp(decimals), Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimals_are_clamped_to_the_offered_range() {
        assert_eq!(clamp(0), 2);
        assert_eq!(clamp(4), 4);
        assert_eq!(clamp(9), 6);
    }

    #[test]
    fn mz_follows_the_requested_decimals() {
        assert_eq!(fmt_mz_with(483.123456789, 2), "483.12");
        assert_eq!(fmt_mz_with(483.123456789, 6), "483.123457");
    }

    #[test]
    fn tolerance_text_names_its_unit() {
        assert_eq!(fmt_tolerance(&MassTolerance::ppm(10.0)), "10 ppm");
        assert_eq!(fmt_tolerance(&MassTolerance::mda(2.5)), "2.5 mDa");
    }
}

//! User preferences kept on this computer in one `settings.json` next to the other app files:
//! theme, interface size and how many decimals an on-screen m/z shows. Exports and stored
//! data always keep full precision. A missing, partial or unreadable file means defaults.

use crate::validation::MassTolerance;
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};

/// Decimals offered in the View menu.
pub(super) const MZ_DECIMAL_CHOICES: [u8; 5] = [2, 3, 4, 5, 6];
const DEFAULT_MZ_DECIMALS: u8 = 4;
const ZOOM_RANGE: std::ops::RangeInclusive<f32> = 0.9..=1.3;

static MZ_DECIMALS: AtomicU8 = AtomicU8::new(DEFAULT_MZ_DECIMALS);

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub(super) enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

impl From<ThemeChoice> for egui::ThemePreference {
    fn from(value: ThemeChoice) -> Self {
        match value {
            ThemeChoice::System => egui::ThemePreference::System,
            ThemeChoice::Light => egui::ThemePreference::Light,
            ThemeChoice::Dark => egui::ThemePreference::Dark,
        }
    }
}

impl From<egui::ThemePreference> for ThemeChoice {
    fn from(value: egui::ThemePreference) -> Self {
        match value {
            egui::ThemePreference::System => ThemeChoice::System,
            egui::ThemePreference::Light => ThemeChoice::Light,
            egui::ThemePreference::Dark => ThemeChoice::Dark,
        }
    }
}

/// The saved preferences. Every field is optional in the file.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(default)]
pub(super) struct Settings {
    pub mz_decimals: u8,
    pub theme: ThemeChoice,
    pub zoom: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mz_decimals: DEFAULT_MZ_DECIMALS,
            theme: ThemeChoice::System,
            zoom: 1.0,
        }
    }
}

impl Settings {
    /// Pull out-of-range values back to something the interface offers.
    fn sanitised(mut self) -> Self {
        self.mz_decimals = self.mz_decimals.clamp(
            MZ_DECIMAL_CHOICES[0],
            MZ_DECIMAL_CHOICES[MZ_DECIMAL_CHOICES.len() - 1],
        );
        self.zoom = if self.zoom.is_finite() {
            self.zoom.clamp(*ZOOM_RANGE.start(), *ZOOM_RANGE.end())
        } else {
            1.0
        };
        self
    }
}

fn parse(bytes: &[u8]) -> Settings {
    serde_json::from_slice::<Settings>(bytes)
        .unwrap_or_default()
        .sanitised()
}

fn path() -> Option<PathBuf> {
    crate::import::settings_path().map(|p| p.with_file_name("settings.json"))
}

/// Read the saved preferences once at startup and make the decimals current.
pub(super) fn load() -> Settings {
    let settings = path()
        .and_then(|p| std::fs::read(p).ok())
        .map(|bytes| parse(&bytes))
        .unwrap_or_default();
    MZ_DECIMALS.store(settings.mz_decimals, Ordering::Relaxed);
    settings
}

/// Write the current theme, interface size and decimals.
pub(super) fn save(ctx: &egui::Context) {
    let settings = Settings {
        mz_decimals: mz_decimals(),
        theme: ctx.options(|o| o.theme_preference).into(),
        zoom: ctx.zoom_factor(),
    };
    let Some(path) = path() else { return };
    let saved = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| {
            crate::atomic_write::write_atomic(
                &path,
                serde_json::to_vec_pretty(&settings).map_err(std::io::Error::other)?,
            )
        });
    if let Err(e) = saved {
        log::warn!("Cannot save settings: {e}");
    }
}

/// Decimals currently used for every on-screen m/z.
pub(super) fn mz_decimals() -> u8 {
    MZ_DECIMALS.load(Ordering::Relaxed)
}

/// Change the decimals for this session and remember the choice.
pub(super) fn set_mz_decimals(ctx: &egui::Context, decimals: u8) {
    MZ_DECIMALS.store(
        Settings {
            mz_decimals: decimals,
            ..Settings::default()
        }
        .sanitised()
        .mz_decimals,
        Ordering::Relaxed,
    );
    save(ctx);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_or_broken_file_gives_defaults() {
        assert_eq!(parse(b""), Settings::default());
        assert_eq!(parse(b"not json"), Settings::default());
        assert_eq!(parse(b"{}"), Settings::default());
    }

    #[test]
    fn a_partial_file_keeps_the_other_defaults() {
        let s = parse(br#"{"theme":"dark"}"#);
        assert_eq!(s.theme, ThemeChoice::Dark);
        assert_eq!(s.mz_decimals, 4);
        assert_eq!(s.zoom, 1.0);
    }

    #[test]
    fn out_of_range_values_are_pulled_into_the_offered_choices() {
        let s = parse(br#"{"mz_decimals":9,"zoom":5.0}"#);
        assert_eq!(s.mz_decimals, 6);
        assert_eq!(s.zoom, 1.3);
        assert_eq!(parse(br#"{"mz_decimals":0}"#).mz_decimals, 2);
    }

    #[test]
    fn settings_round_trip_through_json() {
        let s = Settings {
            mz_decimals: 3,
            theme: ThemeChoice::Light,
            zoom: 1.15,
        };
        assert_eq!(parse(&serde_json::to_vec(&s).unwrap()), s);
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

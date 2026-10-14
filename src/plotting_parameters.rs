//! # contains enums for defining line properties
//!
//! This module defines various enums and their associated methods for representing line properties in a graphical context. It includes definitions for line colors, line types, and plot types, which can be used in conjunction with the `egui` and `egui_plot` libraries for rendering graphics.
//!
//! ## Enums
//!
//! ### `LineColor`
//!
//! An enumeration representing the color of a line. The available colors are:
//!
//! - `Red`
//! - `Green`
//! - `Blue`
//! - `Black`
//! - `Yellow`
//! - `White`
//!
//! The `LineColor` enum derives the `PartialEq` and `Default` traits, allowing for comparison and default instantiation (defaulting to `Red`).
//!
//!
//! ### `PlotType`
//!
//! An enumeration representing different types of plots. The available plot types are:
//!
//! - `Xic`
//! - `Bpc`
//! - `Tic` (default)
//!
//! The `PlotType` enum derives the `PartialEq`, `Debug`, and `Default` traits, allowing for comparison, debugging output, and default instantiation.
//!
//! ## Usage
//!
//! This module can be used to define and manipulate line properties in graphical applications, allowing for customizable visual representations of data. The enums can be easily converted to types compatible with the `egui` and `egui_plot` libraries for rendering.

#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(serde::Serialize, serde::Deserialize, PartialEq, Default, Clone, Copy, Debug)]
pub enum LineColor {
    #[default]
    Red,
    Green,
    Blue,
    Yellow,
    White,
    Gray,
    Cyan,
    Orange,
    Magenta,
    Gold,
}

impl LineColor {
    /// Colour tuned for the plot surface of the active theme, so no choice is
    /// ever invisible: `White` becomes dark ink on light backgrounds, and the
    /// light hues (yellow, cyan, gold) darken on light backgrounds.
    #[cfg(feature = "gui")]
    pub fn to_egui_for(&self, dark: bool) -> egui::ecolor::Color32 {
        use egui::ecolor::Color32 as C;
        match (self, dark) {
            (Self::Red, _) => C::from_rgb(202, 75, 75),
            (Self::Green, _) => C::from_rgb(45, 150, 110),
            (Self::Blue, _) => C::from_rgb(55, 135, 205),
            (Self::Orange, _) => C::from_rgb(201, 125, 58),
            (Self::Magenta, _) => C::from_rgb(162, 103, 195),
            (Self::Yellow, true) => C::YELLOW,
            (Self::Yellow, false) => C::from_rgb(163, 124, 0),
            (Self::White, true) => C::from_rgb(240, 243, 247),
            (Self::White, false) => C::from_rgb(28, 37, 49),
            (Self::Gray, true) => C::from_rgb(170, 178, 190),
            (Self::Gray, false) => C::from_rgb(100, 110, 125),
            (Self::Cyan, true) => C::CYAN,
            (Self::Cyan, false) => C::from_rgb(0, 128, 150),
            (Self::Gold, true) => C::GOLD,
            (Self::Gold, false) => C::from_rgb(145, 105, 0),
        }
    }

    /// Name shown in pickers; `White` reads as black on light themes.
    pub fn label(&self, dark: bool) -> &'static str {
        match (self, dark) {
            (Self::White, true) => "White",
            (Self::White, false) => "Black",
            (Self::Red, _) => "Red",
            (Self::Green, _) => "Green",
            (Self::Blue, _) => "Blue",
            (Self::Yellow, _) => "Yellow",
            (Self::Gray, _) => "Gray",
            (Self::Cyan, _) => "Cyan",
            (Self::Orange, _) => "Orange",
            (Self::Magenta, _) => "Magenta",
            (Self::Gold, _) => "Gold",
        }
    }
}

#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug, Default, Clone, Copy)]
pub enum PlotType {
    Xic,
    Bpc,
    #[default]
    Tic,
}

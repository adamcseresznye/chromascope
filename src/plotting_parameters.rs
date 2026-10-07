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
    pub fn to_egui(&self) -> egui::ecolor::Color32 {
        match self {
            Self::Red => egui::ecolor::Color32::from_rgb(202, 75, 75),
            Self::Green => egui::ecolor::Color32::from_rgb(45, 150, 110),
            Self::Blue => egui::ecolor::Color32::from_rgb(55, 135, 205),
            Self::Yellow => egui::ecolor::Color32::YELLOW,
            Self::White => egui::ecolor::Color32::WHITE,
            Self::Gray => egui::ecolor::Color32::GRAY,
            Self::Cyan => egui::ecolor::Color32::CYAN,
            Self::Orange => egui::ecolor::Color32::from_rgb(201, 125, 58),
            Self::Magenta => egui::ecolor::Color32::from_rgb(162, 103, 195),
            Self::Gold => egui::ecolor::Color32::GOLD,
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug, Default, Clone, Copy)]
pub enum PlotType {
    Xic,
    Bpc,
    #[default]
    Tic,
}

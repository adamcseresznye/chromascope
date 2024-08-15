//! # Line Properties Module
//!
//! This module defines various enumerations and their associated methods for representing line properties in a graphical context. It includes definitions for line colors, line types, and plot types, which can be used in conjunction with the `egui` and `egui_plot` libraries for rendering graphics.
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
//! ### `LineType`
//!
//! An enumeration representing the type of line. The available line types are:
//!
//! - `Solid`
//! - `Dotted`
//! - `Dashed`
//!
//! The `LineType` enum also derives the `PartialEq` and `Default` traits, with the default variant being `Solid`.
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
//! ## Constants
//!
//! - `DASHED_LINE_LENGTH`: A constant defining the length of dashed lines, set to `10.0`.
//! - `DOTTED_LINE_SPACING`: A constant defining the spacing between dotted lines, set to `5.0`.
//!
//! ## Usage
//!
//! This module can be used to define and manipulate line properties in graphical applications, allowing for customizable visual representations of data. The enums can be easily converted to types compatible with the `egui` and `egui_plot` libraries for rendering.

#[derive(PartialEq, Default)]
pub enum LineColor {
    #[default]
    Red,
    Green,
    Blue,
    Black,
    Yellow,
    White,
}

impl LineColor {
    pub fn to_egui(&self) -> egui::ecolor::Color32 {
        match self {
            Self::Red => egui::ecolor::Color32::RED,
            Self::Green => egui::ecolor::Color32::GREEN,
            Self::Blue => egui::ecolor::Color32::BLUE,
            Self::Black => egui::ecolor::Color32::BLACK,
            Self::Yellow => egui::ecolor::Color32::YELLOW,
            Self::White => egui::ecolor::Color32::WHITE,
        }
    }
}

const DASHED_LINE_LENGTH: f32 = 10.0;
const DOTTED_LINE_SPACING: f32 = 5.0;

#[derive(PartialEq, Default)]
pub enum LineType {
    #[default]
    Solid,
    Dotted,
    Dashed,
}

impl LineType {
    pub fn to_egui(&self) -> egui_plot::LineStyle {
        match self {
            Self::Solid => egui_plot::LineStyle::Solid,
            Self::Dashed => egui_plot::LineStyle::Dashed {
                length: DASHED_LINE_LENGTH,
            },
            Self::Dotted => egui_plot::LineStyle::Dotted {
                spacing: DOTTED_LINE_SPACING,
            },
        }
    }
}

#[derive(PartialEq, Debug, Default)]
pub enum PlotType {
    Xic,
    Bpc,
    #[default]
    Tic,
}

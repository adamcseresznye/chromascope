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

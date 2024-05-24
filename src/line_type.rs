#[derive(PartialEq)]
pub enum LineType {
    Solid,
    Dotted,
    Dashed,
}

impl LineType {
    pub fn to_egui(&self) -> egui_plot::LineStyle {
        match self {
            Self::Solid => egui_plot::LineStyle::Solid,
            Self::Dashed => egui_plot::LineStyle::Dashed { length: 10.0 },
            Self::Dotted => egui_plot::LineStyle::Dotted { spacing: 5.0 },
        }
    }
}

impl Default for LineType {
    fn default() -> Self {
        LineType::Solid
    }
}

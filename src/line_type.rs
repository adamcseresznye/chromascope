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

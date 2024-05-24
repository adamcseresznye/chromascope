#[derive(PartialEq)]
pub enum LineColor {
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

impl Default for LineColor {
    fn default() -> Self {
        LineColor::Red
    }
}

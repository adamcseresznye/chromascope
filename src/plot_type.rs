#[derive(PartialEq, Debug)]
pub enum PlotType {
    XIC,
    BPC,
    TIC,
}

impl Default for PlotType {
    fn default() -> Self {
        PlotType::TIC
    }
}

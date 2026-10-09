//! Shared result tables: full-model filtering/sorting and viewport-only rendering.
use eframe::egui;
use std::cmp::Ordering;
#[derive(Clone)]
pub(super) struct Cell {
    pub text: String,
    pub number: Option<f64>,
}
impl Cell {
    pub fn text(value: impl ToString) -> Self {
        Self {
            text: value.to_string(),
            number: None,
        }
    }
    pub fn number(value: Option<f64>) -> Self {
        Self {
            text: value
                .map(|v| format!("{v:.6e}"))
                .unwrap_or_else(|| "Unavailable".into()),
            number: value,
        }
    }
}
#[derive(Clone)]
pub(super) struct Row {
    pub key: String,
    pub cells: Vec<Cell>,
}
#[derive(Clone, Default)]
struct State {
    filter: String,
    column: usize,
    descending: bool,
    widths: Vec<f32>,
    signature: Option<u64>,
    cached_order: Vec<usize>,
}
fn order(rows: &[Row], state: &State) -> Vec<usize> {
    let filter = state.filter.to_lowercase();
    let mut indices: Vec<_> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| {
            row.cells
                .iter()
                .any(|c| c.text.to_lowercase().contains(&filter))
        })
        .map(|(i, _)| i)
        .collect();
    indices.sort_by(|&a, &b| {
        let cmp = match (
            rows[a].cells.get(state.column),
            rows[b].cells.get(state.column),
        ) {
            (Some(a), Some(b)) => match (a.number, b.number) {
                (Some(a), Some(b)) => a.total_cmp(&b),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                _ => a.text.cmp(&b.text),
            },
            _ => Ordering::Equal,
        };
        if state.descending {
            cmp.reverse()
        } else {
            cmp
        }
    });
    indices
}
/// Returns original model index; sort/filter never changes the model or selection.
pub(super) fn show(
    ui: &mut egui::Ui,
    id: &str,
    headers: &[&str],
    rows: &[Row],
    selected: Option<&str>,
) -> Option<usize> {
    let key = egui::Id::new(("analytical_table", id));
    let mut state = ui
        .ctx()
        .data(|d| d.get_temp::<State>(key))
        .unwrap_or_default();
    state.widths.resize(headers.len(), 150.);
    ui.push_id(id, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label("Filter rows");
            ui.add(egui::TextEdit::singleline(&mut state.filter).desired_width(200.));
            if ui.button("Clear filter").clicked() {
                state.filter.clear();
            }
            ui.small(format!("{} total rows", rows.len()));
        });
    });
    use std::hash::{Hash, Hasher};
    let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
    state.filter.hash(&mut fingerprint);
    state.column.hash(&mut fingerprint);
    state.descending.hash(&mut fingerprint);
    for row in rows {
        row.key.hash(&mut fingerprint);
        for cell in &row.cells {
            cell.text.hash(&mut fingerprint);
            cell.number.map(f64::to_bits).hash(&mut fingerprint);
        }
    }
    let signature = fingerprint.finish();
    if state.signature != Some(signature) {
        state.cached_order = order(rows, &state);
        state.signature = Some(signature);
    }
    let indices = &state.cached_order;
    let mut clicked = None;
    egui::ScrollArea::horizontal()
        .id_salt((id, "horizontal"))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                for (i, header) in headers.iter().enumerate() {
                    ui.vertical(|ui| {
                        if ui
                            .add_sized(
                                [state.widths[i], 28.],
                                egui::Button::new(format!(
                                    "{header}{}",
                                    if state.column == i {
                                        if state.descending {
                                            " ↓"
                                        } else {
                                            " ↑"
                                        }
                                    } else {
                                        ""
                                    }
                                )),
                            )
                            .clicked()
                        {
                            if state.column == i {
                                state.descending = !state.descending;
                            } else {
                                state.column = i;
                                state.descending = false;
                            }
                        }
                        ui.add(
                            egui::DragValue::new(&mut state.widths[i])
                                .range(70.0..=600.0)
                                .prefix("Width ")
                                .suffix(" px"),
                        )
                        .on_hover_text("Resize this column");
                    });
                }
            });
            egui::ScrollArea::vertical()
                .id_salt((id, "rows"))
                .max_height(280.)
                .show_rows(ui, 28., indices.len(), |ui, range| {
                    for visible in range {
                        let index = indices[visible];
                        let row = &rows[index];
                        ui.push_id(&row.key, |ui| {
                            ui.horizontal(|ui| {
                                for (column, cell) in row.cells.iter().enumerate() {
                                    let width = state.widths.get(column).copied().unwrap_or(150.);
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(width, 28.),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            let button = if cell.number.is_some() {
                                                egui::Button::new("").right_text(
                                                    egui::RichText::new(&cell.text).monospace(),
                                                )
                                            } else {
                                                egui::Button::new(&cell.text)
                                            };
                                            let response = ui.add_sized(
                                                [width, 28.],
                                                button
                                                    .selected(selected == Some(row.key.as_str()))
                                                    .truncate(),
                                            );
                                            let response = response.on_hover_text(
                                                cell.number
                                                    .map(|v| v.to_string())
                                                    .unwrap_or_else(|| cell.text.clone()),
                                            );
                                            if response.clicked() {
                                                clicked = Some(index);
                                                ui.ctx().data_mut(|d| {
                                                    d.insert_temp(
                                                        egui::Id::new(("table_column", id)),
                                                        column,
                                                    )
                                                });
                                            }
                                        },
                                    );
                                }
                            });
                        });
                    }
                });
        });
    ui.small(format!(
        "{} matching rows · select any cell to inspect linked evidence",
        indices.len()
    ));
    ui.ctx().data_mut(|d| d.insert_temp(key, state));
    clicked
}
pub(super) fn selected_column(ui: &egui::Ui, id: &str) -> usize {
    ui.ctx()
        .data(|d| d.get_temp::<usize>(egui::Id::new(("table_column", id))))
        .unwrap_or(0)
}
pub(super) fn records(
    ui: &mut egui::Ui,
    id: &str,
    records: serde_json::Value,
    identity: &str,
    selected: Option<&str>,
) -> Option<usize> {
    let records = records.as_array()?;
    let headers: Vec<_> = records
        .first()?
        .as_object()?
        .keys()
        .map(String::as_str)
        .collect();
    let rows: Vec<_> = records
        .iter()
        .enumerate()
        .map(|(index, row)| Row {
            key: row[identity]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| index.to_string()),
            cells: headers
                .iter()
                .map(|key| {
                    let value = &row[*key];
                    if let Some(number) = value.as_f64() {
                        Cell::number(Some(number))
                    } else if value.is_null() {
                        Cell::text("Unavailable")
                    } else if let Some(text) = value.as_str() {
                        Cell::text(text)
                    } else {
                        Cell::text(value)
                    }
                })
                .collect(),
        })
        .collect();
    show(ui, id, &headers, &rows, selected)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numeric_sort_filter_retains_model_identity() {
        let rows = vec![
            Row {
                key: "ten".into(),
                cells: vec![Cell::number(Some(10.))],
            },
            Row {
                key: "two".into(),
                cells: vec![Cell::number(Some(2.))],
            },
            Row {
                key: "missing".into(),
                cells: vec![Cell::number(None)],
            },
        ];
        assert_eq!(order(&rows, &State::default()), vec![1, 0, 2]);
        assert_eq!(
            order(
                &rows,
                &State {
                    filter: "unavailable".into(),
                    ..Default::default()
                }
            ),
            vec![2]
        );
        assert_eq!(rows[1].key, "two");
    }
}

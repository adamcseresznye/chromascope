//! Shared result tables: full-model filtering/sorting and viewport-only rendering.
use eframe::egui;
use std::cmp::Ordering;
#[derive(Clone)]
pub(super) struct Cell {
    pub text: String,
    pub number: Option<f64>,
    pub color: Option<egui::Color32>,
}
impl Cell {
    pub fn text(value: impl ToString) -> Self {
        Self {
            text: value.to_string(),
            number: None,
            color: None,
        }
    }
    /// Tint the cell text, e.g. to signal review status.
    pub fn colored(mut self, color: egui::Color32) -> Self {
        self.color = Some(color);
        self
    }
    pub fn number(value: Option<f64>) -> Self {
        Self {
            text: value
                .map(|v| format!("{v:.6e}"))
                .unwrap_or_else(|| "Unavailable".into()),
            number: value,
            color: None,
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
    show_with(ui, id, headers, rows, selected, 280., &[])
}
const ROW_HEIGHT: f32 = 26.;
/// Table with a filter bar, sortable/resizable header and striped, click-to-select rows.
/// `body_height` bounds the scrolling row area; `weights` seed relative column widths.
pub(super) fn show_with(
    ui: &mut egui::Ui,
    id: &str,
    headers: &[&str],
    rows: &[Row],
    selected: Option<&str>,
    body_height: f32,
    weights: &[f32],
) -> Option<usize> {
    let key = egui::Id::new(("analytical_table", id));
    let mut state = ui
        .ctx()
        .data(|d| d.get_temp::<State>(key))
        .unwrap_or_default();
    if state.widths.len() != headers.len() {
        state.widths = (0..headers.len())
            .map(|i| weights.get(i).copied().unwrap_or(150.))
            .collect();
    }
    ui.push_id(id, |ui| {
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut state.filter)
                    .hint_text("Filter rows…")
                    .desired_width(220.),
            );
            if !state.filter.is_empty() && ui.small_button("✕").on_hover_text("Clear filter").clicked() {
                state.filter.clear();
            }
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
    let indices = state.cached_order.clone();
    ui.add_space(4.);
    let available = ui.available_width();
    let sum: f32 = state.widths.iter().sum::<f32>().max(1.);
    // Columns stretch to fill the pane; dragging a header edge changes their relative share.
    let scale = (available / sum).max(1.);
    let widths: Vec<f32> = state.widths.iter().map(|w| w * scale).collect();
    let total = widths.iter().sum::<f32>();
    let mut clicked = None;
    egui::ScrollArea::horizontal()
        .id_salt((id, "horizontal"))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.;
            let (header, _) = ui.allocate_exact_size(egui::vec2(total, ROW_HEIGHT), egui::Sense::hover());
            ui.painter()
                .rect_filled(header, 3., ui.visuals().widgets.noninteractive.weak_bg_fill);
            let mut x = header.left();
            for (i, title) in headers.iter().enumerate() {
                let cell = egui::Rect::from_min_size(
                    egui::pos2(x, header.top()),
                    egui::vec2(widths[i], ROW_HEIGHT),
                );
                x += widths[i];
                let sort = ui.interact(cell, ui.id().with((id, "sort", i)), egui::Sense::click());
                if sort.clicked() {
                    if state.column == i {
                        state.descending = !state.descending;
                    } else {
                        state.column = i;
                        state.descending = false;
                    }
                }
                let arrow = if state.column == i {
                    if state.descending { " ↓" } else { " ↑" }
                } else {
                    ""
                };
                let color = if sort.hovered() {
                    ui.visuals().strong_text_color()
                } else {
                    ui.visuals().text_color()
                };
                ui.painter().with_clip_rect(cell).text(
                    egui::pos2(cell.left() + 8., cell.center().y),
                    egui::Align2::LEFT_CENTER,
                    format!("{title}{arrow}"),
                    egui::TextStyle::Body.resolve(ui.style()),
                    color,
                );
                sort.on_hover_text("Click to sort");
                let grip = egui::Rect::from_center_size(
                    egui::pos2(cell.right(), cell.center().y),
                    egui::vec2(8., ROW_HEIGHT),
                );
                let drag = ui
                    .interact(grip, ui.id().with((id, "grip", i)), egui::Sense::drag())
                    .on_hover_cursor(egui::CursorIcon::ResizeColumn);
                if drag.dragged() {
                    state.widths[i] = (state.widths[i] + drag.drag_delta().x / scale).clamp(50., 600.);
                }
                if i + 1 < headers.len() {
                    ui.painter().vline(
                        cell.right(),
                        cell.y_range().shrink(6.),
                        ui.visuals().widgets.noninteractive.bg_stroke,
                    );
                }
            }
            egui::ScrollArea::vertical()
                .id_salt((id, "rows"))
                .max_height(body_height.max(ROW_HEIGHT * 3.))
                .auto_shrink([false, true])
                .show_rows(ui, ROW_HEIGHT, indices.len(), |ui, range| {
                    for visible in range {
                        let index = indices[visible];
                        let row = &rows[index];
                        let is_selected = selected == Some(row.key.as_str());
                        let (rect, response) = ui.allocate_exact_size(
                            egui::vec2(total, ROW_HEIGHT),
                            egui::Sense::click(),
                        );
                        response.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::WidgetType::SelectableLabel,
                                true,
                                is_selected,
                                row.cells
                                    .iter()
                                    .map(|c| c.text.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", "),
                            )
                        });
                        let visuals = ui.visuals();
                        let fill = if is_selected {
                            visuals.selection.bg_fill
                        } else if response.hovered() {
                            visuals.widgets.hovered.weak_bg_fill
                        } else if visible % 2 == 1 {
                            visuals.faint_bg_color
                        } else {
                            egui::Color32::TRANSPARENT
                        };
                        ui.painter().rect_filled(rect, 0., fill);
                        let mut x = rect.left();
                        let mut hovered_text = None;
                        for (column, cell) in row.cells.iter().enumerate() {
                            let width = widths.get(column).copied().unwrap_or(150.);
                            let area = egui::Rect::from_min_size(
                                egui::pos2(x, rect.top()),
                                egui::vec2(width, ROW_HEIGHT),
                            );
                            x += width;
                            if response.hover_pos().is_some_and(|p| area.contains(p)) {
                                hovered_text = Some(
                                    cell.number
                                        .map(|v| v.to_string())
                                        .unwrap_or_else(|| cell.text.clone()),
                                );
                            }
                            if response.clicked()
                                && response
                                    .interact_pointer_pos()
                                    .is_some_and(|p| area.contains(p))
                            {
                                ui.ctx().data_mut(|d| {
                                    d.insert_temp(egui::Id::new(("table_column", id)), column)
                                });
                            }
                            let color = cell.color.unwrap_or(if is_selected {
                                ui.visuals().strong_text_color()
                            } else {
                                ui.visuals().text_color()
                            });
                            let painter = ui.painter().with_clip_rect(area);
                            if cell.number.is_some() {
                                painter.text(
                                    egui::pos2(area.right() - 10., area.center().y),
                                    egui::Align2::RIGHT_CENTER,
                                    &cell.text,
                                    egui::TextStyle::Monospace.resolve(ui.style()),
                                    color,
                                );
                            } else {
                                painter.text(
                                    egui::pos2(area.left() + 8., area.center().y),
                                    egui::Align2::LEFT_CENTER,
                                    &cell.text,
                                    egui::TextStyle::Body.resolve(ui.style()),
                                    color,
                                );
                            }
                        }
                        if response.clicked() {
                            clicked = Some(index);
                        }
                        if let Some(text) = hovered_text {
                            response.on_hover_text(text);
                        }
                    }
                });
        });
    ui.add_space(2.);
    ui.small(if indices.len() == rows.len() {
        format!("{} {}", rows.len(), if rows.len() == 1 { "row" } else { "rows" })
    } else {
        format!("{} of {} rows", indices.len(), rows.len())
    });
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

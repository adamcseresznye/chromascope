//! Structured editors serialize back into the existing typed engine DTOs.
//! Drafts are committed only after deserialization; untouched values stay exact.
use eframe::egui;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

pub(super) fn typed<T: DeserializeOwned + Serialize>(
    ui: &mut egui::Ui,
    title: &str,
    text: &mut String,
) {
    body::<T>(ui, title, None, text, false);
}

/// Guided routine form: open by default with a one-line scientific
/// orientation. Values map faithfully onto the same typed operation
/// configuration as the advanced editor; invalid edits are rejected with
/// the engine's explanation and the draft is left unchanged.
pub(super) fn guided<T: DeserializeOwned + Serialize>(
    ui: &mut egui::Ui,
    title: &str,
    help: &str,
    text: &mut String,
) {
    body::<T>(ui, title, Some(help), text, true);
}

fn body<T: DeserializeOwned + Serialize>(
    ui: &mut egui::Ui,
    title: &str,
    help: Option<&str>,
    text: &mut String,
    default_open: bool,
) {
    egui::CollapsingHeader::new(title).id_salt(title).default_open(default_open).show(ui, |ui| {
        if let Some(help) = help {
            ui.small(help);
        }
        let Ok(draft)=serde_json::from_str::<T>(text) else { ui.label("Load or prepare a valid draft to edit this form. The advanced editor retains invalid input for correction.");return; };
        let Ok(mut value)=serde_json::to_value(draft) else {return;};
        let roles = if std::any::type_name::<T>().contains("::untargeted::") { &["sample", "blank", "qc"][..] } else if std::any::type_name::<T>().contains("::targeted::") { &["standard", "blank", "qc", "unknown"][..] } else { &[] };
        ui.push_id(title, |ui| {
            if edit(ui,"",&mut value,roles) {
                match serde_json::from_value::<T>(value) {
                    Ok(updated)=>*text=serde_json::to_string_pretty(&updated).unwrap(),
                    Err(error)=>{ui.colored_label(ui.visuals().error_fg_color,format!("Draft unchanged: {error}"));}
                }
            }
        });
    });
}

fn choices(field: &str) -> &'static [&'static str] {
    match field {
        "polarity" => &["positive", "negative", "unknown"],
        "weighting" => &["unweighted", "inverse_x", "inverse_x2"],
        "unit" => &["ng_ml", "ug_ml", "mg_l", "nmol_l", "umol_l"],
        "purpose" => &["batch_qc", "method_validation"],
        "representation" => &["centroid", "profile", "unknown"],
        "confidence" => &[
            "unknown",
            "compound_class",
            "probable_structure",
            "confirmed_identity",
        ],
        _ => &[],
    }
}
fn label(field: &str) -> String {
    let units = if field.ends_with("_seconds") {
        " (s)"
    } else if field.ends_with("_minutes") {
        " (min)"
    } else if field.ends_with("_ppm") {
        " (ppm)"
    } else {
        ""
    };
    format!("{}{units}", field.replace('_', " "))
}
fn edit(ui: &mut egui::Ui, field: &str, value: &mut Value, roles: &[&str]) -> bool {
    let mut changed = false;
    if !value.is_null()
        && optional_default(field).is_some()
        && ui
            .small_button(format!("Clear optional {}", label(field)))
            .clicked()
    {
        *value = Value::Null;
        return true;
    }
    ui.push_id(field,|ui| match value {
        Value::Object(map)=>{
            if matches!(field,"nominal"|"exclusions"|"metadata"|"excluded_samples"|"excluded_features"|"metadata_equals") {
                let id = ui.make_persistent_id("new_dictionary_key");
                let mut key = ui.ctx().data(|d| d.get_temp::<String>(id)).unwrap_or_default();
                ui.horizontal_wrapped(|ui| {
                    ui.label("New target / metadata key"); ui.text_edit_singleline(&mut key);
                    if ui.add_enabled(!key.trim().is_empty() && !map.contains_key(&key), egui::Button::new("Add entry")).clicked() {
                        map.insert(key.clone(), if field == "nominal" { serde_json::json!(0.0) } else { Value::String(String::new()) }); key.clear(); changed = true;
                    }
                });
                ui.ctx().data_mut(|d| d.insert_temp(id,key));
                let mut remove = None;
                for (key,value) in map.iter_mut() { ui.push_id(key,|ui| { changed |= edit(ui,key,value,roles); if ui.small_button("Remove entry").clicked() {remove=Some(key.clone());} }); }
                if let Some(key) = remove { map.remove(&key); changed=true; }
                return;
            }
            for (key,value) in map.iter_mut() {
                // Evidence is immutable and can be enormous. Configuration is editable.
                if matches!(key.as_str(),"targeted_evidence"|"source"|"matrix"|"batch"|"analysis"|"report"|"ledger"|"library"|"processed"|"query") && value.is_object() {
                    ui.collapsing(format!("{} · retained evidence",label(key)),|ui| {ui.label("Preserved from the original result. Prepare a new analysis to change evidence.");});
                } else if value.is_object() || value.is_array() {
                    egui::CollapsingHeader::new(label(key)).id_salt(key).show(ui,|ui| {changed|=edit(ui,key,value,roles);});
                } else { changed|=edit(ui,key,value,roles); }
            }
        }
        Value::Array(items)=>{
            if matches!(field,"peaks"|"points"|"eic"|"isotope_mz"|"values"){ui.label(format!("{} retained values; inspect the linked table/plot for numeric evidence",items.len()));return;}
            let template=items.first().cloned().or_else(|| array_template(field));
            let mut remove=None;
            for (i,item) in items.iter_mut().enumerate() {
                ui.push_id(i,|ui| {
                    ui.horizontal(|ui| { ui.strong(format!("{} {}",label(field),i+1));if ui.small_button("Remove row").clicked(){remove=Some(i);} });
                    changed|=edit(ui,"value",item,roles);
                    ui.separator();
                });
            }
            if let Some(i)=remove {items.remove(i);changed=true;}
            if let Some(template)=template {
                if ui.button("Add row from current template").clicked() {items.push(template);changed=true;}
                ui.small("New rows copy the first row. Give each sample, target or rule its own identity before running.");
            } else {ui.small("No rows configured. Use the workspace's preparation or import controls to initialize this collection.");}
        }
        Value::Bool(v)=>{changed|=ui.checkbox(v,label(field)).changed();}
        Value::String(v)=>{ui.horizontal_wrapped(|ui| {
            let field_label = ui.label(label(field));
            let options=if field == "role" { roles } else if field == "unit" && matches!(v.as_str(),"ppm"|"da") { &["ppm","da"][..] } else if field == "unit" && !matches!(v.as_str(),"ng_ml"|"ug_ml"|"mg_l"|"nmol_l"|"umol_l") { &[] } else { choices(field) };
            if options.is_empty() {changed|=ui.add(egui::TextEdit::singleline(v).desired_width(260.)).labelled_by(field_label.id).changed();}
            else {egui::ComboBox::from_id_salt("choice").selected_text(v.as_str()).show_ui(ui,|ui| {for option in options {changed|=ui.selectable_value(v,(*option).to_owned(),*option).changed();}});}
        });}
        Value::Number(n)=>{ui.horizontal_wrapped(|ui| {
            let field_label = ui.label(label(field));
            if let Some(mut number)=n.as_u64() {if ui.add(egui::DragValue::new(&mut number)).labelled_by(field_label.id).changed(){*n=number.into();changed=true;}}
            else if let Some(mut number)=n.as_i64() {if ui.add(egui::DragValue::new(&mut number)).labelled_by(field_label.id).changed(){*n=number.into();changed=true;}}
            else if let Some(mut number)=n.as_f64() {if ui.add(egui::DragValue::new(&mut number).speed(0.01).max_decimals(10)).labelled_by(field_label.id).changed(){if let Some(new)=serde_json::Number::from_f64(number){*n=new;changed=true;}}}
        });}
        Value::Null=>{ui.horizontal_wrapped(|ui|{ui.label(format!("{}: unavailable / not configured",label(field)));if let Some(default)=optional_default(field){if ui.button("Configure").clicked(){*value=default;changed=true;}}});}
    });
    changed
}
fn optional_default(field: &str) -> Option<Value> {
    match field {
        "groups" => Some(
            serde_json::json!({"metadata_key":"group","reference":"control","comparison":"treated"}),
        ),
        "is_area_range" => Some(serde_json::json!([1.0, 1e12])),
        "internal_standard"
        | "reference_group"
        | "group"
        | "batch"
        | "candidate_accession"
        | "instrument"
        | "precursor_type" => Some(Value::String(String::new())),
        "nominal"
        | "value"
        | "response"
        | "is_response"
        | "rt_minutes"
        | "expected_rt_minutes"
        | "mass_error_ppm"
        | "centroid_snr"
        | "precursor_mz" => Some(serde_json::json!(0.0)),
        _ => None,
    }
}

fn array_template(field: &str) -> Option<Value> {
    match field {
        "evidence" => Some(
            serde_json::json!({"description":"","reference":"","same_method_standard":false,"rt_match":false,"diagnostic_fragments":false,"resolves_alternatives":false}),
        ),
        "elements" | "adducts" | "hypotheses" => None,
        _ => None,
    }
}

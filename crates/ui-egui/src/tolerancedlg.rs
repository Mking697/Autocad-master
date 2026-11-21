//! The Geometric Tolerance dialog (TOLERANCE from the Dimension menu or toolbar): builds a
//! feature control frame the same way TOLERANCE's own command-line prompts do, then OK runs the
//! real `tolerance` engine command with the composed frame. The dialog holds no drawing data of
//! its own (it reads nothing from and duplicates no parsing of the engine) — it only assembles
//! [`cadcraft_doc::GdtLine`]/[`cadcraft_doc::GdtFrame`] values and calls
//! [`cadcraft_doc::compose_frame`], the exact function TOLERANCE itself (JSON form and
//! command-line prompts) and LEADER/QLEADER's tolerance annotation use to turn those values into
//! the DXF frame string; the empty-frame and dimension-style checks happen inside the `tolerance`
//! command, not here.
//!
//! State lives in egui's own per-`Id` temp storage (as the Drawing Units dialog does), not on
//! `CadApp`, since a dialog here owns no persistent drawing state.

use egui::vec2;
use serde_json::{Value, json};

use cadcraft_doc::{GdtDatum, GdtFrame, GdtLine, GdtSymbol, GdtTol};

use crate::CadApp;
use crate::theme::Tokens;

/// One frame line's form fields: a tolerance value is included only when its checkbox is on, so
/// an empty text field cleanly means "no second tolerance" rather than a blank one.
#[derive(Clone, Debug, PartialEq)]
struct LineForm {
    symbol: Option<GdtSymbol>,
    tol1_on: bool,
    tol1_dia: bool,
    tol1_val: String,
    /// "" (none), "M", "L" or "S".
    tol1_mat: String,
    tol2_on: bool,
    tol2_dia: bool,
    tol2_val: String,
    tol2_mat: String,
    datums: [String; 3],
    datum_mats: [String; 3],
}

impl Default for LineForm {
    fn default() -> Self {
        LineForm {
            symbol: Some(GdtSymbol::Position),
            tol1_on: false,
            tol1_dia: false,
            tol1_val: String::new(),
            tol1_mat: String::new(),
            tol2_on: false,
            tol2_dia: false,
            tol2_val: String::new(),
            tol2_mat: String::new(),
            datums: [String::new(), String::new(), String::new()],
            datum_mats: [String::new(), String::new(), String::new()],
        }
    }
}

/// The whole dialog's fields.
#[derive(Clone, Debug, PartialEq)]
struct ToleranceForm {
    line1: LineForm,
    second_line: bool,
    line2: LineForm,
    /// Projected tolerance zone height ("" = none).
    height: String,
    projected: bool,
    /// Datum identifier ("" = none).
    datum_id: String,
    rotation: f64,
    at: [f64; 2],
    style: String,
    error: Option<String>,
}

impl Default for ToleranceForm {
    fn default() -> Self {
        ToleranceForm {
            line1: LineForm::default(),
            second_line: false,
            line2: LineForm { symbol: None, ..LineForm::default() },
            height: String::new(),
            projected: false,
            datum_id: String::new(),
            rotation: 0.0,
            at: [0.0, 0.0],
            style: String::new(),
            error: None,
        }
    }
}

/// The geometric characteristics the dialog offers, in the same order AutoCAD's own Geometric
/// Tolerance dialog shows them.
const CHARACTERISTICS: &[(&str, GdtSymbol)] = &[
    ("Position", GdtSymbol::Position),
    ("Concentricity", GdtSymbol::Concentricity),
    ("Symmetry", GdtSymbol::Symmetry),
    ("Parallelism", GdtSymbol::Parallelism),
    ("Perpendicularity", GdtSymbol::Perpendicularity),
    ("Angularity", GdtSymbol::Angularity),
    ("Cylindricity", GdtSymbol::Cylindricity),
    ("Flatness", GdtSymbol::Flatness),
    ("Circularity", GdtSymbol::Circularity),
    ("Straightness", GdtSymbol::Straightness),
    ("Profile of a Surface", GdtSymbol::SurfaceProfile),
    ("Profile of a Line", GdtSymbol::LineProfile),
    ("Circular Runout", GdtSymbol::CircularRunout),
    ("Total Runout", GdtSymbol::TotalRunout),
];

/// Material condition choices for a tolerance value or datum reference: (label, the "M"/"L"/"S"
/// stored in the form, matching what [`cadcraft_doc::GdtSymbol::parse`] accepts).
const MATERIALS: &[(&str, &str)] =
    &[("None", ""), ("Maximum Material Condition", "M"), ("Least Material Condition", "L"), ("Regardless of Feature Size", "S")];

fn characteristic_label(s: GdtSymbol) -> &'static str {
    CHARACTERISTICS.iter().find(|(_, g)| *g == s).map_or("Position", |(l, _)| l)
}

/// `form.line1` and, when `second_line`, `form.line2` as [`GdtLine`]s — exactly the struct
/// TOLERANCE's own command-line prompts and JSON form build.
fn to_gdt_line(l: &LineForm) -> GdtLine {
    let tol = |on: bool, dia: bool, val: &str, mat: &str| -> Option<GdtTol> {
        on.then(|| GdtTol { diameter: dia, value: val.to_string(), material: (!mat.is_empty()).then(|| mat.to_string()) })
    };
    let datums = l
        .datums
        .iter()
        .zip(&l.datum_mats)
        .filter(|(d, _)| !d.trim().is_empty())
        .map(|(d, m)| GdtDatum { letter: d.trim().to_string(), material: (!m.is_empty()).then(|| m.to_string()) })
        .collect();
    GdtLine {
        symbol: l.symbol,
        tol1: tol(l.tol1_on, l.tol1_dia, &l.tol1_val, &l.tol1_mat),
        tol2: tol(l.tol2_on, l.tol2_dia, &l.tol2_val, &l.tol2_mat),
        datums,
    }
}

/// The [`GdtFrame`] the form describes.
fn build_frame(form: &ToleranceForm) -> GdtFrame {
    let mut lines = vec![to_gdt_line(&form.line1)];
    if form.second_line {
        lines.push(to_gdt_line(&form.line2));
    }
    GdtFrame { lines, height: form.height.clone(), projected: form.projected, datum_id: form.datum_id.clone() }
}

/// The frame string TOLERANCE itself would compose from these fields (DXF group 1); shared with
/// TOLERANCE's own command-line prompts and JSON form, and LEADER/QLEADER's tolerance annotation,
/// through [`cadcraft_doc::compose_frame`] — no frame-building logic is duplicated here.
fn build_text(form: &ToleranceForm) -> String {
    cadcraft_doc::compose_frame(&build_frame(form))
}

/// A frame with nothing to show, the same check TOLERANCE's own prompts and JSON form make.
fn is_empty_frame(text: &str) -> bool {
    cadcraft_doc::parse_frame(text).iter().flatten().all(cadcraft_doc::compartment_is_empty)
}

/// Runs the real `tolerance` command with the frame the form describes. The command itself
/// re-validates the frame and dimension style; this function duplicates none of that.
fn apply(app: &mut CadApp, form: &ToleranceForm) -> Result<Value, String> {
    let mut params = json!({ "at": form.at, "rotation": form.rotation, "text": build_text(form) });
    if !form.style.trim().is_empty()
        && let Some(obj) = params.as_object_mut()
    {
        obj.insert("style".into(), json!(form.style.trim()));
    }
    app.run("tolerance", params)
}

/// Open the dialog.
pub fn open(app: &mut CadApp) {
    app.ui.dialog = Some("tolerance".into());
}

fn material_combo(ui: &mut egui::Ui, salt: &str, mat: &mut String) {
    let current = MATERIALS.iter().find(|(_, k)| *k == mat.as_str()).map_or("None", |(l, _)| l);
    egui::ComboBox::from_id_salt(salt).selected_text(current).width(90.0).show_ui(ui, |ui| {
        for (label, key) in MATERIALS {
            if ui.selectable_label(mat.as_str() == *key, *label).clicked() {
                *mat = key.to_string();
            }
        }
    });
}

fn tol_row(ui: &mut egui::Ui, salt: &str, label: &str, fields: (&mut bool, &mut bool, &mut String, &mut String)) {
    let (on, dia, val, mat) = fields;
    ui.label(label);
    ui.horizontal(|ui| {
        ui.checkbox(on, "");
        ui.checkbox(dia, "⌀");
        ui.add_enabled(*on, egui::TextEdit::singleline(val).desired_width(70.0));
        ui.add_enabled_ui(*on, |ui| material_combo(ui, salt, mat));
    });
    ui.end_row();
}

/// One frame line's fields: geometric characteristic, up to two tolerance values and up to
/// three datum references.
fn line_editor(ui: &mut egui::Ui, salt: &str, l: &mut LineForm) {
    egui::Grid::new(salt).num_columns(2).spacing(vec2(8.0, 4.0)).show(ui, |ui| {
        ui.label(crate::tl!("Geometric characteristic"));
        let symbol_salt = format!("{salt}_symbol");
        egui::ComboBox::from_id_salt(symbol_salt.as_str()).selected_text(l.symbol.map_or("None", characteristic_label)).show_ui(ui, |ui| {
            if ui.selectable_label(l.symbol.is_none(), crate::tl!("None")).clicked() {
                l.symbol = None;
            }
            for (label, sym) in CHARACTERISTICS {
                if ui.selectable_label(l.symbol == Some(*sym), *label).clicked() {
                    l.symbol = Some(*sym);
                }
            }
        });
        ui.end_row();
        let tol1_label = crate::tl!("Tolerance 1");
        tol_row(ui, &format!("{salt}_tol1"), tol1_label, (&mut l.tol1_on, &mut l.tol1_dia, &mut l.tol1_val, &mut l.tol1_mat));
        let tol2_label = crate::tl!("Tolerance 2");
        tol_row(ui, &format!("{salt}_tol2"), tol2_label, (&mut l.tol2_on, &mut l.tol2_dia, &mut l.tol2_val, &mut l.tol2_mat));
        for i in 0..3 {
            ui.label(crate::tf!("Datum {n}", n = i + 1));
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut l.datums[i]);
                material_combo(ui, &format!("{salt}_datum{i}"), &mut l.datum_mats[i]);
            });
            ui.end_row();
        }
    });
}

/// Geometric Tolerance window: builds a feature control frame and runs it through the real
/// `tolerance` command on OK.
pub fn dialog(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) {
    let id = egui::Id::unique("tolerance_dialog_form");
    let mut form = ctx.data_mut(|d| d.get_temp::<ToleranceForm>(id)).unwrap_or_default();
    let (mut ok, mut cancel) = (false, false);
    let text = build_text(&form);
    let empty = is_empty_frame(&text);
    egui::Window::new(crate::tl!("Geometric Tolerance"))
        .id(egui::Id::unique("geometric_tolerance_dialog"))
        .open(open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.group(|ui| {
                ui.strong(crate::tl!("Tolerance 1"));
                line_editor(ui, "gtol_line1", &mut form.line1);
            });
            ui.checkbox(&mut form.second_line, crate::tl!("Second tolerance line"));
            if form.second_line {
                ui.group(|ui| {
                    ui.strong(crate::tl!("Tolerance 2"));
                    line_editor(ui, "gtol_line2", &mut form.line2);
                });
            }
            egui::Grid::new("gtol_extra").num_columns(2).spacing(vec2(8.0, 4.0)).show(ui, |ui| {
                ui.label(crate::tl!("Height (projected tolerance zone)"));
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut form.height).desired_width(70.0));
                    ui.checkbox(&mut form.projected, crate::tl!("Projected"));
                });
                ui.end_row();
                ui.label(crate::tl!("Datum identifier"));
                ui.text_edit_singleline(&mut form.datum_id);
                ui.end_row();
                ui.label(crate::tl!("Rotation (degrees)"));
                ui.add(egui::DragValue::new(&mut form.rotation).speed(1.0));
                ui.end_row();
                ui.label(crate::tl!("Location"));
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut form.at[0]).speed(0.1).prefix("X: "));
                    ui.add(egui::DragValue::new(&mut form.at[1]).speed(0.1).prefix("Y: "));
                });
                ui.end_row();
                ui.label(crate::tl!("Dimension style"));
                ui.add(egui::TextEdit::singleline(&mut form.style).hint_text(crate::tl!("(current)")).desired_width(120.0));
                ui.end_row();
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(crate::tl!("Preview:"));
                let preview = if empty {
                    crate::tl!("(empty — add a characteristic, value or datum)").to_string()
                } else {
                    format!("{} compartments", text.matches("%%v").count() + 1)
                };
                ui.label(preview);
            });
            if let Some(e) = &form.error {
                ui.colored_label(Tokens::get().warn, e.as_str());
            }
            ui.separator();
            ui.horizontal(|ui| {
                ok = ui.add_enabled(!empty, egui::Button::new(crate::tl!("OK"))).clicked();
                cancel = ui.button(crate::tl!("Cancel")).clicked();
            });
        });
    if ok {
        match apply(app, &form) {
            Ok(_) => {
                ctx.data_mut(|d| d.remove::<ToleranceForm>(id));
                *open = false;
                return;
            }
            Err(e) => form.error = Some(e),
        }
    }
    if cancel || !*open {
        ctx.data_mut(|d| d.remove::<ToleranceForm>(id));
        *open = false;
    } else {
        ctx.data_mut(|d| d.insert_temp(id, form));
    }
}

#[cfg(test)]
mod tests {
    use cadcraft_engine::Session;
    use cadcraft_engine::doc::EntityKind;

    use super::*;
    use crate::{CadApp, Services};

    #[test]
    fn dialog_renders_real_fields() {
        fn collect(shape: &egui::Shape, text: &mut String) {
            match shape {
                egui::Shape::Text(t) => {
                    text.push_str(t.galley.text());
                    text.push('\n');
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, text);
                    }
                }
                _ => {}
            }
        }
        let mut app = CadApp::new(Session::new(), Services::default());
        let ctx = egui::Context::default();
        let mut open = true;
        let mut text = String::new();
        for _ in 0..2 {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| dialog(&mut app, ui.ctx(), &mut open));
            output.textures_delta.clear();
            for shape in output.shapes {
                collect(&shape.shape, &mut text);
            }
        }
        for label in ["Geometric Tolerance", "Geometric characteristic", "Tolerance 1", "Datum 1", "Datum identifier", "Location", "OK", "Cancel"] {
            assert!(text.contains(label), "missing {label}: {text}");
        }
    }

    #[test]
    fn default_form_describes_a_non_empty_position_frame() {
        let form = ToleranceForm::default();
        assert!(!is_empty_frame(&build_text(&form)), "a bare characteristic symbol is content");
    }

    #[test]
    fn a_frame_with_no_characteristic_tolerance_or_datum_is_empty() {
        let mut form = ToleranceForm::default();
        form.line1.symbol = None;
        assert!(is_empty_frame(&build_text(&form)));
    }

    #[test]
    fn the_form_builds_the_gdt_line_the_fields_describe() {
        let mut form = ToleranceForm::default();
        form.line1.tol1_on = true;
        form.line1.tol1_dia = true;
        form.line1.tol1_val = "0.1".into();
        form.line1.tol1_mat = "M".into();
        form.line1.datums[0] = "A".into();
        let frame = build_frame(&form);
        assert_eq!(frame.lines.len(), 1, "second line is off by default");
        assert_eq!(frame.lines[0].symbol, Some(GdtSymbol::Position));
        assert_eq!(frame.lines[0].tol1, Some(GdtTol { diameter: true, value: "0.1".into(), material: Some("M".into()) }));
        assert_eq!(frame.lines[0].datums, vec![GdtDatum { letter: "A".into(), material: None }]);
    }

    #[test]
    fn a_second_line_is_included_only_when_turned_on() {
        let mut form = ToleranceForm::default();
        form.line2.symbol = Some(GdtSymbol::Flatness);
        assert_eq!(build_frame(&form).lines.len(), 1);
        form.second_line = true;
        let frame = build_frame(&form);
        assert_eq!(frame.lines.len(), 2);
        assert_eq!(frame.lines[1].symbol, Some(GdtSymbol::Flatness));
    }

    #[test]
    fn ok_calls_the_real_tolerance_command_and_adds_a_frame_entity() {
        let mut app = CadApp::new(Session::new(), Services::default());
        let mut form = ToleranceForm::default();
        form.line1.tol1_on = true;
        form.line1.tol1_val = "0.1".into();
        form.at = [3.0, 4.0];
        let r = apply(&mut app, &form).unwrap();
        assert!(r.get("handle").is_some());
        let d = app.session.doc().unwrap();
        let frames: Vec<_> = d
            .model
            .iter()
            .filter_map(|e| match &e.kind {
                EntityKind::Tolerance(frame) => Some(frame),
                _ => None,
            })
            .collect();
        assert_eq!(frames.len(), 1);
        assert!((frames[0].insert.x - 3.0).abs() < 1e-9 && (frames[0].insert.y - 4.0).abs() < 1e-9);
    }

    #[test]
    fn an_empty_frame_is_rejected_like_the_command_line_prompt() {
        let mut app = CadApp::new(Session::new(), Services::default());
        let mut form = ToleranceForm::default();
        form.line1.symbol = None;
        assert!(apply(&mut app, &form).is_err());
    }

    #[test]
    fn an_unknown_dimension_style_is_rejected_by_the_real_command() {
        let mut app = CadApp::new(Session::new(), Services::default());
        let form = ToleranceForm { style: "NoSuchStyle".into(), ..Default::default() };
        let err = apply(&mut app, &form).unwrap_err();
        assert!(err.contains("NoSuchStyle"), "{err}");
    }
}

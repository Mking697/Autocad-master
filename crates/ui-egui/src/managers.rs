//! Style and settings dialogs opened from the menus: Linetype Manager, Point Style, Text Style,
//! Table Style, Multileader Style, Page Setup Manager, Plot and Constraint Settings.
//!
//! Typed or chosen from a menu (no parameters) these commands open their dialog; JSON calls run
//! the command. The dialogs read the drawing and change it only through the same commands
//! (`app.run`), so drawing changes are undoable and scriptable. MATCHPROP Settings remembers
//! session preferences through a command too, without adding drawing undo steps.

use std::sync::atomic::{AtomicU32, Ordering};

use egui::{RichText, Sense, Stroke, vec2};
use serde_json::{Value, json};

use crate::CadApp;
use crate::parametric::expr_field;
use crate::theme::Tokens;

/// The Block Attribute Manager (BATTMAN).
mod battman;
mod matchprop;

/// Open the dialog for a typed or menu-invoked command. `None` if `id` has no dialog here or the
/// call carries parameters.
pub fn route(app: &mut CadApp, id: &str, params: &Value) -> Option<Result<Value, String>> {
    if !params.is_null() {
        return None;
    }
    let dialog = match id {
        "battman" => "battman",
        "linetype" | "lt" | "ltype" => "linetype",
        "ddptype" | "ptype" => "ptype",
        "style" | "st" => "textstyle",
        "tablestyle" | "ts" => "tablestyle",
        "mleaderstyle" | "mls" => "mleaderstyle",
        "dimstyle" | "d" | "dst" | "ddim" => "dimstyle",
        "pagesetup" => "pagesetup",
        "constraintsettings" | "csettings" => "csettings",
        "plot" | "print" => {
            PLOT_GEN.fetch_add(1, Ordering::Relaxed);
            "plot"
        }
        _ => return None,
    };
    app.ui.dialog = Some(dialog.into());
    Some(Ok(Value::Null))
}

/// Show the dialog named `name`; closes it (`open = false`) if it isn't one of these.
pub fn dialog(app: &mut CadApp, ctx: &egui::Context, name: &str, open: &mut bool) {
    let action = match name {
        "battman" => battman::dialog(app, ctx, open),
        "linetype" => linetype(app, ctx, open),
        "ptype" => point_style(app, ctx, open),
        "textstyle" => text_style(app, ctx, open),
        "tablestyle" => table_style(app, ctx, open),
        "mleaderstyle" => mleader_style(app, ctx, open),
        "dimstyle" => dimstyle(app, ctx, open),
        "pagesetup" => page_setup(app, ctx, open),
        "csettings" => constraint_settings(app, ctx, open),
        "matchprop" => matchprop::dialog(app, ctx, open),
        "plot" => plot(app, ctx, open),
        _ => {
            *open = false;
            None
        }
    };
    if let Some((cmd, p)) = action {
        // Errors are echoed on the command line by `run`.
        let _ = app.run(cmd, p);
    }
}

/// A command to run after the dialog is drawn.
type Action = Option<(&'static str, Value)>;

fn fmt_num(v: f64) -> String {
    let s = format!("{v:.4}");
    if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s }
}

/// A number field that commits on Enter or focus loss.
///
/// `label` is the visible label's widget id (see [`expr_field`]).
fn num_field(ui: &mut egui::Ui, id: impl egui::AsIdSalt, v: f64, label: Option<egui::Id>) -> Option<f64> {
    expr_field(ui, ui.make_persistent_id(id), &fmt_num(v), label).and_then(|t| t.parse::<f64>().ok()).filter(|v| v.is_finite())
}

fn temp<T: Clone + Send + Sync + 'static>(ctx: &egui::Context, id: &str) -> Option<T> {
    ctx.data_mut(|d| d.get_temp::<T>(egui::Id::unique(id)))
}

fn set_temp<T: Clone + Send + Sync + 'static>(ctx: &egui::Context, id: &str, v: T) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(id), v));
}

fn no_drawing(ctx: &egui::Context, title: &str, open: &mut bool) {
    egui::Window::new(title).open(open).collapsible(false).resizable(false).show(ctx, |ui| {
        ui.label("Open a drawing first.");
    });
}

// ---------- Linetype Manager ----------

/// Linetype Manager: current linetype and global scale, loaded linetypes, and load/current actions; returns the command to run after drawing.
fn linetype(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) -> Action {
    let t = Tokens::get();
    let Ok(d) = app.session.doc() else {
        no_drawing(ctx, "Linetype Manager", open);
        return None;
    };
    let current = d.header.str("CELTYPE", "ByLayer");
    let ltscale = d.header.f64("LTSCALE", 1.0);
    let mut rows: Vec<(String, String, Vec<f64>)> =
        vec![("ByLayer".into(), String::new(), Vec::new()), ("ByBlock".into(), String::new(), Vec::new())];
    rows.extend(
        d.linetypes
            .iter()
            .filter(|l| !l.name.eq_ignore_ascii_case("ByLayer") && !l.name.eq_ignore_ascii_case("ByBlock"))
            .map(|l| (l.name.clone(), l.description.clone(), l.pattern.iter().map(|e| e.length).collect())),
    );
    let library: Vec<String> = cadcraft_doc::library::standard_linetypes().into_iter().map(|l| l.name).filter(|n| d.linetype(n).is_none()).collect();
    let mut sel = temp::<String>(ctx, "ltmgr_sel").filter(|s| rows.iter().any(|r| r.0.eq_ignore_ascii_case(s))).unwrap_or_else(|| current.clone());
    let mut action = None;
    egui::Window::new("Linetype Manager").open(open).default_size(vec2(520.0, 360.0)).resizable(true).show(ctx, |ui| {
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("ltmgr_load").selected_text("Load...").width(110.0).show_ui(ui, |ui| {
                if library.is_empty() {
                    ui.label("Every library linetype is loaded.");
                } else if ui.selectable_label(false, "All").clicked() {
                    action = Some(("linetype", json!({ "load": "*" })));
                }
                for n in &library {
                    if ui.selectable_label(false, n).clicked() {
                        action = Some(("linetype", json!({ "load": n })));
                    }
                }
                // The user's own .lin file (where the host has a file picker).
                if let Some(pick) = app.services.pick_open.as_ref() {
                    ui.separator();
                    if ui.selectable_label(false, "From File...").clicked()
                        && let Some(path) = pick()
                    {
                        action = Some(("linetype", json!({ "load": "*", "file": path })));
                    }
                }
            });
            let is_current = sel.eq_ignore_ascii_case(&current);
            if ui.add_enabled(!is_current, egui::Button::new("Current")).on_hover_text("Draw new objects with the selected linetype").clicked() {
                action = Some(("linetype", json!({ "current": sel })));
            }
        });
        ui.label(RichText::new(format!("Current linetype: {current}")).color(t.text_dim));
        ui.separator();
        egui::ScrollArea::vertical().max_height(240.0).auto_shrink([false, true]).show(ui, |ui| {
            egui::Grid::new("ltmgr_grid").striped(true).num_columns(3).spacing(vec2(16.0, 4.0)).show(ui, |ui| {
                for h in ["Linetype", "Appearance", "Description"] {
                    ui.label(RichText::new(h).strong());
                }
                ui.end_row();
                for (name, desc, pattern) in &rows {
                    let mut label = RichText::new(name);
                    if name.eq_ignore_ascii_case(&current) {
                        label = label.strong();
                    }
                    let r = ui.selectable_label(sel.eq_ignore_ascii_case(name), label);
                    if r.clicked() {
                        sel = name.clone();
                    }
                    if r.double_clicked() {
                        action = Some(("linetype", json!({ "current": name })));
                    }
                    let (rect, _) = ui.allocate_exact_size(vec2(120.0, 14.0), Sense::hover());
                    if !matches!(name.as_str(), "ByLayer" | "ByBlock") {
                        draw_pattern(ui.painter(), rect, pattern, Stroke::new(1.5, t.text));
                    }
                    ui.label(RichText::new(desc).color(t.text_dim));
                    ui.end_row();
                }
            });
        });
        ui.separator();
        ui.horizontal(|ui| {
            let label = ui.label("Global scale factor");
            if let Some(v) = num_field(ui, "ltmgr_scale", ltscale, Some(label.id)).filter(|v| *v > 0.0) {
                action = Some(("ltscale", json!({ "scale": v })));
            }
        });
    });
    set_temp(ctx, "ltmgr_sel", sel);
    action
}

/// A linetype sample: dashes (positive lengths), gaps (negative) and dots (zero), two repeats.
fn draw_pattern(p: &egui::Painter, rect: egui::Rect, pattern: &[f64], stroke: Stroke) {
    let y = rect.center().y;
    let total: f64 = pattern.iter().map(|l| l.abs()).sum();
    if pattern.is_empty() || !total.is_finite() || total <= 0.0 {
        p.line_segment([egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)], stroke);
        return;
    }
    let k = f64::from(rect.width()) / (total * 2.0);
    let mut x = f64::from(rect.left());
    for l in pattern.iter().cycle().take(400) {
        if x >= f64::from(rect.right()) {
            break;
        }
        let len = l.abs() * k;
        if *l > 0.0 {
            let end = (x + len).min(f64::from(rect.right()));
            p.line_segment([egui::pos2(x as f32, y), egui::pos2(end as f32, y)], stroke);
        } else if *l == 0.0 {
            p.circle_filled(egui::pos2(x as f32, y), stroke.width, stroke.color);
        }
        x += len.max(1.0);
    }
}

// ---------- Point Style ----------

/// PDMODE values in dialog order: four rows of five.
const PDMODES: [i64; 20] = [0, 1, 2, 3, 4, 32, 33, 34, 35, 36, 64, 65, 66, 67, 68, 96, 97, 98, 99, 100];

/// The point glyph for a PDMODE, as drawn by the point style tiles.
fn draw_point(p: &egui::Painter, c: egui::Pos2, r: f32, mode: i64, stroke: Stroke) {
    let line = |a: egui::Vec2, b: egui::Vec2| p.line_segment([c + a, c + b], stroke);
    match mode & 7 {
        0 => {
            p.circle_filled(c, stroke.width, stroke.color);
        }
        2 => {
            line(vec2(-r, 0.0), vec2(r, 0.0));
            line(vec2(0.0, -r), vec2(0.0, r));
        }
        3 => {
            line(vec2(-r, -r), vec2(r, r));
            line(vec2(-r, r), vec2(r, -r));
        }
        4 => {
            line(vec2(0.0, 0.0), vec2(0.0, -r));
        }
        _ => {}
    }
    if mode & 32 != 0 {
        p.circle_stroke(c, r * 0.7, stroke);
    }
    if mode & 64 != 0 {
        p.rect_stroke(egui::Rect::from_center_size(c, vec2(r * 1.4, r * 1.4)), 0.0, stroke, egui::StrokeKind::Middle);
    }
}

/// Point Style dialog for the PDMODE/PDSIZE presets; returns the `ddptype` action.
fn point_style(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) -> Action {
    let t = Tokens::get();
    let Ok(d) = app.session.doc() else {
        no_drawing(ctx, "Point Style", open);
        return None;
    };
    let (mode, pdsize) = (d.header.i64("PDMODE", 0), d.header.f64("PDSIZE", 0.0));
    // PDSIZE 0 is 5% of the screen, negative a percentage of the screen, positive drawing units.
    let rel = pdsize <= 0.0;
    let size = if pdsize == 0.0 { 5.0 } else { pdsize.abs() };
    let set = |m: i64, size: f64, rel: bool| Some(("ddptype", json!({ "pdmode": m, "pdsize": if rel { -size } else { size } })));
    let mut action = None;
    egui::Window::new("Point Style").open(open).collapsible(false).resizable(false).show(ctx, |ui| {
        egui::Grid::new("ptype_grid").spacing(vec2(6.0, 6.0)).show(ui, |ui| {
            for (i, v) in PDMODES.iter().enumerate() {
                let (rect, r) = ui.allocate_exact_size(vec2(44.0, 36.0), Sense::click());
                let fill = if *v == mode {
                    t.accent
                } else if r.hovered() {
                    t.control_hover
                } else {
                    t.control
                };
                ui.painter().rect_filled(rect, 3.0, fill);
                draw_point(ui.painter(), rect.center(), 9.0, *v, Stroke::new(1.5, t.text));
                if r.on_hover_text(format!("PDMODE {v}")).clicked() && *v != mode {
                    action = set(*v, size, rel);
                }
                if i % 5 == 4 {
                    ui.end_row();
                }
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let label = ui.label("Point Size");
            if let Some(v) = num_field(ui, "ptype_size", size, Some(label.id)).filter(|v| *v > 0.0) {
                action = set(mode, v, rel);
            }
            ui.label(if rel { "%" } else { "units" });
        });
        if ui.radio(rel, "Set Size Relative to Screen").clicked() && !rel {
            action = set(mode, size, true);
        }
        if ui.radio(!rel, "Set Size in Absolute Units").clicked() && rel {
            action = set(mode, size, false);
        }
    });
    action
}

// ---------- Text Style, Table Style and Multileader Style ----------

/// One editable style property.
enum Field {
    Num(&'static str, &'static str, f64),
    Bool(&'static str, &'static str, bool),
    Choice(&'static str, &'static str, String, Vec<String>),
    /// A block-or-none choice (MLEADERSTYLE's default content block): `None` shows as
    /// `"(MText)"` and is sent as JSON `null`; `Some(name)` is sent as that block name.
    BlockChoice(&'static str, &'static str, Option<String>, Vec<String>),
    /// A colour field (DIMCLRD/DIMCLRE/DIMCLRT): the Layer Manager's own colour swatch popup
    /// (ACI palette + true colour), reused as-is via `layers::color_button`.
    Color(&'static str, &'static str, cadcraft_color::Color),
    /// An arrowhead/leader block name: the empty/default value (closed filled or inherit
    /// DIMBLK for per-end arrows) is shown as `none_label` and sent as JSON `""` (unlike
    /// `BlockChoice`'s MText default, which is `null`).
    ArrowBlock(&'static str, &'static str, &'static str, String, Vec<String>),
    /// A linetype name (DIMLTYPE/DIMLTEX1/DIMLTEX2): `""` is ByBlock. Reuses the Layer Manager's
    /// own linetype preview popup via `layers::linetype_button` (no Load submenu here — only
    /// linetypes already loaded in the drawing are offered, same as the Properties palette).
    Linetype(&'static str, &'static str, String, Vec<cadcraft_doc::Linetype>),
    /// A lineweight (DIMLWD/DIMLWE): ByLayer/ByBlock/Default/0..=211 (1/100 mm), reusing the
    /// Layer Manager's own lineweight popup via `layers::lineweight_button`.
    Weight(&'static str, &'static str, cadcraft_doc::Lineweight),
}

impl Field {
    /// The immediate-apply style manager's wire value, shared by picker commits and tests.
    fn picker_value(&self) -> Option<(&'static str, Value)> {
        match self {
            Self::Color(key, _, color) => Some((key, json!(color.name()))),
            Self::ArrowBlock(key, _, _, name, _) | Self::Linetype(key, _, name, _) => Some((key, json!(name))),
            Self::Weight(key, _, weight) => Some((key, json!(weight.to_dxf()))),
            _ => None,
        }
    }
}

fn style_change(cmd: &'static str, name: &str, key: &str, value: Value) -> (&'static str, Value) {
    (cmd, json!({ "name": name, key: value, "current": false }))
}

struct Style {
    name: String,
    fields: Vec<Field>,
}

fn text_style(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) -> Action {
    let Ok(d) = app.session.doc() else {
        no_drawing(ctx, "Text Style", open);
        return None;
    };
    let mut styles: Vec<Style> = d
        .text_styles
        .iter()
        .map(|s| Style {
            name: s.name.clone(),
            fields: vec![
                Field::Num("height", "Height (0 = ask)", s.height),
                Field::Num("widthFactor", "Width factor", s.width_factor),
                Field::Bool("backwards", "Backwards", s.backwards),
                Field::Bool("upsideDown", "Upside down", s.upside_down),
            ],
        })
        .collect();
    if styles.is_empty() {
        let s = cadcraft_doc::TextStyle::default();
        styles.push(Style { name: s.name, fields: Vec::new() });
    }
    let current = d.header.str("TEXTSTYLE", "Standard");
    style_manager(ctx, open, "Text Style", "style", &current, &styles)
}

fn table_style(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) -> Action {
    let Ok(d) = app.session.doc() else {
        no_drawing(ctx, "Table Style", open);
        return None;
    };
    let mut styles: Vec<Style> = d
        .table_styles
        .iter()
        .map(|s| Style {
            name: s.name.clone(),
            fields: vec![
                Field::Num("textHeight", "Text height", s.text_height),
                Field::Num("margin", "Cell margin", s.margin),
                Field::Bool("title", "Title row", s.title),
                Field::Bool("header", "Header row", s.header),
            ],
        })
        .collect();
    if styles.is_empty() {
        let s = cadcraft_doc::TableStyle::default();
        styles.push(Style { name: s.name, fields: Vec::new() });
    }
    let current = d.header.str("CTABLESTYLE", "Standard");
    style_manager(ctx, open, "Table Style", "tablestyle", &current, &styles)
}

fn mleader_style(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) -> Action {
    let Ok(d) = app.session.doc() else {
        no_drawing(ctx, "Multileader Style", open);
        return None;
    };
    let text_styles: Vec<String> = d.text_styles.iter().map(|s| s.name.clone()).collect();
    let block_names: Vec<String> = d.blocks.values().filter(|b| !b.name.starts_with('*')).map(|b| b.name.clone()).collect();
    let mut styles: Vec<Style> = d
        .mleader_styles
        .iter()
        .map(|s| Style {
            name: s.name.clone(),
            fields: vec![
                Field::Num("arrowSize", "Arrowhead size", s.arrow_size),
                Field::Num("textHeight", "Text height", s.text_height),
                Field::Num("landingGap", "Landing gap", s.landing_gap),
                Field::Num("dogleg", "Landing length", s.dogleg),
                Field::Choice("textStyle", "Text style", s.text_style.clone(), text_styles.clone()),
                Field::Bool("annotative", "Annotative", s.annotative),
                Field::BlockChoice("block", "Default content", s.block.clone(), block_names.clone()),
                Field::Num("blockScale", "Block content scale", s.block_scale),
                Field::Bool("blockCenter", "Attach block centre (not insertion point)", s.block_center),
            ],
        })
        .collect();
    if styles.is_empty() {
        let s = cadcraft_doc::MLeaderStyle::default();
        styles.push(Style { name: s.name, fields: Vec::new() });
    }
    let current = d.header.str("CMLEADERSTYLE", "Standard");
    style_manager(ctx, open, "Multileader Style", "mleaderstyle", &current, &styles)
}

/// Numeric (incl. 0/255 small-integer) field, label, DimStyle field accessor.
type DimField = (&'static str, &'static str, fn(&cadcraft_doc::DimStyle) -> f64);
/// Fields shown as plain numbers. The dialog remains a flat list, not the full seven-tab
/// Lines/Symbols/Text/Fit/Primary/Alternate/Tolerances layout; enum dropdowns are still pending.
const DIM_NUM_FIELDS: &[DimField] = &[
    ("scale", "Overall scale (DIMSCALE)", |s| s.scale),
    ("arrowSize", "Arrowhead size", |s| s.arrow_size),
    ("extOffset", "Extension line offset from origin", |s| s.ext_offset),
    ("extExtend", "Extension line extend beyond dim line", |s| s.ext_extend),
    ("textHeight", "Text height", |s| s.text_height),
    ("textGap", "Text gap", |s| s.text_gap),
    ("decimals", "Decimal places", |s| f64::from(s.decimals)),
    ("angularDecimals", "Angular decimal places (-1 = match linear)", |s| f64::from(s.angular_decimals)),
    ("linearFactor", "Linear scale factor (DIMLFAC)", |s| s.linear_factor),
    ("textAbove", "Text position (0 centred, 1 above)", |s| f64::from(s.text_above)),
    ("tickSize", "Tick size (0 = arrowheads)", |s| s.tick_size),
    ("centerMark", "Centre mark size (negative = lines)", |s| s.center_mark),
    ("zeroSuppression", "Zero suppression bits (DIMZIN)", |s| f64::from(s.zero_suppression)),
    ("linearUnit", "Linear unit format (1 sci…6 Windows)", |s| f64::from(s.linear_unit)),
    ("tolPlus", "Tolerance plus", |s| s.tol_plus),
    ("tolMinus", "Tolerance minus", |s| s.tol_minus),
    ("baselineSpacing", "Baseline spacing", |s| s.baseline_spacing),
    ("dimLineExtend", "Dimension line extension", |s| s.dim_line_extend),
    ("textJust", "Text justification (DIMJUST)", |s| f64::from(s.text_just)),
    ("round", "Rounding increment (0 = none)", |s| s.round),
    ("fractionFormat", "Fraction format (0 horiz, 1 diag, 2 none)", |s| f64::from(s.fraction_format)),
    ("tolDecimals", "Tolerance decimal places", |s| f64::from(s.tol_decimals)),
    ("tolScale", "Tolerance text height factor", |s| s.tol_scale),
    ("tolZeroSuppression", "Tolerance zero suppression", |s| f64::from(s.tol_zero_suppression)),
    ("altTolDecimals", "Alternate tolerance decimal places", |s| f64::from(s.alt_tol_decimals)),
    ("altTolZeroSuppression", "Alternate tolerance zero suppression", |s| f64::from(s.alt_tol_zero_suppression)),
    ("altFactor", "Alternate units factor", |s| s.alt_factor),
    ("altDecimals", "Alternate units decimal places", |s| f64::from(s.alt_decimals)),
    ("altRound", "Alternate units rounding (0 = none)", |s| s.alt_round),
    ("altUnit", "Alternate unit format", |s| f64::from(s.alt_unit)),
    ("altZeroSuppression", "Alternate units zero suppression", |s| f64::from(s.alt_zero_suppression)),
    ("angularUnit", "Angular unit format (0 dec…3 radians)", |s| f64::from(s.angular_unit)),
    ("fit", "What moves outside ext lines (DIMATFIT)", |s| f64::from(s.fit)),
    ("textVerticalPos", "Text vertical position (text heights)", |s| s.text_vertical_pos),
    ("tolJust", "Tolerance vertical place (0 bottom…2 top)", |s| f64::from(s.tol_just)),
    ("angularZeroSuppression", "Angular zero suppression bits", |s| f64::from(s.angular_zero_suppression)),
    ("arcSymbol", "Arc length symbol (0 before, 1 above, 2 none)", |s| f64::from(s.arc_symbol)),
    ("fixedExtLength", "Fixed extension line length", |s| s.fixed_ext_length),
    ("jogAngle", "Jog angle (degrees, radius jog dimensions)", |s| s.jog_angle),
];
/// Boolean fields shown as checkboxes.
const DIM_BOOL_FIELDS: &[(&str, &str, fn(&cadcraft_doc::DimStyle) -> bool)] = &[
    ("textInsideHorizontal", "Text inside extension lines horizontal", |s| s.text_inside_horizontal),
    ("textOutsideHorizontal", "Text outside extension lines horizontal", |s| s.text_outside_horizontal),
    ("tolerance", "Show tolerances", |s| s.tolerance),
    ("limits", "Show limits instead of the measurement", |s| s.limits),
    ("annotative", "Annotative", |s| s.annotative),
    ("suppressExt1", "Suppress extension line 1", |s| s.suppress_ext1),
    ("suppressExt2", "Suppress extension line 2", |s| s.suppress_ext2),
    ("suppressDim1", "Suppress dimension line 1 and its arrow", |s| s.suppress_dim1),
    ("suppressDim2", "Suppress dimension line 2 and its arrow", |s| s.suppress_dim2),
    ("textInside", "Text always between extension lines", |s| s.text_inside),
    ("suppressOutsideArrows", "Suppress arrowheads that don't fit inside", |s| s.suppress_outside_arrows),
    ("forceLineInside", "Dimension line forced between extension lines", |s| s.force_line_inside),
    ("userPositionedText", "Cursor positions text during creation", |s| s.user_positioned_text),
    ("fixedExt", "Fixed-length extension lines", |s| s.fixed_ext),
    ("alt", "Show alternate units", |s| s.alt),
];
/// Colour fields (DIMCLRD/DIMCLRE/DIMCLRT): key, label, `DimStyle` field accessor.
type DimPickerField<T> = (&'static str, &'static str, fn(&cadcraft_doc::DimStyle) -> T);
type DimArrowField = (&'static str, &'static str, &'static str, fn(&cadcraft_doc::DimStyle) -> String);
const DIM_COLOR_FIELDS: &[DimPickerField<cadcraft_color::Color>] = &[
    ("dimLineColor", "Dimension line color (DIMCLRD)", |s| s.dim_line_color),
    ("extLineColor", "Extension line color (DIMCLRE)", |s| s.ext_line_color),
    ("textColor", "Text color (DIMCLRT)", |s| s.text_color),
];
/// Arrowhead/leader block fields (DIMBLK/DIMBLK1/DIMBLK2/DIMLDRBLK): key, label, none-label,
/// `DimStyle` field accessor. Empty per-end names inherit DIMBLK; other empty names use closed filled.
const DIM_ARROW_FIELDS: &[DimArrowField] = &[
    ("arrowBlock", "Arrowhead block (DIMBLK)", "(Closed filled)", |s| s.arrow_block.clone()),
    ("arrowBlock1", "Arrowhead 1 block (DIMBLK1)", "(Use DIMBLK)", |s| s.arrow_block1.clone()),
    ("arrowBlock2", "Arrowhead 2 block (DIMBLK2)", "(Use DIMBLK)", |s| s.arrow_block2.clone()),
    ("leaderArrowBlock", "Leader arrowhead block (DIMLDRBLK)", "(Closed filled)", |s| s.leader_arrow_block.clone()),
];
/// Linetype fields (DIMLTYPE/DIMLTEX1/DIMLTEX2): key, label, `DimStyle` field accessor. `""` is
/// ByBlock.
const DIM_LINETYPE_FIELDS: &[DimPickerField<String>] = &[
    ("dimLinetype", "Dimension line linetype (DIMLTYPE)", |s| s.dim_linetype.clone()),
    ("extLinetype1", "Extension line 1 linetype (DIMLTEX1)", |s| s.ext_linetype1.clone()),
    ("extLinetype2", "Extension line 2 linetype (DIMLTEX2)", |s| s.ext_linetype2.clone()),
];
/// Lineweight fields (DIMLWD/DIMLWE): key, label, `DimStyle` field accessor.
const DIM_LINEWEIGHT_FIELDS: &[DimPickerField<cadcraft_doc::Lineweight>] = &[
    ("dimLineweight", "Dimension line lineweight (DIMLWD)", |s| cadcraft_doc::Lineweight::from_dxf(s.dim_lineweight)),
    ("extLineweight", "Extension line lineweight (DIMLWE)", |s| cadcraft_doc::Lineweight::from_dxf(s.ext_lineweight)),
];

/// Build the dialog's underlying picker state directly from the drawing tables.
fn dimstyle_fields(s: &cadcraft_doc::DimStyle, text_styles: &[String], block_names: &[String], linetypes: &[cadcraft_doc::Linetype]) -> Vec<Field> {
    let mut fields: Vec<Field> = DIM_NUM_FIELDS.iter().map(|(key, label, get)| Field::Num(key, label, get(s))).collect();
    fields.push(Field::Choice("textStyle", "Text style", s.text_style.clone(), text_styles.to_vec()));
    fields.extend(DIM_BOOL_FIELDS.iter().map(|(key, label, get)| Field::Bool(key, label, get(s))));
    fields.extend(DIM_COLOR_FIELDS.iter().map(|(key, label, get)| Field::Color(key, label, get(s))));
    fields
        .extend(DIM_ARROW_FIELDS.iter().map(|(key, label, none_label, get)| Field::ArrowBlock(key, label, none_label, get(s), block_names.to_vec())));
    fields.extend(DIM_LINETYPE_FIELDS.iter().map(|(key, label, get)| Field::Linetype(key, label, get(s), linetypes.to_vec())));
    fields.extend(DIM_LINEWEIGHT_FIELDS.iter().map(|(key, label, get)| Field::Weight(key, label, get(s))));
    fields
}

fn dimstyle(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) -> Action {
    let Ok(d) = app.session.doc() else {
        no_drawing(ctx, "Dimension Style Manager", open);
        return None;
    };
    let text_styles: Vec<String> = d.text_styles.iter().map(|s| s.name.clone()).collect();
    let block_names: Vec<String> = d.blocks.values().filter(|b| !b.name.starts_with('*')).map(|b| b.name.clone()).collect();
    let linetypes: Vec<cadcraft_doc::Linetype> = d.linetypes.clone();
    let mut styles: Vec<Style> =
        d.dim_styles.iter().map(|s| Style { name: s.name.clone(), fields: dimstyle_fields(s, &text_styles, &block_names, &linetypes) }).collect();
    if styles.is_empty() {
        let s = cadcraft_doc::DimStyle::default();
        styles.push(Style { name: s.name, fields: Vec::new() });
    }
    let current = d.header.str("DIMSTYLE", "Standard");
    style_manager(ctx, open, "Dimension Style Manager", "dimstyle", &current, &styles)
}

/// A style list (set current, new) beside the selected style's properties. `cmd` takes
/// `{name, <field>: value, current?}`.
fn style_manager(ctx: &egui::Context, open: &mut bool, title: &str, cmd: &'static str, current: &str, styles: &[Style]) -> Action {
    let t = Tokens::get();
    let sel_key = format!("{cmd}_sel");
    let new_key = format!("{cmd}_new");
    let mut sel = temp::<String>(ctx, &sel_key).filter(|s| styles.iter().any(|x| x.name == *s)).unwrap_or_else(|| current.to_string());
    let mut new_name = temp::<String>(ctx, &new_key).unwrap_or_default();
    let mut action = None;
    egui::Window::new(title).open(open).default_size(vec2(520.0, 280.0)).resizable(true).show(ctx, |ui| {
        ui.label(RichText::new(format!("Current style: {current}")).color(t.text_dim));
        ui.separator();
        ui.columns(2, |cols| {
            let [left, right] = cols else { return };
            left.label(RichText::new("Styles").strong());
            egui::ScrollArea::vertical().id_salt((cmd, "list")).max_height(160.0).auto_shrink([false, true]).show(left, |ui| {
                for s in styles {
                    let mut label = RichText::new(&s.name);
                    if s.name.eq_ignore_ascii_case(current) {
                        label = label.strong();
                    }
                    if ui.selectable_label(s.name == sel, label).clicked() {
                        sel = s.name.clone();
                    }
                }
            });
            if left.add_enabled(!sel.eq_ignore_ascii_case(current), egui::Button::new("Set Current")).clicked() {
                action = Some((cmd, json!({ "name": sel })));
            }
            left.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut new_name).hint_text("New style name").desired_width(120.0)).on_hover_text("New style name");
                let name = new_name.trim().to_string();
                let ok = !name.is_empty() && !styles.iter().any(|s| s.name.eq_ignore_ascii_case(&name));
                if ui.add_enabled(ok, egui::Button::new("New")).clicked() {
                    action = Some((cmd, json!({ "name": name, "current": false })));
                    sel = name;
                    new_name.clear();
                }
            });
            right.label(RichText::new(format!("Properties of {sel}")).strong());
            let Some(style) = styles.iter().find(|s| s.name == sel) else { return };
            egui::ScrollArea::vertical().id_salt((cmd, "props_scroll")).max_height(400.0).auto_shrink([false, true]).show(right, |ui| {
                egui::Grid::new((cmd, "props")).num_columns(2).spacing(vec2(10.0, 6.0)).show(ui, |ui| {
                    for f in &style.fields {
                        let change = match f {
                            Field::Num(key, label, v) => {
                                // Not clamped to >= 0 here: some fields are meaningfully negative
                                // (DimStyle's centre mark size, angular decimals' -1 sentinel, text
                                // vertical position) and the engine's own command validates range
                                // per field anyway (reported on the command line if rejected).
                                let field_label = ui.label(*label);
                                num_field(ui, (cmd, &style.name, *key), *v, Some(field_label.id)).map(|v| (*key, json!(v)))
                            }
                            Field::Bool(key, label, v) => {
                                ui.label("");
                                let mut b = *v;
                                ui.checkbox(&mut b, *label).changed().then(|| (*key, json!(b)))
                            }
                            Field::Choice(key, label, v, choices) => {
                                ui.label(*label);
                                let mut picked = None;
                                egui::ComboBox::from_id_salt((cmd, *key)).selected_text(v.as_str()).show_ui(ui, |ui| {
                                    for c in choices {
                                        if ui.selectable_label(c == v, c).clicked() && c != v {
                                            picked = Some(c.clone());
                                        }
                                    }
                                });
                                picked.map(|c| (*key, json!(c)))
                            }
                            Field::BlockChoice(key, label, v, choices) => {
                                const NONE_LABEL: &str = "(MText)";
                                ui.label(*label);
                                let shown = v.as_deref().unwrap_or(NONE_LABEL);
                                let mut picked = None;
                                egui::ComboBox::from_id_salt((cmd, *key)).selected_text(shown).show_ui(ui, |ui| {
                                    if ui.selectable_label(v.is_none(), NONE_LABEL).clicked() && v.is_some() {
                                        picked = Some(None);
                                    }
                                    for c in choices {
                                        if ui.selectable_label(v.as_deref() == Some(c.as_str()), c).clicked() && v.as_deref() != Some(c.as_str()) {
                                            picked = Some(Some(c.clone()));
                                        }
                                    }
                                });
                                picked.map(|c| (*key, c.map_or(Value::Null, |c| json!(c))))
                            }
                            Field::Color(key, label, v) => {
                                ui.label(*label);
                                let id = ui.make_persistent_id((cmd, &style.name, *key));
                                crate::layers::color_button(ui, id, *v, true, 140.0).and_then(|c| Field::Color(key, label, c).picker_value())
                            }
                            Field::ArrowBlock(key, label, none_label, v, choices) => {
                                let field_label = ui.label(*label);
                                let shown = if v.is_empty() { none_label } else { v.as_str() };
                                let mut picked = None;
                                let combo = egui::ComboBox::from_id_salt((cmd, &style.name, *key)).selected_text(shown).show_ui(ui, |ui| {
                                    if ui.selectable_label(v.is_empty(), *none_label).clicked() && !v.is_empty() {
                                        picked = Some(String::new());
                                    }
                                    for c in choices {
                                        if ui.selectable_label(v == c, c).clicked() && v != c {
                                            picked = Some(c.clone());
                                        }
                                    }
                                });
                                combo.response.labelled_by(field_label.id);
                                picked.and_then(|c| Field::ArrowBlock(key, label, none_label, c, Vec::new()).picker_value())
                            }
                            Field::Linetype(key, label, v, linetypes) => {
                                ui.label(*label);
                                let id = ui.make_persistent_id((cmd, &style.name, *key));
                                match crate::layers::linetype_button(ui, id, v, linetypes, &[], Some("ByBlock"), 160.0) {
                                    Some(crate::layers::LinetypePick::Name(n)) => Field::Linetype(key, label, n, Vec::new()).picker_value(),
                                    // No library passed in, so Load variants never fire here.
                                    _ => None,
                                }
                            }
                            Field::Weight(key, label, v) => {
                                ui.label(*label);
                                let id = ui.make_persistent_id((cmd, &style.name, *key));
                                crate::layers::lineweight_button(ui, id, *v, true, 140.0).and_then(|lw| Field::Weight(key, label, lw).picker_value())
                            }
                        };
                        if let Some((key, v)) = change {
                            action = Some(style_change(cmd, &style.name, key, v));
                        }
                        ui.end_row();
                    }
                });
            });
        });
    });
    set_temp(ctx, &sel_key, sel);
    set_temp(ctx, &new_key, new_name);
    action
}

// ---------- Page Setup Manager ----------

/// Page Setup Manager over layouts sorted by tab order; returns the selected setup command.
fn page_setup(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) -> Action {
    let t = Tokens::get();
    let Ok(d) = app.session.doc() else {
        no_drawing(ctx, "Page Setup Manager", open);
        return None;
    };
    let mut layouts: Vec<(u32, String, cadcraft_doc::PageSetup)> = d.layouts.iter().map(|l| (l.tab_order, l.name.clone(), l.page.clone())).collect();
    layouts.sort_by_key(|l| l.0);
    let shown = app.session.layout_space().layout().map(str::to_string);
    let mut sel = temp::<String>(ctx, "pagesetup_sel")
        .filter(|s| layouts.iter().any(|l| l.1 == *s))
        .or(shown)
        .or_else(|| layouts.first().map(|l| l.1.clone()))
        .unwrap_or_default();
    let mut action = None;
    let choices = PageChoices::new(ctx, d);
    egui::Window::new("Page Setup Manager").open(open).default_size(vec2(560.0, 300.0)).resizable(true).show(ctx, |ui| {
        if layouts.is_empty() {
            ui.label("This drawing has no layouts.");
            return;
        }
        ui.columns(2, |cols| {
            let [left, right] = cols else { return };
            left.label(RichText::new("Layouts").strong());
            egui::ScrollArea::vertical().id_salt("pagesetup_list").max_height(200.0).auto_shrink([false, true]).show(left, |ui| {
                for (_, name, _) in &layouts {
                    if ui.selectable_label(*name == sel, name).clicked() {
                        sel = name.clone();
                    }
                }
            });
            let Some((_, name, page)) = layouts.iter().find(|l| l.1 == sel) else { return };
            right.label(RichText::new(format!("Page setup of {name}")).strong());
            // Layout plotting always prints the whole sheet, so only that area is offered.
            let rows = PageRows { areas: &["layout"], scale: true, center: true };
            egui::Grid::new("pagesetup_grid").num_columns(2).spacing(vec2(10.0, 6.0)).show(right, |ui| {
                if let Some((k, v)) = page_rows(ui, egui::Id::unique(("pagesetup", name)), page, &rows, &choices, None) {
                    action = Some(("pagesetup", json!({ "layout": name, k: v })));
                }
            });
            let (w, h) = if page.landscape { (page.height_mm, page.width_mm) } else { (page.width_mm, page.height_mm) };
            right.label(RichText::new(format!("{w:.1} × {h:.1} mm")).color(t.text_dim));
        });
    });
    set_temp(ctx, "pagesetup_sel", sel);
    action
}

/// Which page-setup rows a dialog shows.
struct PageRows {
    /// Plot area choices (`plotArea` values).
    areas: &'static [&'static str],
    scale: bool,
    center: bool,
}

/// The devices and plot style tables the page-setup rows offer.
struct PageChoices {
    devices: std::sync::Arc<Vec<cadcraft_doc::PlotterConfig>>,
    styles: Vec<String>,
}

impl PageChoices {
    fn new(ctx: &egui::Context, d: &cadcraft_doc::Drawing) -> Self {
        PageChoices { devices: device_choices(ctx), styles: cadcraft_doc::plot_style_table_names(d) }
    }
}

/// The plotter devices (built-in, then the user's files), read again at most every 2 s: the
/// dialogs ask every frame and user devices are files.
fn device_choices(ctx: &egui::Context) -> std::sync::Arc<Vec<cadcraft_doc::PlotterConfig>> {
    type Cached = (f64, std::sync::Arc<Vec<cadcraft_doc::PlotterConfig>>);
    let now = crate::now_ms();
    if let Some((at, list)) = temp::<Cached>(ctx, "plot_devices")
        && now - at < 2000.0
    {
        return list;
    }
    let list = std::sync::Arc::new(cadcraft_engine::cmd::plotters::devices());
    set_temp::<Cached>(ctx, "plot_devices", (now, list.clone()));
    list
}

/// A page setup's device, or `"None"` (plot to PDF on the page's paper).
fn device_label(page: &cadcraft_doc::PageSetup) -> &str {
    if page.device.trim().is_empty() { "None" } else { page.device.as_str() }
}

/// A page setup's plot style table, or `"None"`.
fn style_label(page: &cadcraft_doc::PageSetup) -> &str {
    if cadcraft_doc::no_plot_style_table(&page.plot_style_table) { "None" } else { page.plot_style_table.as_str() }
}

fn area_label(a: &str) -> &str {
    match a {
        "layout" => "Layout",
        "extents" => "Extents",
        "display" => "Display",
        "limits" => "Limits",
        "window" => "Window",
        other => other,
    }
}

/// A plot scale typed as a number (paper units per drawing unit) or a ratio such as `1:50`.
fn parse_ratio(t: &str) -> Option<f64> {
    let v = match t.trim().split_once([':', '/']) {
        Some((a, b)) => a.trim().parse::<f64>().ok()? / b.trim().parse::<f64>().ok()?,
        None => t.trim().trim_end_matches(['x', 'X']).parse().ok()?,
    };
    (v.is_finite() && v > 0.0).then_some(v)
}

/// The page-setup rows (device, paper, orientation, plot area, fit, scale, center, lineweights,
/// transparency, plot style table) shared by the Page Setup Manager and the Plot dialog, inside a
/// two-column grid. Returns the setting the user changed as a `pagesetup` parameter (`device`,
/// `paper`, `landscape`, `plotArea`, `scaleToFit`, `scale`, `center`, `lineweights`,
/// `plotTransparency` or `plotStyleTable`).
fn page_rows(
    ui: &mut egui::Ui,
    salt: egui::Id,
    page: &cadcraft_doc::PageSetup,
    rows: &PageRows,
    choices: &PageChoices,
    placement: Option<(&mut bool, &mut [f64; 2], bool)>,
) -> Option<(&'static str, Value)> {
    let mut change = None;
    ui.label("Printer/plotter");
    egui::ComboBox::from_id_salt(salt.with("device")).selected_text(device_label(page)).width(190.0).show_ui(ui, |ui| {
        if ui.selectable_label(device_label(page) == "None", "None").on_hover_text("PDF on the paper chosen below").clicked()
            && device_label(page) != "None"
        {
            change = Some(("device", json!("None")));
        }
        for dev in choices.devices.iter() {
            let on = dev.name.eq_ignore_ascii_case(&page.device);
            if ui.selectable_label(on, &dev.name).on_hover_text(format!("{} ({})", dev.description, dev.output.as_str().to_uppercase())).clicked()
                && !on
            {
                change = Some(("device", json!(dev.name)));
            }
        }
    });
    ui.end_row();
    // The device's papers, else the standard sizes.
    let device = choices.devices.iter().find(|dv| dv.name.eq_ignore_ascii_case(&page.device));
    ui.label("Paper size");
    egui::ComboBox::from_id_salt(salt.with("paper")).selected_text(page.paper.as_str()).width(190.0).show_ui(ui, |ui| match device {
        Some(dev) => {
            for p in &dev.papers {
                if ui.selectable_label(p.name == page.paper, &p.name).clicked() && p.name != page.paper {
                    change = Some(("paper", json!(p.name)));
                }
            }
        }
        None => {
            for p in cadcraft_render::PAPER_SIZES {
                if ui.selectable_label(p.name == page.paper, p.name).clicked() && p.name != page.paper {
                    change = Some(("paper", json!(p.name)));
                }
            }
        }
    });
    ui.end_row();
    ui.label("Orientation");
    ui.horizontal(|ui| {
        if ui.radio(!page.landscape, "Portrait").clicked() && page.landscape {
            change = Some(("landscape", json!(false)));
        }
        if ui.radio(page.landscape, "Landscape").clicked() && !page.landscape {
            change = Some(("landscape", json!(true)));
        }
    });
    ui.end_row();
    ui.label("Plot area");
    ui.add_enabled_ui(rows.areas.len() > 1, |ui| {
        egui::ComboBox::from_id_salt(salt.with("area")).selected_text(area_label(&page.plot_area)).show_ui(ui, |ui| {
            for a in rows.areas {
                if ui.selectable_label(page.plot_area == *a, area_label(a)).clicked() && page.plot_area != *a {
                    change = Some(("plotArea", json!(a)));
                }
            }
        });
    });
    ui.end_row();
    ui.label("");
    let mut fit = page.scale_to_fit;
    if ui.checkbox(&mut fit, "Fit to paper").changed() {
        change = Some(("scaleToFit", json!(fit)));
    }
    ui.end_row();
    if let Some((center, offset_mm, supported)) = placement {
        ui.label("Plot offset");
        ui.add_enabled_ui(supported, |ui| ui.checkbox(center, "Center the plot")).response.on_hover_text(if supported {
            "Center the plotted model bounds on the printable area"
        } else {
            "Model-space offset placement is not supported by SVG"
        });
        ui.end_row();
        for (i, label) in [(0, "X offset (mm)"), (1, "Y offset (mm)")] {
            let field_label = ui.label(label);
            ui.add_enabled_ui(supported && !*center, |ui| {
                if let Some(v) = num_field(ui, ("plot_offset", i), offset_mm[i], Some(field_label.id)) {
                    offset_mm[i] = v;
                }
            })
            .response
            .on_hover_text("Distance from the printable area's lower-left corner; values range from −100000 to 100000 mm");
            ui.end_row();
        }
        if supported && !*center && offset_mm.iter().any(|v| !v.is_finite() || v.abs() > cadcraft_render::paper::MAX_PLOT_OFFSET_MM) {
            ui.label("");
            ui.colored_label(Tokens::get().warn, "Offsets must be within ±100000 mm");
            ui.end_row();
        }
    }
    if rows.scale {
        let label = ui.label("Scale");
        ui.add_enabled_ui(!page.scale_to_fit, |ui| {
            if let Some(v) = expr_field(ui, salt.with("scale"), &fmt_num(page.scale), Some(label.id)).and_then(|t| parse_ratio(&t)) {
                change = Some(("scale", json!(v)));
            }
        })
        .response
        .on_hover_text("Paper units per drawing unit, or a ratio such as 1:50");
        ui.end_row();
    }
    if rows.center {
        ui.label("");
        let mut center = page.center;
        if ui.checkbox(&mut center, "Center the plot").changed() {
            change = Some(("center", json!(center)));
        }
        ui.end_row();
    }
    ui.label("");
    let mut lw = page.lineweights;
    if ui.checkbox(&mut lw, "Plot object lineweights").changed() {
        change = Some(("lineweights", json!(lw)));
    }
    ui.end_row();
    ui.label("");
    let mut transparency = page.plot_transparency;
    if ui
        .checkbox(&mut transparency, "Plot transparency")
        .on_hover_text("Plot objects with their transparency (PLOTTRANSPARENCYOVERRIDE can override it)")
        .changed()
    {
        change = Some(("plotTransparency", json!(transparency)));
    }
    ui.end_row();
    ui.label("Plot style table");
    egui::ComboBox::from_id_salt(salt.with("styles")).selected_text(style_label(page)).width(190.0).show_ui(ui, |ui| {
        if ui.selectable_label(style_label(page) == "None", "None").clicked() && style_label(page) != "None" {
            change = Some(("plotStyleTable", json!("None")));
        }
        for name in &choices.styles {
            let on = name.eq_ignore_ascii_case(&page.plot_style_table);
            if ui.selectable_label(on, name).clicked() && !on {
                change = Some(("plotStyleTable", json!(name)));
            }
        }
    });
    ui.end_row();
    change
}

/// Apply a [`page_rows`] change to a page setup held by a dialog.
/// Device and paper changes follow PAGESETUP's rules (`plotters::page_device`): a device brings
/// its papers, orientation and printable areas.
fn apply_page_change(page: &mut cadcraft_doc::PageSetup, key: &str, v: &Value) {
    use cadcraft_engine::cmd::plotters::page_device;
    match key {
        "device" => {
            let _ = page_device("plot", &json!({ "device": v }), page);
        }
        "paper" => {
            // A device's paper, else a standard size (no device).
            if !page_device("plot", &json!({ "paper": v }), page).unwrap_or(false)
                && let Some(ps) = v.as_str().and_then(cadcraft_render::paper_size)
            {
                page.paper = ps.name.into();
                page.width_mm = ps.width_mm;
                page.height_mm = ps.height_mm;
            }
        }
        "plotStyleTable" => page.plot_style_table = v.as_str().unwrap_or_default().chars().take(260).collect(),
        "plotArea" => page.plot_area = v.as_str().unwrap_or("extents").into(),
        "scale" => page.scale = v.as_f64().filter(|s| s.is_finite() && *s > 0.0).unwrap_or(page.scale),
        _ => {
            let b = v.as_bool().unwrap_or_default();
            match key {
                "landscape" => page.landscape = b,
                "scaleToFit" => page.scale_to_fit = b,
                "center" => page.center = b,
                "lineweights" => page.lineweights = b,
                "plotTransparency" => page.plot_transparency = b,
                _ => {}
            }
        }
    }
}

// ---------- Plot ----------

/// Bumped each time PLOT opens its dialog, so it starts again from the page setup.
static PLOT_GEN: AtomicU32 = AtomicU32::new(0);

/// Set while PLOTWINDOW picks the Plot dialog's window ("Window <").
const PICKING_WINDOW: &str = "plot_picking_window";
/// Set by Print... in a preview opened from the Plot dialog: plot as soon as the dialog is back.
pub(crate) const PLOT_NOW: &str = "plot_now";

/// Every frame: once PLOTWINDOW (started by the Plot dialog's "Window <") ends, bring the dialog
/// back with its settings, plotting the picked window (unchanged if the pick was cancelled).
pub fn plot_window_resume(app: &mut CadApp, ctx: &egui::Context) {
    if temp::<bool>(ctx, PICKING_WINDOW) != Some(true) || app.session.running.is_some() {
        return;
    }
    set_temp(ctx, PICKING_WINDOW, false);
    if let (Some(w), Some(mut st)) = (app.session.plot_window.take(), temp::<PlotState>(ctx, "plot_state")) {
        st.window = [w.min.x, w.min.y, w.max.x, w.max.y];
        st.page.plot_area = "window".into();
        set_temp(ctx, "plot_state", st);
    }
    app.ui.dialog = Some("plot".into());
}

/// Set a flag the Plot dialog reads (see [`PLOT_NOW`]).
pub(crate) fn set_flag(ctx: &egui::Context, id: &str, on: bool) {
    set_temp(ctx, id, on);
}

/// Plot areas for model space (a layout always plots its sheet).
const MODEL_AREAS: &[&str] = &["extents", "display", "limits", "window"];

/// The Plot dialog's settings until OK.
#[derive(Clone)]
struct PlotState {
    generation: u32,
    /// "Model" or a layout name.
    target: String,
    page: cadcraft_doc::PageSetup,
    /// Plot area "window" corners in drawing units: [x1, y1, x2, y2].
    window: [f64; 4],
    /// Model-space placement; stays outside PageSetup so layouts keep their existing behavior.
    center: bool,
    offset_mm: [f64; 2],
}

/// The page setup a plot of `target` starts from: the layout's, or model-space defaults (fitted
/// extents on A4 in metric drawings, ANSI A otherwise).
fn plot_page(d: &cadcraft_doc::Drawing, target: &str) -> cadcraft_doc::PageSetup {
    if let Some(l) = d.layouts.iter().find(|l| l.name == target) {
        let mut page = l.page.clone();
        page.plot_area = "layout".into();
        return page;
    }
    let mut page = cadcraft_doc::PageSetup { plot_area: "extents".into(), scale_to_fit: true, ..Default::default() };
    if cadcraft_render::paper::paper_unit_mm(d) == 1.0
        && let Some(a4) = cadcraft_render::paper_size("A4")
    {
        page.paper = a4.name.into();
        page.width_mm = a4.width_mm;
        page.height_mm = a4.height_mm;
    }
    page
}

/// The `plot` parameters for the dialog's settings (without the output path). `preview_dpi`
/// asks for a PNG at that resolution instead of the device's output, on the same sheet.
fn plot_params(st: &PlotState, preview_dpi: Option<f64>) -> Value {
    let page = &st.page;
    let mut p = json!({
        "layout": st.target,
        "landscape": page.landscape,
        "fit": page.scale_to_fit,
        "lineweights": page.lineweights,
        "plotTransparency": page.plot_transparency,
    });
    if let Some(o) = p.as_object_mut() {
        let device = device_label(page) != "None";
        let output = cadcraft_engine::cmd::plotters::find_device(&page.device).map_or("pdf", |d| d.output.as_str());
        match preview_dpi {
            // The page's own sheet, so a device's paper and printable area show as they plot.
            Some(dpi) => {
                o.insert("device".into(), json!("None"));
                o.insert("output".into(), json!("png"));
                o.insert("resolution".into(), json!(dpi));
                o.insert("width".into(), json!(page.width_mm));
                o.insert("height".into(), json!(page.height_mm));
                o.insert("margins".into(), json!(page.margins_mm));
            }
            None if device => {
                o.insert("device".into(), json!(page.device));
                o.insert("paper".into(), json!(page.paper));
            }
            None if cadcraft_render::paper_size(&page.paper).is_some() => {
                o.insert("device".into(), json!("None"));
                o.insert("paper".into(), json!(page.paper));
            }
            None => {
                o.insert("device".into(), json!("None"));
                o.insert("width".into(), json!(page.width_mm));
                o.insert("height".into(), json!(page.height_mm));
            }
        }
        if style_label(page) != "None" {
            o.insert("plotStyleTable".into(), json!(page.plot_style_table));
        }
        if st.target == "Model" {
            o.insert("plotArea".into(), json!(page.plot_area));
            if output != "svg" {
                o.insert("center".into(), json!(st.center));
                if !st.center {
                    o.insert("offset".into(), json!(st.offset_mm));
                }
            }
            if !page.scale_to_fit {
                o.insert("scale".into(), json!(page.scale));
            }
            if page.plot_area == "window" {
                let [x1, y1, x2, y2] = st.window;
                o.insert("window".into(), json!([[x1, y1], [x2, y2]]));
            }
        }
    }
    p
}

/// The Plot dialog: what to plot, on which paper and at what scale; OK asks where to save the PDF
/// (where the host has a file dialog) and runs `plot` with those settings.
fn plot(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) -> Action {
    let t = Tokens::get();
    let Ok(d) = app.session.doc() else {
        no_drawing(ctx, "Plot", open);
        return None;
    };
    let mut targets = vec!["Model".to_string()];
    let mut layouts: Vec<(u32, String)> = d.layouts.iter().map(|l| (l.tab_order, l.name.clone())).collect();
    layouts.sort_by_key(|l| l.0);
    targets.extend(layouts.into_iter().map(|l| l.1));
    let generation = PLOT_GEN.load(Ordering::Relaxed);
    let mut st = temp::<PlotState>(ctx, "plot_state").filter(|s| s.generation == generation && targets.contains(&s.target)).unwrap_or_else(|| {
        let target = app.session.layout_space().tab_name().to_string();
        let mut ext = d.extents(&cadcraft_doc::Space::Model);
        if ext.is_empty() {
            ext = cadcraft_geom::Bounds2::new(cadcraft_geom::Vec2::ZERO, cadcraft_geom::Vec2::new(12.0, 9.0));
        }
        PlotState {
            generation,
            page: plot_page(d, &target),
            target,
            window: [ext.min.x, ext.min.y, ext.max.x, ext.max.y],
            center: true,
            offset_mm: [0.0, 0.0],
        }
    });
    let model = st.target == "Model";
    let choices = PageChoices::new(ctx, d);
    let offset_supported =
        choices.devices.iter().find(|dev| dev.name.eq_ignore_ascii_case(&st.page.device)).is_none_or(|dev| dev.output.as_str() != "svg");
    // Print... in a preview opened from here plots with these settings.
    let mut ok = temp::<bool>(ctx, PLOT_NOW) == Some(true);
    set_temp(ctx, PLOT_NOW, false);
    let (mut cancel, mut action, mut pick, mut preview) = (false, None, false, false);
    let center = ctx.content_rect().center();
    egui::Window::new("Plot").open(open).collapsible(false).resizable(false).pivot(egui::Align2::CENTER_CENTER).default_pos(center).show(ctx, |ui| {
        egui::Grid::new("plot_target").num_columns(2).spacing(vec2(10.0, 6.0)).show(ui, |ui| {
            ui.label("Layout");
            egui::ComboBox::from_id_salt("plot_layout").selected_text(st.target.as_str()).width(190.0).show_ui(ui, |ui| {
                for n in &targets {
                    if ui.selectable_label(*n == st.target, n).clicked() && *n != st.target {
                        st.page = plot_page(d, n);
                        st.target = n.clone();
                    }
                }
            });
            ui.end_row();
        });
        ui.separator();
        ui.horizontal_top(|ui| {
            egui::Grid::new("plot_grid").num_columns(2).spacing(vec2(10.0, 6.0)).show(ui, |ui| {
                let rows = if model {
                    PageRows { areas: MODEL_AREAS, scale: true, center: false }
                } else {
                    PageRows { areas: &["layout"], scale: false, center: false }
                };
                let placement = model.then_some((&mut st.center, &mut st.offset_mm, offset_supported));
                if let Some((k, v)) = page_rows(ui, egui::Id::unique(("plot", st.target.as_str())), &st.page, &rows, &choices, placement) {
                    apply_page_change(&mut st.page, k, &v);
                }
                if model && st.page.plot_area == "window" {
                    for (i, label) in [(0, "First corner"), (2, "Opposite corner")] {
                        ui.label(label);
                        ui.horizontal(|ui| {
                            for (j, axis) in ["X", "Y"].iter().enumerate() {
                                let axis_label = ui.label(*axis);
                                let k = i + j;
                                if let Some(slot) = st.window.get_mut(k)
                                    && let Some(v) = num_field(ui, ("plot_window", k), *slot, Some(axis_label.id))
                                {
                                    *slot = v;
                                }
                            }
                        });
                        ui.end_row();
                    }
                    ui.label("");
                    pick = ui.button("Window <").on_hover_text("Pick the window's corners in the drawing").clicked();
                    ui.end_row();
                }
            });
            sheet_sketch(ui, &st.page, &t);
        });
        ui.separator();
        ui.horizontal(|ui| {
            if !model && ui.button("Apply to Layout").on_hover_text("Save these settings in the layout's page setup").clicked() {
                let page = &st.page;
                action = Some((
                    "pagesetup",
                    json!({
                        "layout": st.target,
                        "paper": page.paper,
                        "landscape": page.landscape,
                        "scaleToFit": page.scale_to_fit,
                        "lineweights": page.lineweights,
                        "plotTransparency": page.plot_transparency,
                    }),
                ));
            }
            preview = ui.button("Preview").on_hover_text("The sheet as these settings plot it").clicked();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                cancel = ui.button("Cancel").clicked();
                ok |= ui.button("OK").clicked();
            });
        });
    });
    // The output file type follows the device.
    let ext = choices.devices.iter().find(|dv| dv.name.eq_ignore_ascii_case(&st.page.device)).map_or("pdf", |dv| dv.output.as_str());
    set_temp(ctx, "plot_state", st.clone());
    if cancel {
        *open = false;
    }
    if pick {
        // The dialog comes back with these settings and the picked window (`plot_window_resume`).
        set_temp(ctx, PICKING_WINDOW, true);
        *open = false;
        app.start("plotwindow");
        return action;
    }
    if preview {
        // A sheet about 1600 pixels on its long side.
        let long_in = st.page.width_mm.max(st.page.height_mm).max(1.0) / 25.4;
        let mut info = format!("{} · {}", st.target, st.page.paper);
        if st.page.landscape {
            info.push_str(" · landscape");
        }
        crate::exchange::preview_plot(app, ctx, plot_params(&st, Some((1600.0 / long_in).clamp(50.0, 300.0))), info);
        return action;
    }
    if ok {
        let mut params = plot_params(&st, None);
        if let Some(pick) = app.services.pick_save.as_ref() {
            let title = app.session.state().map(|s| s.title.clone()).unwrap_or_default();
            let stem = std::path::Path::new(&title).file_stem().map(|s| s.to_string_lossy().to_string()).filter(|s| !s.is_empty());
            let suggested = format!("{}-{}.{ext}", stem.unwrap_or_else(|| "Drawing".into()), st.target);
            // Cancelling the file dialog keeps the Plot dialog open with its settings.
            let Some(path) = pick(&suggested) else { return action };
            if let Some(o) = params.as_object_mut() {
                o.insert("path".into(), json!(path));
            }
        }
        *open = false;
        match app.run("plot", params) {
            Ok(r) => {
                let msg = match r.get("path").and_then(Value::as_str) {
                    Some(path) => format!("Plotted {} to {path}", st.target),
                    None => format!("Plotted {} ({} bytes of PDF; there is no file dialog here)", st.target, r["bytes"]),
                };
                app.session.echo(msg);
            }
            // `run` already echoed the error; keep the dialog open to fix the settings.
            Err(_) => *open = true,
        }
    }
    action
}

/// The sheet in the chosen orientation with its printable area, and its size.
fn sheet_sketch(ui: &mut egui::Ui, page: &cadcraft_doc::PageSetup, t: &Tokens) {
    let (w, h) = if page.landscape { (page.height_mm, page.width_mm) } else { (page.width_mm, page.height_mm) };
    ui.vertical(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(120.0, 120.0), Sense::hover());
        let k = (110.0 / w.max(h).max(1.0)) as f32;
        let sheet = egui::Rect::from_center_size(rect.center(), vec2(w as f32 * k, h as f32 * k));
        ui.painter().rect_filled(sheet, 0.0, t.field);
        ui.painter().rect_stroke(sheet, 0.0, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
        let [l, b, r, top] = page.margins_mm.map(|m| m as f32 * k);
        let printable = egui::Rect::from_min_max(sheet.min + vec2(l, top), sheet.max - vec2(r, b));
        if printable.is_positive() {
            ui.painter().rect_stroke(printable, 0.0, Stroke::new(1.0, t.accent), egui::StrokeKind::Inside);
        }
        ui.label(RichText::new(format!("{w:.1} × {h:.1} mm")).color(t.text_dim));
    });
}

// ---------- Constraint Settings ----------

/// Constraint Settings dialog editing the drawing's parametric settings; returns the `constraintsettings` action.
fn constraint_settings(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) -> Action {
    let t = Tokens::get();
    let Ok(d) = app.session.doc() else {
        no_drawing(ctx, "Constraint Settings", open);
        return None;
    };
    let s = d.parametric.settings.clone();
    let types = cadcraft_doc::ParametricSettings::default().auto_types;
    let mut action = None;
    let set = |k: &str, v: Value| Some(("constraintsettings", json!({ k: v })));
    egui::Window::new("Constraint Settings").open(open).collapsible(false).resizable(false).show(ctx, |ui| {
        ui.heading("Geometric");
        let mut infer = s.infer;
        if ui.checkbox(&mut infer, "Infer geometric constraints").changed() {
            action = set("infer", json!(infer));
        }
        let mut bars = s.bars_visible;
        if ui.checkbox(&mut bars, "Show constraint bars").changed() {
            action = set("barsVisible", json!(bars));
        }
        ui.horizontal(|ui| {
            let label = ui.label("Constraint bar transparency (%)");
            if let Some(v) = num_field(ui, "csettings_transp", f64::from(s.bar_transparency), Some(label.id)) {
                action = set("barTransparency", json!(v));
            }
        });
        ui.separator();
        ui.heading("Dimensional");
        let mut dims = s.dims_visible;
        if ui.checkbox(&mut dims, "Show dynamic constraints").changed() {
            action = set("dimsVisible", json!(dims));
        }
        ui.separator();
        ui.heading("AutoConstrain");
        ui.label(RichText::new("Constraint types AUTOCONSTRAIN applies").color(t.text_dim));
        egui::Grid::new("csettings_types").num_columns(3).show(ui, |ui| {
            for (i, ty) in types.iter().enumerate() {
                let mut on = s.auto_types.iter().any(|a| a.eq_ignore_ascii_case(ty));
                if ui.checkbox(&mut on, ty.as_str()).changed() {
                    let list: Vec<&String> =
                        types.iter().filter(|x| if *x == ty { on } else { s.auto_types.iter().any(|a| a.eq_ignore_ascii_case(x)) }).collect();
                    action = set("autoTypes", json!(list));
                }
                if i % 3 == 2 {
                    ui.end_row();
                }
            }
        });
        egui::Grid::new("csettings_tol").num_columns(2).show(ui, |ui| {
            let label = ui.label("Distance tolerance");
            if let Some(v) = num_field(ui, "csettings_dist", s.distance_tolerance, Some(label.id)) {
                action = set("distanceTolerance", json!(v));
            }
            ui.end_row();
            let label = ui.label("Angle tolerance (degrees)");
            if let Some(v) = num_field(ui, "csettings_angle", s.angle_tolerance, Some(label.id)) {
                action = set("angleTolerance", json!(v));
            }
            ui.end_row();
        });
    });
    action
}

#[cfg(test)]
mod plot_dialog_tests {
    use super::*;
    use cadcraft_engine::Session;

    fn state(target: &str) -> PlotState {
        let d = cadcraft_doc::Drawing::new_metric();
        PlotState {
            generation: 0,
            page: plot_page(&d, target),
            target: target.into(),
            window: [0.0, 0.0, 10.0, 10.0],
            center: true,
            offset_mm: [0.0, 0.0],
        }
    }

    #[test]
    fn a_device_brings_its_papers_and_the_plot_names_them() {
        let mut st = state("Model");
        apply_page_change(&mut st.page, "device", &json!("CADCraft PNG"));
        assert_eq!(st.page.device, "CADCraft PNG");
        let dev = cadcraft_engine::cmd::plotters::find_device("CADCraft PNG").unwrap();
        assert!(dev.papers.iter().any(|p| p.name == st.page.paper), "a paper of the device: {}", st.page.paper);
        assert!(st.page.margins_mm.iter().any(|m| *m > 0.0), "the paper's printable area");
        apply_page_change(&mut st.page, "plotStyleTable", &json!("monochrome.ctb"));
        let p = plot_params(&st, None);
        assert_eq!((p["device"].as_str(), p["paper"].as_str()), (Some("CADCraft PNG"), Some(st.page.paper.as_str())));
        assert_eq!(p["plotStyleTable"], json!("monochrome.ctb"));
        // The preview plots the same sheet to PNG, whatever the device.
        let q = plot_params(&st, Some(150.0));
        assert_eq!((q["output"].as_str(), q["device"].as_str()), (Some("png"), Some("None")));
        assert_eq!(q["width"].as_f64(), Some(st.page.width_mm));
        assert_eq!(q["margins"], json!(st.page.margins_mm));
        // Back to no device: a standard paper, no device sent.
        apply_page_change(&mut st.page, "device", &json!("None"));
        apply_page_change(&mut st.page, "paper", &json!("A3"));
        let p = plot_params(&st, None);
        let a3 = cadcraft_render::paper_size("A3").unwrap().name;
        assert_eq!((p["device"].as_str(), p["paper"].as_str()), (Some("None"), Some(a3)));
        apply_page_change(&mut st.page, "plotStyleTable", &json!("None"));
        assert!(plot_params(&st, None).get("plotStyleTable").is_none());
    }

    #[test]
    fn plot_transparency_checkbox_reaches_the_plot() {
        let mut st = state("Model");
        assert!(!st.page.plot_transparency, "off by default");
        assert_eq!(plot_params(&st, None)["plotTransparency"], json!(false));
        apply_page_change(&mut st.page, "plotTransparency", &json!(true));
        assert!(st.page.plot_transparency);
        assert_eq!(plot_params(&st, None)["plotTransparency"], json!(true));
        // The preview plots it the same way.
        assert_eq!(plot_params(&st, Some(150.0))["plotTransparency"], json!(true));
        // Into a layout's page setup and back out to its Plot dialog.
        let mut s = Session::new();
        s.execute("pagesetup", &json!({ "layout": "Layout1", "plotTransparency": true })).unwrap();
        let page = plot_page(s.doc().unwrap(), "Layout1");
        assert!(page.plot_transparency);
        apply_page_change(&mut st.page, "plotTransparency", &json!(false));
        assert_eq!(plot_params(&st, None)["plotTransparency"], json!(false));
    }

    #[test]
    fn plot_center_and_offsets_are_model_only_and_omitted_for_svg() {
        let mut st = state("Model");
        assert_eq!(plot_params(&st, None)["center"], json!(true));
        st.center = false;
        st.offset_mm = [12.5, -7.0];
        let params = plot_params(&st, None);
        assert_eq!(params["center"], json!(false));
        assert_eq!(params["offset"], json!([12.5, -7.0]));
        let preview = plot_params(&st, Some(150.0));
        assert_eq!(preview["output"], json!("png"));
        assert_eq!(preview["offset"], json!([12.5, -7.0]));

        let svg = cadcraft_engine::cmd::plotters::devices().into_iter().find(|dev| dev.output.as_str() == "svg").expect("built-in SVG device");
        apply_page_change(&mut st.page, "device", &json!(svg.name));
        let params = plot_params(&st, None);
        assert!(params.get("center").is_none() && params.get("offset").is_none(), "SVG does not receive unsupported placement options: {params}");
        let preview = plot_params(&st, Some(150.0));
        assert!(
            preview.get("center").is_none() && preview.get("offset").is_none(),
            "disabled SVG placement controls cannot affect preview output either"
        );

        st.target = "Layout1".into();
        st.page = plot_page(&cadcraft_doc::Drawing::new_metric(), "Layout1");
        let params = plot_params(&st, None);
        assert!(params.get("center").is_none() && params.get("offset").is_none(), "layout plots retain their page setup mapping");
    }

    #[test]
    fn window_pick_brings_the_dialog_back_with_the_picked_window() {
        let ctx = egui::Context::default();
        let mut app = CadApp::new(Session::new(), crate::Services::default());
        set_temp(&ctx, "plot_state", state("Model"));
        set_temp(&ctx, PICKING_WINDOW, true);
        app.start("plotwindow");
        plot_window_resume(&mut app, &ctx);
        assert_eq!(app.ui.dialog, None, "still picking");
        app.session.cmdline("2,3").unwrap();
        app.session.cmdline("12,9").unwrap();
        plot_window_resume(&mut app, &ctx);
        assert_eq!(app.ui.dialog.as_deref(), Some("plot"));
        let st = temp::<PlotState>(&ctx, "plot_state").unwrap();
        assert_eq!((st.page.plot_area.as_str(), st.window), ("window", [2.0, 3.0, 12.0, 9.0]));
        // A cancelled pick brings the dialog back unchanged.
        app.ui.dialog = None;
        set_temp(&ctx, PICKING_WINDOW, true);
        app.start("plotwindow");
        app.session.input(cadcraft_engine::Input::Cancel).ok();
        plot_window_resume(&mut app, &ctx);
        assert_eq!(app.ui.dialog.as_deref(), Some("plot"));
        assert_eq!(temp::<PlotState>(&ctx, "plot_state").unwrap().window, [2.0, 3.0, 12.0, 9.0]);
    }
}

#[cfg(test)]
mod mleaderstyle_dialog_tests {
    use super::*;
    use cadcraft_engine::Session;

    /// MLEADERSTYLE's annotative/block/blockScale/blockCenter fields (already real in the
    /// engine and doc model, see `docs/gaps.md` F10) render in the dialog without panicking,
    /// whether the style's default content is MText (`block: None`) or a named block, and the
    /// `BlockChoice` field's JSON mapping round-trips through the real `mleaderstyle` command.
    #[test]
    fn dialog_renders_block_content_fields_for_mtext_and_block_defaults() {
        let mut app = CadApp::new(Session::new(), crate::Services::default());
        app.run("point", json!({"at": [0.0, 0.0]})).unwrap();
        let pt = app.session.doc().unwrap().model.iter().next().unwrap().handle.hex();
        app.run("block", json!({"name": "Tag", "base": [0.0, 0.0], "handles": [pt], "keep": "retain"})).unwrap();

        // Default (MText) content: the dialog must render `block: None` without panicking.
        route(&mut app, "mleaderstyle", &Value::Null).unwrap().unwrap();
        let ctx = egui::Context::default();
        let mut open = true;
        ctx.run_ui(egui::RawInput::default(), |ui| dialog(&mut app, ui.ctx(), "mleaderstyle", &mut open)).drop_without_applying_deltas();
        assert!(open);

        // Switch Standard to block content through the real command, then render again.
        app.run(
            "mleaderstyle",
            json!({"name": "Standard", "annotative": true, "block": "Tag", "blockScale": 2.0, "blockCenter": true, "current": false}),
        )
        .unwrap();
        let st = app.session.doc().unwrap().mleader_styles.iter().find(|s| s.name == "Standard").cloned().unwrap();
        assert_eq!(st.block.as_deref(), Some("Tag"));
        assert!(st.annotative && st.block_center);
        assert_eq!(st.block_scale, 2.0);
        ctx.run_ui(egui::RawInput::default(), |ui| dialog(&mut app, ui.ctx(), "mleaderstyle", &mut open)).drop_without_applying_deltas();
        assert!(open);
    }
}

#[cfg(test)]
mod dimstyle_dialog_tests {
    use super::*;
    use cadcraft_engine::Session;

    /// DIMSTYLE had no manager dialog at all before this (typed bare `dimstyle` just errored
    /// "`name` is required" — no route() entry opened a dialog, unlike every other style
    /// command). This renders the new dialog, with all of `DIM_NUM_FIELDS`/`DIM_BOOL_FIELDS`
    /// present, and round-trips a representative field of each kind through the real
    /// `dimstyle` command (not a dialog-local copy): a negative-valued field (centre mark,
    /// confirming the `>= 0` clamp was correctly widened) and a boolean.
    #[test]
    fn dialog_opens_and_round_trips_fields_through_the_real_command() {
        let mut app = CadApp::new(Session::new(), crate::Services::default());
        route(&mut app, "dimstyle", &Value::Null).unwrap().unwrap();
        assert_eq!(app.ui.dialog.as_deref(), Some("dimstyle"));
        let ctx = egui::Context::default();
        let mut open = true;
        ctx.run_ui(egui::RawInput::default(), |ui| dialog(&mut app, ui.ctx(), "dimstyle", &mut open)).drop_without_applying_deltas();
        assert!(open);

        app.run("dimstyle", json!({"name": "Standard", "centerMark": -2.5, "tolerance": true, "current": false})).unwrap();
        let st = app.session.doc().unwrap().dim_styles.iter().find(|s| s.name == "Standard").cloned().unwrap();
        assert_eq!(st.center_mark, -2.5, "negative centre-mark (lines) must reach the style");
        assert!(st.tolerance);
        ctx.run_ui(egui::RawInput::default(), |ui| dialog(&mut app, ui.ctx(), "dimstyle", &mut open)).drop_without_applying_deltas();
        assert!(open);
    }

    /// All twelve fields F7 named as missing UI (colours DIMCLRD/E/T, arrowhead/leader blocks
    /// DIMBLK/DIMBLK1/DIMBLK2/DIMLDRBLK, linetypes DIMLTYPE/DIMLTEX1/DIMLTEX2 and lineweights
    /// DIMLWD/DIMLWE) now render as pickers reusing the Layer Manager's own colour/linetype/
    /// lineweight widgets (`layers::color_button`/`linetype_button`/`lineweight_button`) and the
    /// Multileader Style dialog's block-choice combo pattern. This proves each field round-trips
    /// through the real `dimstyle` command (not a dialog-local copy) and that the dialog renders
    /// with a loaded linetype and a user block present (exercising the non-default picker rows)
    /// without panicking.
    #[test]
    fn dialog_renders_and_round_trips_all_twelve_picker_fields() {
        let mut app = CadApp::new(Session::new(), crate::Services::default());
        // A loaded (non-default) linetype and a user block, so the new pickers have real choices
        // besides the built-in ByLayer/ByBlock/"(Closed filled)" defaults.
        app.run("linetype", json!({"load": "DASHED"})).unwrap();
        app.run("point", json!({"at": [0.0, 0.0]})).unwrap();
        let pt = app.session.doc().unwrap().model.iter().next().unwrap().handle.hex();
        app.run("block", json!({"name": "MyArrow", "base": [0.0, 0.0], "handles": [pt], "keep": "retain"})).unwrap();

        route(&mut app, "dimstyle", &Value::Null).unwrap().unwrap();
        let ctx = egui::Context::default();
        let mut open = true;
        ctx.run_ui(egui::RawInput::default(), |ui| dialog(&mut app, ui.ctx(), "dimstyle", &mut open)).drop_without_applying_deltas();
        assert!(open, "dialog must render all twelve new picker fields without panicking");

        let mut fields = {
            let d = app.session.doc().unwrap();
            dimstyle_fields(&d.dim_styles[0], &[], &["MyArrow".into()], &d.linetypes)
        };
        let mut applied = 0;
        for field in &mut fields {
            match field {
                Field::Color(key, _, color) => {
                    *color = match *key {
                        "dimLineColor" => cadcraft_color::Color::Index(1),
                        "extLineColor" => cadcraft_color::Color::True(cadcraft_color::Rgb(12, 34, 56)),
                        _ => cadcraft_color::Color::ByLayer,
                    };
                }
                Field::ArrowBlock(_, _, _, name, _) => *name = "MyArrow".into(),
                Field::Linetype(_, _, name, _) => *name = "DASHED".into(),
                Field::Weight(key, _, weight) => {
                    *weight = if *key == "dimLineweight" { cadcraft_doc::Lineweight::ByLayer } else { cadcraft_doc::Lineweight::Mm100(25) };
                }
                _ => continue,
            }
            // Exactly the typed picker conversion and immediate-apply action used by the dialog.
            let (key, value) = field.picker_value().unwrap();
            let (cmd, params) = style_change("dimstyle", "Standard", key, value);
            let result = app.run(cmd, params).unwrap();
            assert!(result.get("ignored").is_none(), "{key}: {result}");
            applied += 1;
        }
        assert_eq!(applied, 12, "F7's enumerated variables total twelve, not eleven");
        let st = app.session.doc().unwrap().dim_styles.iter().find(|s| s.name == "Standard").cloned().unwrap();
        assert_eq!(st.dim_line_color, cadcraft_color::Color::Index(1), "DIMCLRD");
        assert_eq!(st.ext_line_color, cadcraft_color::Color::True(cadcraft_color::Rgb(12, 34, 56)), "DIMCLRE");
        assert_eq!(st.text_color, cadcraft_color::Color::ByLayer, "DIMCLRT");
        assert_eq!(st.arrow_block, "MyArrow", "DIMBLK");
        assert_eq!(st.arrow_block1, "MyArrow", "DIMBLK1");
        assert_eq!(st.arrow_block2, "MyArrow", "DIMBLK2");
        assert_eq!(st.leader_arrow_block, "MyArrow", "DIMLDRBLK");
        assert_eq!(st.dim_linetype, "DASHED", "DIMLTYPE");
        assert_eq!(st.ext_linetype1, "DASHED", "DIMLTEX1");
        assert_eq!(st.ext_linetype2, "DASHED", "DIMLTEX2");
        assert_eq!(st.dim_lineweight, -1, "DIMLWD (ByLayer)");
        assert_eq!(st.ext_lineweight, 25, "DIMLWE (0.25mm)");
        let reloaded = {
            let d = app.session.doc().unwrap();
            dimstyle_fields(&st, &[], &["MyArrow".into()], &d.linetypes)
        };
        for (edited, reopened) in fields.iter().zip(&reloaded) {
            assert_eq!(edited.picker_value(), reopened.picker_value());
        }
        // Clear every block/linetype and exercise both remaining logical lineweights.
        for field in &mut fields {
            match field {
                Field::ArrowBlock(_, _, _, name, _) | Field::Linetype(_, _, name, _) => name.clear(),
                Field::Weight(key, _, weight) => {
                    *weight = if *key == "dimLineweight" { cadcraft_doc::Lineweight::ByBlock } else { cadcraft_doc::Lineweight::Default };
                }
                _ => continue,
            }
            let (key, value) = field.picker_value().unwrap();
            let (cmd, params) = style_change("dimstyle", "Standard", key, value);
            app.run(cmd, params).unwrap();
        }
        let cleared = app.session.doc().unwrap().dim_style("Standard").unwrap();
        assert!(
            cleared.arrow_block.is_empty()
                && cleared.arrow_block1.is_empty()
                && cleared.arrow_block2.is_empty()
                && cleared.leader_arrow_block.is_empty()
        );
        assert!(cleared.dim_linetype.is_empty() && cleared.ext_linetype1.is_empty() && cleared.ext_linetype2.is_empty());
        assert_eq!((cleared.dim_lineweight, cleared.ext_lineweight), (-2, -3));

        // Check the new rows' real layout/accessibility; optionally render them for visual review.
        let shot = std::env::var_os("CADCRAFT_DIMSTYLE_SHOT");
        let styles = vec![Style { name: "Standard".into(), fields: reloaded.into_iter().filter(|f| f.picker_value().is_some()).collect() }];
        let builder = egui_kittest::Harness::builder().with_size(vec2(1400.0, 900.0));
        let builder = if shot.is_some() { builder.wgpu() } else { builder };
        let mut harness = builder.build_ui(move |ui| {
            crate::theme::apply(ui.ctx(), egui::Theme::Dark, false);
            let mut open = true;
            let _ = style_manager(ui.ctx(), &mut open, "Dimension Style Manager", "dimstyle", "Standard", &styles);
        });
        harness.run_steps(3);
        if let Some(path) = shot {
            harness.render().unwrap().save(std::path::PathBuf::from(path)).unwrap();
        }

        // The dialog must still render with these non-default values (loaded linetype swatch,
        // named arrow block, ByBlock/explicit colours) present in every new picker row.
        ctx.run_ui(egui::RawInput::default(), |ui| dialog(&mut app, ui.ctx(), "dimstyle", &mut open)).drop_without_applying_deltas();
        assert!(open);
    }
}

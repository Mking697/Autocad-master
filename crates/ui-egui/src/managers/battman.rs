//! Block Attribute Manager (BATTMAN): a block's attribute definitions in a list, with buttons to
//! synchronize, move and remove them and fields to edit the selected one. Every change runs the
//! `battman` command (`app.run`), so it is undoable and scriptable.

use cadcraft_doc::EntityKind;
use egui::{RichText, vec2};
use serde_json::{Value, json};

use crate::CadApp;
use crate::theme::Tokens;

/// One attribute definition as listed.
struct Row {
    tag: String,
    prompt: String,
    default: String,
    invisible: bool,
    constant: bool,
    height: f64,
    layer: String,
    multiline: bool,
    width: f64,
}

fn temp<T: Clone + Send + Sync + 'static>(ctx: &egui::Context, id: &str) -> Option<T> {
    ctx.data_mut(|d| d.get_temp::<T>(egui::Id::unique(id)))
}

fn set_temp<T: Clone + Send + Sync + 'static>(ctx: &egui::Context, id: &str, v: T) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(id), v));
}

/// A text field committing on Enter / focus loss when changed (an empty value too).
///
/// `label` is the visible row label's widget id, so the field has an accessible name.
fn text_field(ui: &mut egui::Ui, id: egui::Id, value: &str, label: egui::Id) -> Option<String> {
    value_field(ui, id, value, label, false)
}

fn value_field(ui: &mut egui::Ui, id: egui::Id, value: &str, label: egui::Id, multiline: bool) -> Option<String> {
    let mut buf = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or_else(|| value.to_string());
    let field = if multiline { egui::TextEdit::multiline(&mut buf).desired_rows(3) } else { egui::TextEdit::singleline(&mut buf) };
    let resp = ui.add(field.id(id.with("te")).desired_width(180.0)).labelled_by(label);
    if resp.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, buf.clone()));
    } else {
        ui.data_mut(|d| d.remove::<String>(id));
    }
    (resp.lost_focus() && buf != value).then_some(buf)
}

fn fmt_num(v: f64) -> String {
    let s = format!("{v:.4}");
    if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s }
}

/// Show the dialog; returns the command to run.
pub fn dialog(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) -> Option<(&'static str, Value)> {
    let t = Tokens::get();
    let Ok(d) = app.session.doc() else {
        egui::Window::new("Block Attribute Manager").open(open).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.label("Open a drawing first.");
        });
        return None;
    };
    let mut blocks: Vec<String> = d.blocks.values().filter(|b| !b.anonymous).map(|b| b.name.clone()).collect();
    blocks.sort_by_key(|n| n.to_ascii_uppercase());
    let layers: Vec<String> = d.layers.iter().map(|l| l.name.clone()).collect();
    let mut block = temp::<String>(ctx, "battman_block").filter(|b| blocks.contains(b)).or_else(|| blocks.first().cloned()).unwrap_or_default();
    let rows: Vec<Row> = d
        .block(&block)
        .map(|b| {
            b.entities
                .iter()
                .filter_map(|e| match &e.kind {
                    EntityKind::AttDef(a) => Some(Row {
                        tag: a.tag.clone(),
                        prompt: a.prompt.clone(),
                        default: a.text.value.clone(),
                        invisible: a.invisible,
                        constant: a.constant,
                        height: a.text.height,
                        layer: e.common.layer.clone(),
                        multiline: a.multiline.is_some(),
                        width: a.multiline.as_ref().map_or(0.0, |ml| ml.mtext.width),
                    }),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    let refs = count_references(d, &block);
    let mut sel = temp::<String>(ctx, "battman_sel").filter(|s| rows.iter().any(|r| r.tag == *s)).or_else(|| rows.first().map(|r| r.tag.clone()));
    let mut apply = temp::<bool>(ctx, "battman_apply").unwrap_or(true);
    let mut action: Option<(&'static str, Value)> = None;
    let act = |p: Value, apply: bool| {
        let mut p = p;
        if let Some(o) = p.as_object_mut() {
            o.insert("sync".into(), json!(apply));
        }
        Some(("battman", p))
    };
    egui::Window::new("Block Attribute Manager").open(open).default_width(460.0).resizable(false).show(ctx, |ui| {
        if blocks.is_empty() {
            ui.label("This drawing has no blocks.");
            return;
        }
        ui.horizontal(|ui| {
            ui.label("Block:");
            egui::ComboBox::from_id_salt("battman_blocks").selected_text(block.as_str()).width(180.0).show_ui(ui, |ui| {
                for b in &blocks {
                    if ui.selectable_label(*b == block, b).clicked() {
                        block = b.clone();
                        sel = None;
                    }
                }
            });
            ui.label(RichText::new(format!("Found in drawing: {refs}")).color(t.text_dim));
        });
        ui.separator();
        {
            egui::ScrollArea::vertical().max_height(180.0).auto_shrink([false, true]).show(ui, |ui| {
                egui::Grid::new("battman_grid").striped(true).num_columns(4).spacing(vec2(16.0, 4.0)).show(ui, |ui| {
                    for h in ["Tag", "Prompt", "Default", "Modes"] {
                        ui.label(RichText::new(h).strong());
                    }
                    ui.end_row();
                    for r in &rows {
                        if ui.selectable_label(sel.as_deref() == Some(r.tag.as_str()), &r.tag).clicked() {
                            sel = Some(r.tag.clone());
                        }
                        ui.label(&r.prompt);
                        ui.label(&r.default);
                        let modes = [(r.invisible, "I"), (r.constant, "C"), (r.multiline, "M")]
                            .iter()
                            .filter(|(on, _)| *on)
                            .map(|(_, m)| *m)
                            .collect::<String>();
                        ui.label(RichText::new(modes).color(t.text_dim));
                        ui.end_row();
                    }
                });
            });
            ui.horizontal(|ui| {
                if ui.button("Sync").on_hover_text("Bring every reference in line with these definitions").clicked() {
                    action = act(json!({ "name": block, "action": "sync" }), true);
                }
                let picked = sel.clone().unwrap_or_default();
                let i = rows.iter().position(|r| r.tag == picked);
                if ui.add_enabled(i.is_some_and(|i| i > 0), egui::Button::new("Move Up")).clicked() {
                    action = act(json!({ "name": block, "action": "up", "tag": picked }), apply);
                }
                if ui.add_enabled(i.is_some_and(|i| i + 1 < rows.len()), egui::Button::new("Move Down")).clicked() {
                    action = act(json!({ "name": block, "action": "down", "tag": picked }), apply);
                }
                if ui.add_enabled(i.is_some(), egui::Button::new("Remove")).clicked() {
                    action = act(json!({ "name": block, "action": "remove", "tag": picked }), apply);
                }
            });
            ui.horizontal(|ui| {
                ui.label("New tag:");
                let id = egui::Id::unique(("battman_new_tag", &block));
                let mut buf = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or_default();
                ui.add(egui::TextEdit::singleline(&mut buf).id(id.with("te")).desired_width(140.0));
                let mut multi = temp::<bool>(ctx, "battman_new_multiline").unwrap_or(false);
                ui.checkbox(&mut multi, "Multiline");
                set_temp(ctx, "battman_new_multiline", multi);
                let can_add = !buf.trim().is_empty()
                    && !buf.trim().contains(char::is_whitespace)
                    && !rows.iter().any(|r| r.tag.eq_ignore_ascii_case(buf.trim()));
                if ui.add_enabled(can_add, egui::Button::new("New")).clicked() {
                    action = act(json!({ "name": block, "action": "add", "tag": buf.trim(), "multiline": multi }), apply);
                    sel = Some(buf.trim().to_ascii_uppercase());
                    buf.clear();
                }
                ui.data_mut(|d| d.insert_temp(id, buf));
            });
        }
        ui.separator();
        if let Some(r) = sel.as_ref().and_then(|s| rows.iter().find(|r| r.tag == *s)) {
            ui.label(RichText::new(format!("Edit attribute {}", r.tag)).strong());
            // Collect only this row's edit fields, so focus loss and a checkbox/combo click
            // in the same frame produce one command instead of overwriting each other.
            let mut edits = None;
            let mut edit = |key: &str, v: Value| {
                let p = edits.get_or_insert_with(|| json!({ "name": block, "action": "edit", "tag": r.tag }));
                if let Some(o) = p.as_object_mut() {
                    o.insert(key.into(), v);
                }
            };
            egui::Grid::new("battman_edit").num_columns(2).spacing(vec2(10.0, 6.0)).show(ui, |ui| {
                let label = ui.label("Tag");
                if let Some(v) = text_field(ui, egui::Id::unique(("battman_tag", &block, &r.tag)), &r.tag, label.id) {
                    edit("newTag", json!(v));
                    sel = Some(v.trim().to_ascii_uppercase());
                }
                ui.end_row();
                let label = ui.label("Prompt");
                if let Some(v) = text_field(ui, egui::Id::unique(("battman_prompt", &block, &r.tag)), &r.prompt, label.id) {
                    edit("prompt", json!(v));
                }
                ui.end_row();
                let label = ui.label("Default");
                if let Some(v) = value_field(ui, egui::Id::unique(("battman_default", &block, &r.tag)), &r.default, label.id, r.multiline) {
                    edit("default", json!(v));
                }
                ui.end_row();
                ui.label("Mode");
                ui.horizontal(|ui| {
                    let (mut inv, mut con) = (r.invisible, r.constant);
                    if ui.checkbox(&mut inv, "Invisible").changed() {
                        edit("invisible", json!(inv));
                    }
                    if ui.checkbox(&mut con, "Constant").changed() {
                        edit("constant", json!(con));
                    }
                    let mut multi = r.multiline;
                    if ui
                        .checkbox(&mut multi, "Multiline")
                        .on_hover_text("Store the default value as multiline text (MTEXT), wrapped at the width below")
                        .changed()
                    {
                        edit("multiline", json!(multi));
                    }
                });
                ui.end_row();
                if r.multiline {
                    let label = ui.label("Width (0 = no wrapping)");
                    if let Some(w) = text_field(ui, egui::Id::unique(("battman_width", &block, &r.tag)), &fmt_num(r.width), label.id)
                        .and_then(|v| v.trim().parse::<f64>().ok())
                        .filter(|w| w.is_finite() && *w >= 0.0)
                    {
                        edit("width", json!(w));
                    }
                    ui.end_row();
                }
                let label = ui.label("Height");
                let height = text_field(ui, egui::Id::unique(("battman_height", &block, &r.tag)), &fmt_num(r.height), label.id);
                if let Some(h) = height.and_then(|v| v.trim().parse::<f64>().ok()).filter(|h| h.is_finite() && *h > 0.0) {
                    edit("height", json!(h));
                }
                ui.end_row();
                ui.label("Layer");
                egui::ComboBox::from_id_salt("battman_layer").selected_text(r.layer.as_str()).show_ui(ui, |ui| {
                    for l in &layers {
                        if ui.selectable_label(*l == r.layer, l).clicked() && *l != r.layer {
                            edit("layer", json!(l));
                        }
                    }
                });
                ui.end_row();
            });
            // Structural actions take precedence; never mix add/remove/move/sync
            // payloads with selected-row edits.
            if action.is_none() {
                action = edits.and_then(|p| act(p, apply));
            }
        }
        ui.separator();
        ui.checkbox(&mut apply, "Apply changes to existing references");
    });
    set_temp(ctx, "battman_block", block);
    if let Some(s) = sel {
        set_temp(ctx, "battman_sel", s);
    }
    set_temp(ctx, "battman_apply", apply);
    action
}

/// References of block `name` in model space, the layouts and other blocks.
fn count_references(d: &cadcraft_doc::Drawing, name: &str) -> usize {
    let is_ref = |e: &std::sync::Arc<cadcraft_doc::Entity>| matches!(&e.kind, EntityKind::Insert(i) if i.block.eq_ignore_ascii_case(name));
    let spaces = std::iter::once(&d.model).chain(d.layouts.iter().map(|l| &l.entities));
    let blocks = d.blocks.iter().filter(|(k, _)| !k.eq_ignore_ascii_case(name)).map(|(_, b)| &b.entities);
    spaces.chain(blocks).map(|st| st.iter().filter(|e| is_ref(e)).count()).sum()
}

#[cfg(test)]
#[path = "battman_tests.rs"]
mod tests;

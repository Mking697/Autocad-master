//! The Object Grouping dialog (#417 part 3): GROUP, GROUPEDIT and UNGROUP's commands, with a
//! list of the drawing's groups (name, description, selectable flag, member count), New…,
//! Rename…, Description…, toggling Selectable, Add Selected / Remove Selected (the current
//! canvas selection) and Delete (ungroup). Every action runs the existing `group`/`groupedit`/
//! `ungroup` engine commands through `app.run(...)`; this dialog holds no group data of its own.

use egui::vec2;
use serde_json::{Value, json};

use crate::CadApp;
use crate::theme::Tokens;

/// The New or Rename/Describe form.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Form {
    pub name: String,
    pub description: String,
    /// `None` while creating a new group; `Some(original name)` while renaming/describing one.
    pub editing: Option<String>,
}

/// The dialog while open. Holds no group data: every frame re-reads `groups.list`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GroupDlgState {
    pub selected: Option<String>,
    pub filter: String,
    pub form: Option<Form>,
    pub error: Option<String>,
}

/// One row as `groups.list` reports it.
#[derive(Clone, Debug, PartialEq)]
struct Row {
    name: String,
    description: String,
    selectable: bool,
    members: usize,
}

fn rows(app: &mut CadApp) -> Vec<Row> {
    let listing = app.session.execute("groups.list", &json!({})).unwrap_or_default();
    listing
        .get("groups")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|g| {
                    Some(Row {
                        name: g.get("name")?.as_str()?.to_string(),
                        description: g.get("description").and_then(Value::as_str).unwrap_or_default().to_string(),
                        selectable: g.get("selectable").and_then(Value::as_bool).unwrap_or(true),
                        members: g.get("members").and_then(Value::as_array).map_or(0, Vec::len),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Open the dialog.
pub fn open(app: &mut CadApp) {
    app.group_dlg = Some(GroupDlgState::default());
    app.ui.dialog = Some("groupmgr".into());
}

/// Object Grouping window: the drawing's groups, and actions that run GROUP/GROUPEDIT/UNGROUP.
pub fn dialog(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) {
    if app.group_dlg.is_none() {
        self::open(app);
    }
    let warn = Tokens::get().warn;
    let all = rows(app);
    let sel_count = app.session.selection().len();
    let Some(st) = app.group_dlg.as_mut() else { return };
    let f = st.filter.trim().to_ascii_lowercase();
    let shown: Vec<Row> = all
        .into_iter()
        .filter(|r| f.is_empty() || r.name.to_ascii_lowercase().contains(&f) || r.description.to_ascii_lowercase().contains(&f))
        .collect();
    let selected_shown = st.selected.as_ref().is_some_and(|s| shown.iter().any(|r| &r.name == s));
    if !selected_shown {
        st.selected = None;
    }
    let sel = st.selected.clone();
    let mut actions: Vec<(&'static str, Value)> = Vec::new();
    egui::Window::new(crate::tl!("Object Grouping"))
        .id(egui::Id::unique("groupmgr_dialog"))
        .open(open)
        .collapsible(false)
        .default_size(vec2(520.0, 460.0))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                let label = ui.label(crate::tl!("Filter"));
                ui.text_edit_singleline(&mut st.filter).labelled_by(label.id);
            });
            egui::ScrollArea::vertical().max_height(220.0).auto_shrink([false, true]).show(ui, |ui| {
                egui::Grid::new("group_rows").striped(true).num_columns(4).show(ui, |ui| {
                    ui.strong(crate::tl!("Name"));
                    ui.strong(crate::tl!("Description"));
                    ui.strong(crate::tl!("Objects"));
                    ui.strong(crate::tl!("Selectable"));
                    ui.end_row();
                    for r in &shown {
                        let is_sel = sel.as_deref() == Some(r.name.as_str());
                        if ui.selectable_label(is_sel, &r.name).clicked() {
                            st.selected = Some(r.name.clone());
                        }
                        ui.label(if r.description.is_empty() { "—".to_string() } else { r.description.clone() });
                        ui.label(r.members.to_string());
                        let mut selectable = r.selectable;
                        if ui.checkbox(&mut selectable, format!("{} selectable", r.name)).changed() {
                            actions.push(("groupedit", json!({ "name": r.name, "selectable": selectable })));
                        }
                        ui.end_row();
                    }
                });
            });
            let sel_known = sel.as_ref().is_some_and(|s| shown.iter().any(|r| &r.name == s));
            ui.horizontal(|ui| {
                if ui.button(crate::tl!("New…")).clicked() {
                    st.form = Some(Form::default());
                    st.error = None;
                }
                if ui.add_enabled(sel_known, egui::Button::new(crate::tl!("Rename/Describe…"))).clicked()
                    && let Some(s) = &sel
                    && let Some(r) = shown.iter().find(|r| &r.name == s)
                {
                    st.form = Some(Form { name: r.name.clone(), description: r.description.clone(), editing: Some(r.name.clone()) });
                    st.error = None;
                }
                if ui.add_enabled(sel_known && sel_count > 0, egui::Button::new(crate::tl!("Add Selected"))).clicked()
                    && let Some(s) = &sel
                {
                    actions.push(("groupedit.add_selected", json!({ "name": s })));
                }
                if ui.add_enabled(sel_known && sel_count > 0, egui::Button::new(crate::tl!("Remove Selected"))).clicked()
                    && let Some(s) = &sel
                {
                    actions.push(("groupedit.remove_selected", json!({ "name": s })));
                }
                if ui.add_enabled(sel_known, egui::Button::new(crate::tl!("Delete"))).clicked()
                    && let Some(s) = &sel
                {
                    actions.push(("ungroup", json!({ "name": s })));
                    st.selected = None;
                }
            });
            if sel_count == 0 {
                ui.label(crate::tl!("Select objects on the drawing to add them to a group, or before New… to seed one."));
            }
            let mut save = false;
            let mut discard = false;
            if let Some(form) = st.form.as_mut() {
                ui.group(|ui| {
                    egui::Grid::new("group_form").num_columns(2).show(ui, |ui| {
                        let name_label = ui.label(crate::tl!("Name"));
                        ui.add_enabled(form.editing.is_none(), egui::TextEdit::singleline(&mut form.name)).labelled_by(name_label.id);
                        ui.end_row();
                        let desc_label = ui.label(crate::tl!("Description"));
                        ui.text_edit_singleline(&mut form.description).labelled_by(desc_label.id);
                        ui.end_row();
                    });
                    ui.horizontal(|ui| {
                        save = ui.button(if form.editing.is_some() { crate::tl!("Save") } else { crate::tl!("Create") }).clicked();
                        discard = ui.button(crate::tl!("Discard")).clicked();
                    });
                });
            }
            if save && let Some(form) = st.form.clone() {
                match &form.editing {
                    Some(old) => actions.push(("groupedit", json!({ "name": old, "rename": form.name, "description": form.description }))),
                    None => actions.push(("group.new", json!({ "name": form.name, "description": form.description }))),
                }
            }
            if discard {
                st.form = None;
                st.error = None;
            }
            if let Some(e) = &st.error {
                ui.colored_label(warn, e.as_str());
            }
        });
    let sel_handles: Vec<Value> = app.session.selection().iter().map(|h| json!(h.hex())).collect();
    for (action, mut params) in actions {
        let r = match action {
            "group.new" => {
                let name = params.get("name").and_then(Value::as_str).filter(|n| !n.trim().is_empty()).map(str::to_string);
                if let Some(obj) = params.as_object_mut() {
                    obj.insert("name".into(), json!(name));
                }
                app.run("group", params)
            }
            "groupedit.remove_selected" => {
                if let Some(obj) = params.as_object_mut() {
                    obj.insert("remove".into(), json!(sel_handles.clone()));
                }
                app.run("groupedit", params)
            }
            "groupedit.add_selected" => {
                if let Some(obj) = params.as_object_mut() {
                    obj.insert("add".into(), json!(sel_handles.clone()));
                }
                app.run("groupedit", params)
            }
            _ => app.run(action, params),
        };
        if let Some(st) = app.group_dlg.as_mut() {
            match r {
                Ok(_) => {
                    st.error = None;
                    if action == "group.new" || action.starts_with("groupedit") {
                        st.form = None;
                    }
                }
                Err(e) => st.error = Some(e),
            }
        }
    }
    if !*open {
        app.group_dlg = None;
    }
}

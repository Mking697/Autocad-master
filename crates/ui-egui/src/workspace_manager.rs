//! Named layout management. All mutations use the same commands as automation.
use serde_json::json;

use crate::CadApp;

#[derive(Clone, Default)]
struct Form {
    name: String,
    selected: Option<String>,
    confirm_delete: bool,
    error: Option<String>,
}

fn button(ui: &mut egui::Ui, label: &str, enabled: bool) -> bool {
    let response = ui.add_enabled(enabled, egui::Button::new(crate::i18n::t(label)));
    crate::control::record_widget(ui, &response, label, "button", None);
    response.clicked()
}

/// The form is temporary; its records are always read from the app's workspace registry.
pub fn dialog(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) {
    let id = egui::Id::unique("named_workspace_form");
    let listing = app.run("ui.workspace.list", json!({})).unwrap_or_default();
    let names: Vec<String> =
        listing.get("names").and_then(|v| v.as_array()).into_iter().flatten().filter_map(|v| v.as_str()).map(str::to_owned).collect();
    let current = listing.get("current").and_then(|v| v.as_str());
    let mut form =
        ctx.data_mut(|data| data.get_temp::<Form>(id)).unwrap_or_else(|| Form { selected: current.map(str::to_owned), ..Default::default() });
    if form.selected.as_ref().is_some_and(|name| !names.contains(name)) {
        form.selected = None;
        form.confirm_delete = false;
    }
    let mut close = false;
    egui::Window::new(crate::tl!("Workspaces")).id(egui::Id::unique("named_workspaces")).open(open).default_width(420.0).min_width(250.0).show(
        ctx,
        |ui| {
            ui.label(format!("{} {}", crate::tl!("Current workspace:"), current.unwrap_or(crate::tl!("Unsaved layout"))));
            ui.label(crate::tl!("Save panel positions and interface tools. Drawings and active commands are not changed."));
            ui.separator();
            egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                for name in &names {
                    let selected = form.selected.as_ref() == Some(name);
                    if ui.selectable_label(selected, name).clicked() {
                        form.selected = Some(name.clone());
                        form.confirm_delete = false;
                        form.error = None;
                    }
                }
                if names.is_empty() {
                    ui.label(crate::tl!("No saved workspaces."));
                }
            });
            ui.separator();
            let name_label = ui.label(crate::tl!("Workspace name"));
            let name =
                ui.add(egui::TextEdit::singleline(&mut form.name).char_limit(64).desired_width(ui.available_width())).labelled_by(name_label.id);
            crate::control::record_widget(ui, &name, "Workspace name", "text", None);
            let mut action = None;
            if button(ui, "Save Workspace", !form.name.trim().is_empty()) {
                action = Some(("ui.workspace.save", form.name.trim().to_owned(), false));
            }
            ui.horizontal_wrapped(|ui| {
                if button(ui, "Switch Workspace", form.selected.is_some()) {
                    action = form.selected.clone().map(|name| ("ui.workspace.switch", name, false));
                }
                if button(ui, "Update Selected Workspace", form.selected.is_some()) {
                    action = form.selected.clone().map(|name| ("ui.workspace.save", name, true));
                }
                if button(ui, "Delete Workspace", form.selected.is_some()) {
                    form.confirm_delete = true;
                }
            });
            if form.confirm_delete {
                ui.label(crate::tl!("Delete the selected saved workspace? The current layout and drawing are kept."));
                ui.horizontal_wrapped(|ui| {
                    if button(ui, "Confirm Delete", true) {
                        action = form.selected.clone().map(|name| ("ui.workspace.delete", name, false));
                    }
                    if button(ui, "Keep Workspace", true) {
                        form.confirm_delete = false;
                    }
                });
            }
            if let Some((command, name, replace)) = action {
                match app.run(command, json!({"name":name, "replace":replace})) {
                    Ok(_) => {
                        form.selected = (command != "ui.workspace.delete").then_some(name);
                        form.confirm_delete = false;
                        form.error = None;
                    }
                    Err(error) => form.error = Some(error),
                }
            }
            if let Some(error) = &form.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            ui.separator();
            close = button(ui, "Close Workspaces", true);
        },
    );
    if close {
        *open = false;
    }
    ctx.data_mut(|data| {
        if *open {
            data.insert_temp(id, form);
        } else {
            data.remove::<Form>(id);
        }
    });
}

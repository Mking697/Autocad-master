//! Standalone settings for the next typed MATCHPROP, not interception of its keyword prompt.

use super::{Action, CadApp, json};

const PROPERTIES: &[(&str, &str)] = &[
    ("color", "Color"),
    ("layer", "Layer"),
    ("linetype", "Linetype"),
    ("ltscale", "Linetype scale"),
    ("lineweight", "Lineweight"),
    ("transparency", "Transparency"),
    ("thickness", "Thickness"),
    ("plotStyle", "Plot style"),
    ("dimension", "Dimension"),
    ("text", "Text"),
    ("hatch", "Hatch"),
    ("polyline", "Polyline"),
    ("viewport", "Viewport"),
    ("table", "Table"),
    ("multileader", "Multileader"),
];

/// Changes apply immediately through a command, like Constraint Settings. Session preferences
/// are not drawing data and do not create document undo steps. Closing leaves them remembered.
pub(super) fn dialog(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) -> Action {
    let active = app.session.last_used.matchprop.names();
    let mut properties = serde_json::Map::new();
    let mut close = false;
    egui::Window::new("Match Properties Settings").open(open).collapsible(false).resizable(false).show(ctx, |ui| {
        ui.label("Choose properties to copy with typed MATCHPROP.");
        ui.label("Changes apply immediately and are remembered for this session.");
        ui.separator();
        for (heading, fields) in [("Basic properties", PROPERTIES.get(..8)), ("Special properties", PROPERTIES.get(8..))] {
            ui.heading(heading);
            egui::Grid::new(("matchprop_settings", heading)).num_columns(2).spacing(egui::vec2(24.0, 4.0)).show(ui, |ui| {
                for (i, (name, label)) in fields.unwrap_or_default().iter().enumerate() {
                    let mut on = active.contains(name);
                    if ui.checkbox(&mut on, *label).changed() {
                        properties.insert((*name).into(), json!(on));
                    }
                    if i % 2 == 1 {
                        ui.end_row();
                    }
                }
            });
            ui.separator();
        }
        ui.horizontal(|ui| {
            for (label, on) in [("Select all", true), ("Clear all", false)] {
                if ui.button(label).clicked() {
                    properties.extend(PROPERTIES.iter().map(|(name, _)| ((*name).into(), json!(on))));
                }
            }
            close = ui.button("Close").clicked();
        });
    });
    if close {
        *open = false;
    }
    (!properties.is_empty()).then(|| ("matchprop.settings", json!({ "properties": properties })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_engine::Session;

    fn frame(app: &mut CadApp, ctx: &egui::Context, events: Vec<egui::Event>) -> (egui::FullOutput, Action) {
        let mut action = None;
        let out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 900.0))),
                events,
                focused: true,
                ..Default::default()
            },
            |ui| action = dialog(app, ui.ctx(), &mut true),
        );
        (out, action)
    }

    fn click(app: &mut CadApp, ctx: &egui::Context, label: &str) -> Action {
        for _ in 0..3 {
            frame(app, ctx, Vec::new()).0.drop_without_applying_deltas();
        }
        let (out, _) = frame(app, ctx, Vec::new());
        let pos = out
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::epaint::Shape::Text(t) if t.galley.text() == label => Some(t.pos + t.galley.size() * 0.5),
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing widget label {label}"));
        out.drop_without_applying_deltas();
        frame(app, ctx, vec![egui::Event::PointerMoved(pos)]).0.drop_without_applying_deltas();
        let (out, action) = frame(
            app,
            ctx,
            [true, false]
                .into_iter()
                .map(|pressed| egui::Event::PointerButton { pos, pressed, button: egui::PointerButton::Primary, modifiers: Default::default() })
                .collect(),
        );
        out.drop_without_applying_deltas();
        action
    }

    #[test]
    fn matchprop_dialog_checkboxes_and_bulk_buttons_change_the_real_settings() {
        let mut app = CadApp::new(Session::new(), crate::Services::default());
        let ctx = egui::Context::default();
        assert_eq!(PROPERTIES.iter().map(|p| p.0).collect::<Vec<_>>(), app.session.last_used.matchprop.names());
        for (name, label) in PROPERTIES {
            let (cmd, params) = click(&mut app, &ctx, label).expect("checkbox action");
            assert_eq!(cmd, "matchprop.settings");
            assert_eq!(params, json!({"properties": { *name: false }}));
            app.run(cmd, params).unwrap();
            assert!(!app.session.last_used.matchprop.names().contains(name));
        }
        assert!(app.session.last_used.matchprop.names().is_empty());
        for (label, on) in [("Select all", true), ("Clear all", false)] {
            let (cmd, params) = click(&mut app, &ctx, label).expect("bulk action");
            assert_eq!(params["properties"].as_object().unwrap().len(), 15);
            assert!(params["properties"].as_object().unwrap().values().all(|v| *v == json!(on)));
            app.run(cmd, params).unwrap();
            assert_eq!(app.session.last_used.matchprop.names().len(), if on { 15 } else { 0 });
        }
        // Reopening uses engine state, not a stale dialog-local copy.
        app.ui.dialog = None;
        app.run("ui.dialog.matchprop", json!(null)).unwrap();
        assert_eq!(app.ui.dialog.as_deref(), Some("matchprop"));
        let (cmd, params) = click(&mut app, &ctx, "Color").unwrap();
        assert_eq!(params, json!({"properties": {"color": true}}));
        app.run(cmd, params).unwrap();
        assert_eq!(app.session.last_used.matchprop.names(), ["color"]);
    }

    #[test]
    fn matchprop_dialog_dispatch_does_not_start_or_replace_the_running_command() {
        let mut app = CadApp::new(Session::new(), crate::Services::default());
        app.start("line");
        app.run("ui.dialog.matchprop", json!(null)).unwrap();
        let ctx = egui::Context::default();
        let out = ctx.run_ui(egui::RawInput::default(), |ui| crate::dialogs::show(&mut app, ui.ctx()));
        out.drop_without_applying_deltas();
        assert_eq!(app.ui.dialog.as_deref(), Some("matchprop"));
        let (cmd, params) = click(&mut app, &ctx, "Color").unwrap();
        app.run(cmd, params).unwrap();
        assert!(!app.session.last_used.matchprop.color);
        assert_eq!(app.session.running.as_ref().unwrap().id, "line");
        assert_eq!(app.session.doc().unwrap().model.len(), 0);
    }
}

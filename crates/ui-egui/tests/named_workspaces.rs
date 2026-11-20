//! Named workspaces change UI layout only, never drafting state.
use cadcraft_engine::Session;
use cadcraft_ui_egui::{CadApp, Services};
use serde_json::json;

fn app() -> CadApp {
    CadApp::new(Session::new(), Services::default())
}

#[test]
fn save_and_switch_restores_layout_without_touching_active_command_or_drawing() {
    let mut app = app();
    app.run("line", json!({"points": [[0,0], [2,0]]})).unwrap();
    app.start("line");
    app.cmd.buffer = "pending point".into();
    app.ui.show_ribbon = true;
    app.ui.show_toolbar = false;
    app.ui.ribbon_tab = "Output".into();
    app.ui.qat_commands = vec!["saveas".into()];
    app.run("ui.workspace.save", json!({"name":"Plotting"})).unwrap();
    app.ui.show_ribbon = false;
    app.ui.show_toolbar = true;
    app.ui.ribbon_tab = "Home".into();
    app.ui.qat_commands.clear();
    app.run("ui.workspace.switch", json!({"name":"plotting"})).unwrap();
    assert!(app.ui.show_ribbon);
    assert!(!app.ui.show_toolbar);
    assert_eq!(app.ui.ribbon_tab, "Output");
    assert_eq!(app.ui.qat_commands, ["saveas"]);
    assert_eq!(app.session.running.as_ref().map(|r| r.id.as_str()), Some("line"));
    assert_eq!(app.cmd.buffer, "pending point");
    assert_eq!(app.session.doc().unwrap().model.len(), 1);
}

#[test]
fn manager_saves_and_switches_through_the_same_commands() {
    use egui_kittest::{Harness, kittest::Queryable};
    let mut app = app();
    app.ui.dialog = Some("workspaces".into());
    app.ui.show_ribbon = true;
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 650.0))
        .build_ui_state(|ui, app: &mut CadApp| cadcraft_ui_egui::dialogs::show(app, ui.ctx()), app);
    h.run();
    h.get_by_label("Workspace name").click();
    h.run();
    h.get_by_label("Workspace name").type_text("Plotting");
    h.run();
    assert_eq!(h.get_by_label("Workspace name").value().as_deref(), Some("Plotting"));
    h.get_by_label("Save Workspace").click();
    h.run();
    assert_eq!(h.state_mut().run("ui.workspace.list", json!({})).unwrap()["names"], json!(["Plotting"]));
    h.state_mut().ui.show_ribbon = false;
    h.get_by_label("Switch Workspace").click();
    h.run();
    assert!(h.state().ui.show_ribbon);
    assert_eq!(h.state().ui.dialog.as_deref(), Some("workspaces"));
}

//! The Ribbon's Quick Access Toolbar (`ribbon::qat_row`): default buttons render, clicking one
//! starts its command, and the "Customize Quick Access Toolbar" dropdown adds/removes a button
//! without crashing.

use cadcraft_engine::Session;
use cadcraft_ui_egui::{CadApp, Services};
use egui::accesskit::Role;
use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::{By, NodeT, Queryable};
use serde_json::json;

fn harness() -> Harness<'static, CadApp> {
    let mut app = CadApp::new(Session::default(), Services::default());
    app.run("line", json!({"points": [[0, 0], [10, 0]]})).unwrap();
    app.ui.in_window_menu = false;
    app.ui.show_ribbon = true;
    // The classic Tool Bar shares some of the same labels (Save, Open...); turn it and Tool Sets
    // off so the QAT's own buttons are each the only match for their label, like the other ribbon
    // tests do for Tool Sets' "Line" button.
    app.ui.show_toolbar = false;
    app.ui.show_toolsets = false;
    // The file tabs' "+" button is also labelled "New Drawing".
    app.ui.show_file_tabs = false;
    app.integrated_titlebar = true;
    let mut harness = Harness::builder().with_size(vec2(1400.0, 800.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            app.ui(ui);
        },
        app,
    );
    harness.run_steps(5);
    harness
}

#[test]
fn qat_choices_survive_preferences_and_legacy_defaults() {
    let mut app = CadApp::new(Session::default(), Services::default());
    app.ui.qat_commands = vec!["saveas".into(), "new".into()];
    let mut restored = CadApp::new(Session::default(), Services::default());
    restored.load_prefs(&app.prefs_json());
    assert_eq!(restored.ui.qat_commands, ["saveas", "new"]);
    app.ui.qat_commands.clear();
    restored.load_prefs(&app.prefs_json());
    assert!(restored.ui.qat_commands.is_empty());
    let legacy: cadcraft_ui_egui::UiState = serde_json::from_value(json!({"showRibbon": true})).unwrap();
    assert_eq!(legacy.qat_commands, cadcraft_ui_egui::UiState::default().qat_commands);
}

#[test]
fn qat_disables_document_commands_and_keeps_customize_in_narrow_windows() {
    let mut app = CadApp::new(Session::empty(), Services::default());
    app.ui.qat_commands = cadcraft_ui_egui::ribbon::QAT_CANDIDATES.iter().map(|(_, id, _, _)| (*id).into()).collect();
    let mut h = Harness::builder().with_size(vec2(160.0, 220.0)).build_ui_state(|ui, app: &mut CadApp| cadcraft_ui_egui::ribbon::show(app, ui), app);
    h.run_steps(3);
    assert!(!h.get_by_label("New Drawing").accesskit_node().is_disabled());
    assert!(!h.get_by_label("Open").accesskit_node().is_disabled());
    assert!(h.get_by_label("Save").accesskit_node().is_disabled());
    let customize = h.get_by_label("Customize Quick Access Toolbar").rect();
    assert!(customize.left() >= 0.0 && customize.right() <= 160.0, "customize must remain on screen: {customize:?}");
    h.get_by_label("Customize Quick Access Toolbar").click();
    h.run_steps(3);
    h.get(By::new().label("Save As").role(Role::CheckBox));
}

#[test]
fn ui_set_sanitizes_qat_even_before_ribbon_is_visible() {
    use cadcraft_ui_egui::control::{ControlRequest, Outcome, handle};
    let mut app = CadApp::new(Session::default(), Services::default());
    let (request, _reply) = ControlRequest::new("ui.set", json!({"qatCommands": ["u", "u", "bad", "new"]}));
    let Outcome::Done(reply) = handle(&mut app, &egui::Context::default(), &request) else { panic!("ui.set should complete") };
    assert_eq!(reply["ok"], true);
    assert_eq!(app.ui.qat_commands, ["u", "new"]);
}

/// The default QAT buttons (New Drawing, Open, Save, Undo, Redo, Print) all render.
#[test]
fn qat_shows_its_default_buttons() {
    let h = harness();
    for (_, _, label, default) in cadcraft_ui_egui::ribbon::QAT_CANDIDATES {
        if *default {
            h.get_by_label(label);
        }
    }
}

/// Clicking the QAT's Undo button runs the `u` (undo) command for real: the LINE drawn before
/// the harness started goes away.
#[test]
fn clicking_a_qat_button_starts_its_command() {
    let mut h = harness();
    assert_eq!(h.state().session.doc().unwrap().model.handles().len(), 1);
    h.get_by_label("Undo").click();
    h.run_steps(3);
    assert_eq!(h.state().session.doc().unwrap().model.handles().len(), 0, "QAT's Undo button should have undone the LINE");
}

/// The Customize dropdown can remove a default button (Save) and add a non-default one (Save
/// As), without crashing; the toolbar reflects both changes on the next frame. Checkbox rows are
/// queried by role since the dropdown's own "Save" checkbox shares its label with the QAT's
/// "Save" button underneath it.
#[test]
fn customize_dropdown_toggles_qat_buttons() {
    let mut h = harness();
    assert!(h.state().ui.qat_commands.iter().any(|c| c == "qsave"));
    assert!(!h.state().ui.qat_commands.iter().any(|c| c == "saveas"));
    h.get_by_label("Customize Quick Access Toolbar").click();
    h.run_steps(2);
    h.get(By::new().label("Save").role(Role::CheckBox)).click();
    h.run_steps(2);
    assert!(!h.state().ui.qat_commands.iter().any(|c| c == "qsave"), "Save should have come off the QAT");
    h.get_by_label("Customize Quick Access Toolbar").click();
    h.run_steps(2);
    h.get(By::new().label("Save As").role(Role::CheckBox)).click();
    h.run_steps(2);
    assert!(h.state().ui.qat_commands.iter().any(|c| c == "saveas"), "Save As should have been added to the QAT");
}

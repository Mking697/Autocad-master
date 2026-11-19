//! The Object Grouping dialog (#417 part 3): a list of the drawing's groups with New…,
//! Rename/Describe…, toggling Selectable, Add/Remove Selected and Delete, all driving the
//! existing GROUP/GROUPEDIT/UNGROUP engine commands.

use cadcraft_doc::Handle;
use cadcraft_engine::Session;
use cadcraft_ui_egui::{CadApp, Services, dialogs, theme};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

fn harness(app: CadApp) -> Harness<'static, CadApp> {
    let mut h = Harness::builder().with_size(egui::vec2(900.0, 700.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            theme::apply(ui.ctx(), egui::Theme::Dark, false);
            dialogs::show(app, ui.ctx());
        },
        app,
    );
    h.run_steps(3);
    h
}

fn drawing_with_a_group() -> (CadApp, Vec<String>) {
    let mut app = CadApp::new(Session::new(), Services::default());
    app.session.execute("line", &json!({ "points": [[0, 0], [10, 0]] })).unwrap();
    app.session.execute("line", &json!({ "points": [[0, 0], [0, 10]] })).unwrap();
    app.session.execute("circle", &json!({ "center": [20, 20], "radius": 2 })).unwrap();
    let handles: Vec<String> = app.session.doc().unwrap().model.iter().map(|e| e.handle.hex()).collect();
    app.session.set_selection(Vec::new());
    (app, handles)
}

fn group_names(app: &mut CadApp) -> Vec<String> {
    app.session.execute("groups.list", &json!({})).unwrap()["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["name"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn ui_groupmgr_opens_the_dialog_but_json_does_not() {
    let (mut app, _) = drawing_with_a_group();
    app.start("ui.groupmgr");
    assert_eq!(app.ui.dialog.as_deref(), Some("groupmgr"));
    app.ui.dialog = None;
    let r = app.run("groups.list", json!({}));
    assert!(r.is_ok());
    assert_eq!(app.ui.dialog, None);
}

#[test]
fn creating_a_group_from_the_selection_then_renaming_it() {
    let (mut app, handles) = drawing_with_a_group();
    // Select the two lines (not the circle), then open the dialog and create a group from them.
    app.session.set_selection(vec![Handle::parse_hex(&handles[0]).unwrap(), Handle::parse_hex(&handles[1]).unwrap()]);
    app.start("ui.groupmgr");
    let mut h = harness(app);
    h.get_by_label("New…").click();
    h.run_steps(2);
    {
        let form = h.state_mut().group_dlg.as_mut().and_then(|s| s.form.as_mut()).expect("the new-group form");
        form.name = "MYGROUP".into();
        form.description = "two lines".into();
    }
    h.run_steps(2);
    h.get_by_label("Create").click();
    h.run_steps(2);
    assert!(h.state_mut().group_dlg.as_ref().and_then(|s| s.error.clone()).is_none(), "create should not fail");
    assert_eq!(group_names(h.state_mut()), vec!["MYGROUP".to_string()]);
    let members = h.state_mut().session.execute("groups.list", &json!({})).unwrap()["groups"][0]["members"].as_array().unwrap().len();
    assert_eq!(members, 2, "the group holds the two selected lines");

    // Select the new row and rename/describe it through the dialog.
    h.state_mut().group_dlg.as_mut().unwrap().selected = Some("MYGROUP".into());
    h.run_steps(2);
    h.get_by_label("Rename/Describe…").click();
    h.run_steps(2);
    {
        let form = h.state_mut().group_dlg.as_mut().and_then(|s| s.form.as_mut()).expect("the rename form");
        assert_eq!(form.description, "two lines", "the form is pre-filled from the selected group");
        form.name = "RENAMED".into();
        form.description = "renamed description".into();
    }
    h.run_steps(2);
    h.get_by_label("Save").click();
    h.run_steps(2);
    assert_eq!(group_names(h.state_mut()), vec!["RENAMED".to_string()]);
    let g = h.state_mut().session.execute("groups.list", &json!({})).unwrap()["groups"][0].clone();
    assert_eq!(g["description"], json!("renamed description"));
}

#[test]
fn toggling_selectable_and_deleting_a_group() {
    let (mut app, handles) = drawing_with_a_group();
    app.run("group", json!({"name": "G1", "handles": [handles[0].clone(), handles[1].clone()]})).unwrap();
    app.start("ui.groupmgr");
    let mut h = harness(app);
    assert!(h.state_mut().session.doc().unwrap().groups[0].selectable, "GROUP's default is selectable");
    h.get_by_label("G1 selectable").click();
    h.run_steps(2);
    assert!(!h.state_mut().session.doc().unwrap().groups[0].selectable, "the checkbox toggled it off");

    h.state_mut().group_dlg.as_mut().unwrap().selected = Some("G1".into());
    h.run_steps(2);
    h.get_by_label("Delete").click();
    h.run_steps(2);
    assert!(group_names(h.state_mut()).is_empty(), "UNGROUP removed the group, not the objects");
    assert_eq!(h.state_mut().session.doc().unwrap().model.len(), 3);
}

#[test]
fn add_and_remove_selected_change_membership() {
    let (mut app, handles) = drawing_with_a_group();
    app.run("group", json!({"name": "G1", "handles": [handles[0].clone()]})).unwrap();
    app.session.set_selection(vec![Handle::parse_hex(&handles[1]).unwrap()]);
    app.start("ui.groupmgr");
    let mut h = harness(app);
    h.state_mut().group_dlg.as_mut().unwrap().selected = Some("G1".into());
    h.run_steps(2);
    h.get_by_label("Add Selected").click();
    h.run_steps(2);
    assert_eq!(h.state_mut().session.doc().unwrap().groups[0].members.len(), 2);

    h.state_mut().session.set_selection(vec![Handle::parse_hex(&handles[1]).unwrap()]);
    h.run_steps(2);
    h.get_by_label("Remove Selected").click();
    h.run_steps(2);
    assert_eq!(h.state_mut().session.doc().unwrap().groups[0].members.len(), 1);
}

#[test]
fn a_form_with_no_selection_shows_the_engines_refusal() {
    let (mut app, _) = drawing_with_a_group();
    app.session.set_selection(Vec::new());
    app.start("ui.groupmgr");
    let mut h = harness(app);
    h.get_by_label("New…").click();
    h.run_steps(2);
    h.get_by_label("Create").click();
    h.run_steps(2);
    assert!(h.state_mut().group_dlg.as_ref().and_then(|s| s.error.clone()).is_some(), "GROUP refuses an empty selection");
    assert!(group_names(h.state_mut()).is_empty());
}

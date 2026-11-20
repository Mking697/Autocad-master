//! Contextual ribbon tabs follow real editor state, not a manually selected tab name.
use cadcraft_engine::Session;
use cadcraft_ui_egui::{CadApp, Services, ribbon};
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{By, Queryable};
use serde_json::json;

fn harness() -> Harness<'static, CadApp> {
    let mut app = CadApp::new(Session::new(), Services::default());
    app.ui.ribbon_tab = "Output".into();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 400.0)).build_ui_state(|ui, app: &mut CadApp| ribbon::show(app, ui), app);
    h.run_steps(3);
    h
}

fn begin_mtext(app: &mut CadApp) {
    app.start("mtext");
    app.session.input(cadcraft_engine::Input::Point(cadcraft_engine::geom::Vec2::new(1.0, 1.0))).unwrap();
    app.session.input(cadcraft_engine::Input::Point(cadcraft_engine::geom::Vec2::new(6.0, 0.0))).unwrap();
}

#[test]
fn nested_text_context_returns_to_block_then_the_normal_tab() {
    let mut h = harness();
    h.state_mut().run("bedit", json!({"name": "Nested", "create": true})).unwrap();
    h.run_steps(3);
    assert_eq!(h.state().ui.ribbon_tab, "Block Editor");
    begin_mtext(h.state_mut());
    h.run_steps(3);
    assert_eq!(h.state().ui.ribbon_tab, "Text Editor");
    h.state_mut().session.cancel();
    h.run_steps(3);
    assert_eq!(h.state().ui.ribbon_tab, "Block Editor");
    h.state_mut().run("bclose", json!({"save": false})).unwrap();
    h.run_steps(3);
    assert_eq!(h.state().ui.ribbon_tab, "Output");
}

#[test]
fn a_manual_normal_tab_remains_selected_until_a_new_context_enters() {
    let mut h = harness();
    h.state_mut().run("bedit", json!({"name": "Manual", "create": true})).unwrap();
    h.run_steps(3);
    h.get(By::new().role(Role::Tab).label("Insert")).click();
    h.run_steps(3);
    assert_eq!(h.state().ui.ribbon_tab, "Insert");
    h.state_mut().run("bclose", json!({"save": false})).unwrap();
    h.run_steps(3);
    assert_eq!(h.state().ui.ribbon_tab, "Insert");
}

#[test]
fn stale_context_names_and_document_switch_do_not_leave_dead_tabs() {
    let mut h = harness();
    h.state_mut().ui.ribbon_tab = "Text Editor".into();
    h.run_steps(3);
    assert_eq!(h.state().ui.ribbon_tab, "Output");
    h.state_mut().run("bedit", json!({"name": "Switch", "create": true})).unwrap();
    h.run_steps(3);
    h.state_mut().run("new", json!({})).unwrap();
    h.run_steps(3);
    assert_eq!(h.state().ui.ribbon_tab, "Output");
    assert!(h.query(By::new().role(Role::Tab).label("Block Editor")).is_none());
    h.state_mut().run("document.switch", json!({"index": 0})).unwrap();
    h.run_steps(3);
    assert_eq!(h.state().ui.ribbon_tab, "Block Editor");
}

#[test]
fn block_save_and_close_buttons_use_the_real_engine() {
    let mut h = harness();
    h.state_mut().run("bedit", json!({"name": "Saved", "create": true})).unwrap();
    h.state_mut().run("line", json!({"points": [[0,0],[2,0]]})).unwrap();
    h.run_steps(3);
    h.get_by_label("Save Block").click();
    h.run_steps(3);
    assert!(!h.state().session.state().unwrap().block_unsaved());
    h.get_by_label("Close Block Editor").click();
    h.run_steps(3);
    assert!(h.state().session.state().unwrap().block_editor().is_none());
    assert_eq!(h.state().session.doc().unwrap().block("Saved").unwrap().entities.handles().len(), 1);
    assert_eq!(h.state().ui.ribbon_tab, "Output");
}

#[test]
fn tabledit_context_preserves_pending_cell_input() {
    let mut h = harness();
    h.state_mut().run("table", json!({"at": [0,0], "rows": 2, "cols": 2})).unwrap();
    h.state_mut().start("tabledit");
    h.state_mut().cmd.buffer = "pending cell".into();
    h.run_steps(3);
    assert_eq!(h.state().ui.ribbon_tab, "Table Tools");
    h.get_by_label("Insert Row").click();
    h.run_steps(3);
    assert_eq!(h.state().cmd.buffer, "pending cell");
    assert_eq!(h.state().session.running.as_ref().map(|r| r.id.as_str()), Some("tabledit"));
    h.state_mut().session.cancel();
    h.run_steps(3);
    assert_eq!(h.state().ui.ribbon_tab, "Output");
}

#[test]
fn narrow_windows_scroll_the_active_context_into_view_without_moving_panel_rows() {
    let mut app = CadApp::new(Session::new(), Services::default());
    app.run("bedit", json!({"name": "Narrow", "create": true})).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(320.0, 400.0)).build_ui_state(|ui, app: &mut CadApp| ribbon::show(app, ui), app);
    h.run_steps(30);
    let tab = h.get(By::new().role(Role::Tab).label("Block Editor")).rect();
    assert!(tab.left() >= 0.0 && tab.right() <= 320.0, "active context must be on screen: {tab:?}");
    let save = h.get_by_label("Save Block").rect();
    let close = h.get_by_label("Close Block Editor").rect();
    assert!((save.top() - close.top()).abs() < 1.0);
    assert!(save.top() > tab.bottom());
}

#[test]
fn ambiguous_and_locked_selections_do_not_enable_table_mutations() {
    use egui_kittest::kittest::NodeT;
    let mut h = harness();
    let result = h.state_mut().run("table", json!({"at": [0,0], "rows": 2, "cols": 2})).unwrap();
    let table = cadcraft_engine::doc::Handle::parse_hex(result["handle"].as_str().unwrap()).unwrap();
    let result = h.state_mut().run("line", json!({"points": [[0,0],[2,0]]})).unwrap();
    let line = cadcraft_engine::doc::Handle::parse_hex(result["handles"][0].as_str().unwrap()).unwrap();
    h.state_mut().session.set_selection(vec![table, line]);
    h.run_steps(3);
    assert_eq!(h.state().ui.ribbon_tab, "Output");
    h.state_mut().session.set_selection(vec![table]);
    h.run_steps(3);
    let layer = h.state().session.doc().unwrap().entity(table).unwrap().common.layer.clone();
    h.state_mut().session.doc_mut().unwrap().layer_mut(&layer).unwrap().locked = true;
    h.run_steps(3);
    assert!(h.get_by_label("Insert Row").accesskit_node().is_disabled());
}

#[test]
fn block_editor_tab_appears_and_restores_the_previous_normal_tab() {
    let mut h = harness();
    assert!(h.query(By::new().role(Role::Tab).label("Block Editor")).is_none());
    h.state_mut().run("bedit", json!({"name": "Ribbon block", "create": true})).unwrap();
    h.run_steps(3);
    h.get(By::new().role(Role::Tab).label("Block Editor"));
    assert_eq!(h.state().ui.ribbon_tab, "Block Editor");
    h.state_mut().run("bclose", json!({"save": false})).unwrap();
    h.run_steps(3);
    assert!(h.query(By::new().role(Role::Tab).label("Block Editor")).is_none());
    assert_eq!(h.state().ui.ribbon_tab, "Output");
}

#[test]
fn selected_table_context_can_append_a_row_and_column() {
    let mut h = harness();
    let result = h.state_mut().run("table", json!({"at": [0,0], "rows": 2, "cols": 2})).unwrap();
    let handle = cadcraft_engine::doc::Handle::parse_hex(result["handle"].as_str().unwrap()).unwrap();
    h.state_mut().session.set_selection(vec![handle]);
    h.run_steps(3);
    assert_eq!(h.state().ui.ribbon_tab, "Table Tools");
    assert!(h.query_by_label("Insert Row").is_some(), "selected tables need real row controls");
    h.get_by_label("Insert Row").click();
    h.run_steps(3);
    h.get_by_label("Insert Column").click();
    h.run_steps(3);
    let doc = h.state().session.doc().unwrap();
    let cadcraft_engine::doc::EntityKind::Table(table) = &doc.entity(handle).unwrap().kind else { panic!("table expected") };
    assert_eq!((table.row_heights.len(), table.col_widths.len()), (3, 3));
    h.state_mut().session.set_selection(vec![]);
    h.run_steps(3);
    assert_eq!(h.state().ui.ribbon_tab, "Output");
}

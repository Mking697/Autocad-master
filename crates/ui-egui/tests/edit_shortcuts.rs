//! Windows/browser Ctrl edit shortcuts operate on the drawing, unless a text field owns focus.
use cadcraft_engine::Session;
use cadcraft_ui_egui::{CadApp, Services};
use egui::{Key, Modifiers, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::json;

fn harness() -> Harness<'static, CadApp> {
    let mut app = CadApp::new(Session::new(), Services::default());
    app.ui.show_ribbon = true;
    app.ui.in_window_menu = false;
    app.run("line", json!({"points": [[0,0], [2,0]]})).unwrap();
    app.run("circle", json!({"center": [5,5], "radius": 1})).unwrap();
    let mut h = Harness::builder().with_size(vec2(1280.0, 800.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        app,
    );
    h.ctx.set_os(egui::os::OperatingSystem::Windows);
    h.run_steps(5);
    h
}

fn ctrl(h: &mut Harness<'_, CadApp>, key: Key) {
    h.key_press_modifiers(Modifiers::CTRL | Modifiers::COMMAND, key);
    h.run_steps(3);
}

#[test]
fn ctrl_a_delete_ctrl_z_ctrl_y_roundtrip_the_real_drawing() {
    let mut h = harness();
    assert_eq!(h.state().session.doc().unwrap().model.len(), 2);
    ctrl(&mut h, Key::A);
    assert_eq!(h.state().session.selection().len(), 2);
    h.key_press(Key::Delete);
    h.run_steps(3);
    assert!(h.state().session.doc().unwrap().model.is_empty());
    ctrl(&mut h, Key::Z);
    assert_eq!(h.state().session.doc().unwrap().model.len(), 2);
    ctrl(&mut h, Key::Y);
    assert!(h.state().session.doc().unwrap().model.is_empty());
    ctrl(&mut h, Key::Z);
    assert_eq!(h.state().session.doc().unwrap().model.len(), 2);
}

#[test]
fn focused_search_owns_ctrl_a_and_edit_history_without_modifying_the_drawing() {
    let mut h = harness();
    let drawing = h.state().session.doc().unwrap().clone();
    h.get_by_label("Command search").click();
    h.run_steps(3);
    h.get_by_label("Command search").type_text("line");
    h.run_steps(40);
    assert!(h.ctx.egui_wants_keyboard_input());
    ctrl(&mut h, Key::A);
    assert!(h.state().session.selection().is_empty());
    h.get_by_label("Command search").type_text("circle");
    h.run_steps(40);
    assert_eq!(h.get_by_label("Command search").value().as_deref(), Some("circle"));
    ctrl(&mut h, Key::Z);
    assert_eq!(h.get_by_label("Command search").value().as_deref(), Some("line"));
    ctrl(&mut h, Key::Y);
    assert_eq!(h.get_by_label("Command search").value().as_deref(), Some("circle"));
    assert_eq!(h.state().session.doc().unwrap(), &drawing);
    assert!(h.state().session.running.is_none());
}

//! Exercise the actual ribbon Info Bar without the rest of the application chrome.

use cadcraft_engine::Session;
use cadcraft_ui_egui::{CadApp, Services, infobar, theme};
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

fn harness(app: CadApp, width: f32) -> Harness<'static, CadApp> {
    let mut h = Harness::builder().with_size(egui::vec2(width, 180.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            theme::apply(ui.ctx(), egui::Theme::Dark, false);
            infobar::show(app, ui);
        },
        app,
    );
    h.run_steps(3);
    h
}

fn enter(h: &mut Harness<'_, CadApp>, text: &str) {
    h.get_by_role_and_label(Role::TextInput, "Command search").click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::TextInput, "Command search").type_text(text);
    h.run_steps(2);
}

#[test]
fn enter_in_search_starts_command_and_returns_keyboard_to_drafting() {
    let mut app = CadApp::new(Session::new(), Services::default());
    app.ui.show_ribbon = true;
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 800.0)).build_ui_state(|ui, app: &mut CadApp| app.ui(ui), app);
    h.run_steps(5);
    enter(&mut h, "line");
    h.state_mut().cmd.buffer = "stale command-line input".into();
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert!(h.state().cmd.buffer.is_empty());
    assert_eq!(h.state().session.running.as_ref().map(|r| r.id.as_str()), Some("line"));
    assert_eq!(h.get_by_role_and_label(Role::TextInput, "Command search").value().as_deref(), Some(""));
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(!h.state().session.prompting());
}

#[test]
fn known_alias_starts_the_real_interactive_command() {
    let mut h = harness(CadApp::new(Session::new(), Services::default()), 1100.0);
    enter(&mut h, "  L  ");
    h.get_by_role_and_label(Role::Button, "Run").click();
    h.run_steps(3);
    assert_eq!(h.state().session.last_command.as_deref(), Some("line"));
    assert!(h.state().session.prompting());
    assert_eq!(h.get_by_role_and_label(Role::TextInput, "Command search").value().as_deref(), Some(""));
}

#[test]
fn empty_and_unknown_search_do_not_cancel_the_running_command() {
    let mut app = CadApp::new(Session::new(), Services::default());
    app.start("line");
    let mut h = harness(app, 1100.0);
    let log = h.state().session.log.clone();
    enter(&mut h, "   ");
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(h.state().session.log, log);
    assert!(h.state().session.prompting());

    enter(&mut h, "not-a-cadcraft-command");
    h.get_by_role_and_label(Role::Button, "Run").click();
    h.run_steps(3);
    h.get_by_label("Unknown command");
    assert_eq!(h.state().session.log, log);
    assert_eq!(h.state().session.last_command.as_deref(), Some("line"));
    assert!(h.state().session.prompting());
}

#[test]
fn registered_ui_command_uses_app_dispatch() {
    let mut h = harness(CadApp::new(Session::new(), Services::default()), 1100.0);
    enter(&mut h, "ui.dialog.about");
    h.get_by_role_and_label(Role::Button, "Run").click();
    h.run_steps(3);
    assert_eq!(h.state().ui.dialog.as_deref(), Some("about"));
}

#[test]
fn readout_tracks_document_dirty_state_and_paper_space() {
    let mut app = CadApp::new(Session::new(), Services::default());
    app.session.state_mut().unwrap().title = "Info Bar drawing.dxf".into();
    let mut h = harness(app, 1100.0);
    h.get_by_label("Info Bar drawing.dxf");
    h.get_by_label("Model");
    assert!(h.query_by_label("*").is_none());
    h.state_mut().run("line", json!({"points": [[0, 0], [10, 0]]})).unwrap();
    h.state_mut().run("layout.new", json!({"name": "Sheet A"})).unwrap();
    h.state_mut().run("layout.set", json!({"name": "Sheet A"})).unwrap();
    h.run_steps(3);
    h.get_by_label("*");
    h.get_by_label("Paper: Sheet A");
    h.state_mut().session.docs.clear();
    h.run_steps(3);
    h.get_by_label("No active document");
    assert!(h.query_by_label("Info Bar drawing.dxf").is_none());
}

#[test]
fn hostile_paste_is_bounded_and_narrow_layout_keeps_one_row() {
    let mut app = CadApp::new(Session::new(), Services::default());
    app.session.state_mut().unwrap().title = "é".repeat(4096);
    let mut h = harness(app, 1100.0);
    enter(&mut h, &"x".repeat(4096));
    let value = h.get_by_role_and_label(Role::TextInput, "Command search").value().unwrap();
    assert_eq!(value.chars().count(), 128);
    h.get_by_role_and_label(Role::Button, "Run").click();
    h.run_steps(3);
    assert!(h.state().session.last_command.is_none());
    h.get_by_label("Unknown command");

    let h = harness(CadApp::new(Session::new(), Services::default()), 320.0);
    let field = h.get_by_role_and_label(Role::TextInput, "Command search").rect();
    let run = h.get_by_role_and_label(Role::Button, "Run").rect();
    assert!((field.center().y - run.center().y).abs() < 2.0, "narrow windows scroll rather than wrapping Run below the field");
    assert!(run.left() >= field.right());
    assert!(field.height() <= 32.0);
}

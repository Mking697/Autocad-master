//! Window ▸ Ribbon shows the AutoCAD-for-Windows style Ribbon; toggling it off restores the
//! Mac-style chrome without either crashing or duplicating a Tool Sets command's label.

use cadcraft_engine::Session;
use cadcraft_ui_egui::{CadApp, Services};
use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

fn harness() -> Harness<'static, CadApp> {
    let mut app = CadApp::new(Session::default(), Services::default());
    app.run("line", json!({"points": [[0, 0], [10, 0]]})).unwrap();
    app.ui.in_window_menu = false;
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

/// Off by default: the Tool Sets palette's own "Line" button is the only one.
#[test]
fn the_ribbon_is_off_by_default_and_tool_sets_renders_as_before() {
    let h = harness();
    assert!(!h.state().ui.show_ribbon);
    h.get_by_label("Line");
}

/// Turning the Ribbon on adds its own tabs and Home panels, without crashing and without a
/// control that collides with Tool Sets' own (both would otherwise claim the "Line" label). Every
/// panel sits in the same row (#U12: a wrapping layout once pushed Modify onto a second, clipped
/// row, so only Draw ever showed).
#[test]
fn the_ribbon_shows_its_tabs_and_home_panels_once_turned_on() {
    let mut h = harness();
    h.state_mut().ui.show_ribbon = true;
    h.state_mut().ui.show_toolsets = false;
    h.run_steps(3);
    for tab in cadcraft_ui_egui::ribbon::TABS {
        h.get_by_label(tab);
    }
    h.get_by_label("Line");
    h.get_by_label("Draw");
    h.get_by_label("Modify");
    let draw_y = h.get_by_label("Line").rect().top();
    let move_button = h.get_by_label("Move").rect();
    assert!((move_button.top() - draw_y).abs() < 1.0, "Modify's Move button should share Draw's row, not wrap to one below it");
    // #U12 round 2: a vertical panel column was sized to the whole (near-infinite) ScrollArea
    // content width instead of shrinking to its own buttons, leaving a huge gap before the next
    // panel. Polygon (Draw's last tool) and Move (Modify's first) should sit close together.
    let polygon_right = h.get_by_label("Polygon").rect().right();
    let move_left = move_button.left();
    assert!(
        (0.0..60.0).contains(&(move_left - polygon_right)),
        "Modify's panel should start just after Draw's last tool, not leave a wide gap (gap was {})",
        move_left - polygon_right
    );
}

/// Clicking a Ribbon tool starts its command, the same as the matching Tool Sets button.
#[test]
fn clicking_a_ribbon_tool_starts_its_command() {
    let mut h = harness();
    h.state_mut().ui.show_ribbon = true;
    h.state_mut().ui.show_toolsets = false;
    h.run_steps(3);
    h.get_by_label("Circle").click();
    h.run_steps(3);
    assert_eq!(h.state().session.running.as_ref().map(|r| r.id.as_str()), Some("circle"));
}

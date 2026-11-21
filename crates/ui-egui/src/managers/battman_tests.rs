use super::*;
use crate::Services;
use cadcraft_engine::Session;

fn app() -> CadApp {
    let mut app = CadApp::new(Session::new(), Services::default());
    let result = app.session.execute("attdef", &json!({"tag": "NAME", "at": [0,0], "height": 1})).unwrap();
    app.session.execute("block", &json!({"name": "TEST", "base": [0,0], "handles": [result["handle"]], "keep": "convert"})).unwrap();
    app
}

fn frame(app: &mut CadApp, ctx: &egui::Context, events: Vec<egui::Event>) -> (egui::FullOutput, Option<(&'static str, Value)>) {
    let mut action = None;
    let out = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1000.0, 900.0))),
            events,
            focused: true,
            ..Default::default()
        },
        |ui| {
            action = dialog(app, ui.ctx(), &mut true);
        },
    );
    (out, action)
}

fn label_pos(out: &egui::FullOutput, label: &str) -> egui::Pos2 {
    out.shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::epaint::Shape::Text(t) if t.galley.text() == label => Some(t.pos + t.galley.size() * 0.5),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing widget label {label}"))
}

fn click(app: &mut CadApp, ctx: &egui::Context, pos: egui::Pos2) -> Option<(&'static str, Value)> {
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

fn edit_multiline_pos(out: &egui::FullOutput) -> egui::Pos2 {
    // The upper Multiline checkbox belongs to New tag, not the selected definition.
    out.shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::epaint::Shape::Text(t) if t.galley.text() == "Multiline" => Some(t.pos + t.galley.size() * 0.5),
            _ => None,
        })
        .max_by(|a, b| a.y.total_cmp(&b.y))
        .expect("selected-definition Multiline checkbox")
}

fn type_then_click_multiline(app: &mut CadApp, ctx: &egui::Context, label: &str, field: &str, text: &str) -> (&'static str, Value) {
    for _ in 0..3 {
        frame(app, ctx, Vec::new()).0.drop_without_applying_deltas();
    }
    let (out, _) = frame(app, ctx, Vec::new());
    // Default also labels a table column; the lower label is the editable field.
    let pos = out
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::epaint::Shape::Text(t) if t.galley.text() == label => Some(t.pos + t.galley.size() * 0.5),
            _ => None,
        })
        .max_by(|a, b| a.y.total_cmp(&b.y))
        .expect("edit field label")
        + vec2(120.0, 0.0);
    out.drop_without_applying_deltas();
    assert!(click(app, ctx, pos).is_none());
    let id = egui::Id::unique((field, "TEST", "NAME")).with("te");
    assert!(ctx.memory(|m| m.has_focus(id)), "click must focus {label}");
    let (out, action) = frame(
        app,
        ctx,
        vec![
            egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers { ctrl: true, command: true, ..Default::default() },
            },
            egui::Event::Text(text.to_string()),
        ],
    );
    assert!(action.is_none(), "typing must not commit before focus loss");
    let pos = edit_multiline_pos(&out);
    out.drop_without_applying_deltas();
    // No Enter or intervening focus change: the checkbox click must commit both edits.
    click(app, ctx, pos).expect("focus loss and checkbox return one command")
}

#[test]
fn battman_default_focus_loss_and_multiline_click_preserve_both_edits() {
    let mut app = app();
    let ctx = egui::Context::default();
    let value = "Typed default before clicking Multiline";
    let (cmd, params) = type_then_click_multiline(&mut app, &ctx, "Default", "battman_default", value);
    assert_eq!(cmd, "battman");
    assert_eq!(params, json!({"name": "TEST", "action": "edit", "tag": "NAME", "default": value, "multiline": true, "sync": true}));
    // Assert the combined command before executing it, then inspect the real drawing.
    app.session.execute(cmd, &params).unwrap();
    let definition = |app: &CadApp| {
        app.session
            .doc()
            .unwrap()
            .block("TEST")
            .unwrap()
            .entities
            .iter()
            .find_map(|e| match &e.kind {
                EntityKind::AttDef(a) if a.tag == "NAME" => Some(a.clone()),
                _ => None,
            })
            .unwrap()
    };
    let a = definition(&app);
    assert_eq!(a.text.value, value);
    let ml = a.multiline.as_ref().expect("checkbox created MTEXT");
    assert_eq!(ml.mtext.contents, value);
    assert_eq!(ml.read_as, value);
    // Switch back using the actual checkbox and prove the default survives the roundtrip.
    for _ in 0..3 {
        frame(&mut app, &ctx, Vec::new()).0.drop_without_applying_deltas();
    }
    let (out, _) = frame(&mut app, &ctx, Vec::new());
    let pos = edit_multiline_pos(&out);
    out.drop_without_applying_deltas();
    let (cmd, params) = click(&mut app, &ctx, pos).expect("switch back to single line");
    assert_eq!(params["multiline"], false);
    app.session.execute(cmd, &params).unwrap();
    let a = definition(&app);
    assert_eq!(a.text.value, value);
    assert!(a.multiline.is_none());
}

#[test]
fn battman_sibling_focus_loss_and_multiline_click_preserve_both_edits() {
    for (label, field, text, key, expected) in
        [("Prompt", "battman_prompt", "New prompt", "prompt", json!("New prompt")), ("Height", "battman_height", "2", "height", json!(2.0))]
    {
        let mut app = app();
        let ctx = egui::Context::default();
        let (cmd, params) = type_then_click_multiline(&mut app, &ctx, label, field, text);
        assert_eq!(cmd, "battman");
        assert_eq!(params["action"], "edit");
        assert_eq!(params[key], expected, "lost {label}: {params}");
        assert_eq!(params["multiline"], true);
        app.session.execute(cmd, &params).unwrap();
        let d = app.session.doc().unwrap();
        let a = d
            .block("TEST")
            .unwrap()
            .entities
            .iter()
            .find_map(|e| match &e.kind {
                EntityKind::AttDef(a) if a.tag == "NAME" => Some(a),
                _ => None,
            })
            .unwrap();
        assert!(a.multiline.is_some());
        if key == "prompt" {
            assert_eq!(a.prompt, "New prompt");
        } else {
            assert_eq!(a.text.height, 2.0);
        }
    }
}

#[test]
fn battman_dialog_creates_multiline_and_switches_both_directions_via_commands() {
    let mut app = app();
    let ctx = egui::Context::default();
    // Set the draft only, not the drawing. Click the actual dialog's New button.
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(("battman_new_tag", "TEST")), "NOTES".to_string()));
    set_temp(&ctx, "battman_new_multiline", true);
    let (out, _) = frame(&mut app, &ctx, Vec::new());
    out.drop_without_applying_deltas();
    let (out, _) = frame(&mut app, &ctx, Vec::new());
    let pos = label_pos(&out, "New");
    out.drop_without_applying_deltas();
    let (cmd, params) = click(&mut app, &ctx, pos).expect("New returns a command");
    assert_eq!(cmd, "battman");
    assert_eq!(params["multiline"], true);
    app.session.execute(cmd, &params).unwrap();
    let definition = |app: &CadApp| -> bool {
        app.session
            .doc()
            .unwrap()
            .block("TEST")
            .unwrap()
            .entities
            .iter()
            .find_map(|e| match &e.kind {
                EntityKind::AttDef(a) if a.tag == "NOTES" => Some(a.multiline.is_some()),
                _ => None,
            })
            .unwrap()
    };
    assert!(definition(&app));
    for expected in [false, true] {
        // Grid column widths and window size settle after the selected row/type changes.
        for _ in 0..3 {
            frame(&mut app, &ctx, Vec::new()).0.drop_without_applying_deltas();
        }
        let (out, _) = frame(&mut app, &ctx, Vec::new());
        // The draft checkbox and selected-definition checkbox share their visible label;
        // select the latter (the lower one).
        let pos = out
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::epaint::Shape::Text(t) if t.galley.text() == "Multiline" => Some(t.pos + t.galley.size() * 0.5),
                _ => None,
            })
            .max_by(|a, b| a.y.total_cmp(&b.y))
            .expect("edit multiline checkbox");
        out.drop_without_applying_deltas();
        let (cmd, params) = click(&mut app, &ctx, pos).expect("checkbox returns command");
        assert_eq!(params["action"], "edit");
        assert_eq!(params["multiline"], expected, "returned action: {params}");
        app.session.execute(cmd, &params).unwrap();
        assert_eq!(definition(&app), expected);
    }
}

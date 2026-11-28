//! BPARAMETER + BACTIONTOOL + BSAVE: the smallest vertical slice of dynamic-block authoring
//! (`docs/gaps.md`'s Top-10 #4 / F3). A Linear parameter and its paired Stretch action are
//! authored while editing a block, saved, and read back exactly as this crate's own reader
//! (`cadcraft_doc::dynblock`) reads a dynamic block authored by another program — including a
//! working stretch grip on a reference to it. The DXF round trip proves the written bytes, not
//! just the in-memory structures.

use cadcraft_doc::dynblock::{self, GripPart};
use cadcraft_doc::{EntityKind, Handle, Line};
use serde_json::json;

use cadcraft_engine::Session;

/// A "DOOR2" block: two lines, (0,0)-(2,0) (the leaf's width) and (2,0)-(2,1) (its swing, which
/// should move with the stretch), base point (0,0); and a reference to it at (100, 0).
fn door_block() -> (Session, Handle, Handle) {
    let mut s = Session::new();
    let l1 = s.execute("line", &json!({ "points": [[0.0, 0.0], [2.0, 0.0]] })).unwrap()["handles"][0].as_str().unwrap().to_string();
    let l2 = s.execute("line", &json!({ "points": [[2.0, 0.0], [2.0, 1.0]] })).unwrap()["handles"][0].as_str().unwrap().to_string();
    s.execute("block", &json!({ "name": "DOOR2", "base": [0.0, 0.0], "handles": [l1, l2], "keep": "retain" })).unwrap();
    let r = s.execute("insert", &json!({ "name": "DOOR2", "at": [100.0, 0.0] })).unwrap()["handle"].as_str().unwrap().to_string();
    let r = Handle::parse_hex(&r).unwrap();
    let _ = l2;
    // BLOCK gives the block's copies fresh handles: find the swing line (2,0)-(2,1) among them.
    let swing = s
        .doc()
        .unwrap()
        .block("DOOR2")
        .unwrap()
        .entities
        .iter()
        .find(|e| matches!(&e.kind, EntityKind::Line(l) if l.a.x == 2.0 && l.b.y == 1.0))
        .unwrap()
        .handle;
    (s, r, swing)
}

/// The swing line in block `name` (the one not on the 0..2 width segment, found by shape since
/// a stretched reference's anonymous block copies its lines under fresh handles).
fn swing(d: &cadcraft_doc::Drawing, name: &str) -> Line {
    let b = d.block(name).unwrap().clone();
    let lines: Vec<Line> = b.entities.iter().filter_map(|e| if let EntityKind::Line(l) = &e.kind { Some(l.clone()) } else { None }).collect();
    lines.into_iter().find(|l| l.a.y != l.b.y).expect("no swing line")
}

#[test]
fn bparameter_bactiontool_bsave_round_trips_through_dxf_and_the_grip_works() {
    let (mut s, reference, _swing_in_block) = door_block();

    // Edit the block: author a Linear parameter (Width, its end at (2, 0)) and pair a Stretch
    // action that moves the swing line (the leaf's own width line is not in the selection).
    s.execute("bedit", &json!({ "name": "DOOR2" })).unwrap();
    let swing_in_editor = {
        let b = s.doc().unwrap();
        let l = b.model.iter().find(|e| matches!(&e.kind, EntityKind::Line(l) if l.a.x == 2.0 && l.b.y == 1.0)).unwrap();
        l.handle
    };
    let r = s.execute("bparameter", &json!({ "type": "linear", "base": [0.0, 0.0], "end": [2.0, 0.0], "label": "Width" })).unwrap();
    assert_eq!(r["node"], 1);
    let r = s
        .execute(
            "bactiontool",
            &json!({ "type": "stretch", "handles": [swing_in_editor.hex()], "frame": [[1.0, -1.0], [3.0, 2.0]], "name": "Stretch1" }),
        )
        .unwrap();
    assert_eq!(r["node"], 2);
    s.execute("bsave", &json!({})).unwrap();
    s.execute("bclose", &json!({})).unwrap();

    // In the same session, it already reads as a dynamic block with a working grip.
    let m = dynblock::dynamic_block(s.doc().unwrap(), "DOOR2").expect("DOOR2 is now a dynamic block");
    assert_eq!(m.parameters.len(), 1);
    assert_eq!(m.actions.len(), 1);
    let g = dynblock::grips(s.doc().unwrap(), reference);
    let end = g.iter().find(|g| g.part == GripPart::End).expect("the end grip is shown");
    assert_eq!((end.at.x, end.at.y), (102.0, 0.0), "reference at (100,0), parameter end at (2,0)");

    // The real proof: write it to DXF bytes and reparse them, exactly as a saved file reopens.
    let dxf = cadcraft_io::write_dxf(s.doc().unwrap());
    let reopened = cadcraft_io::read_dxf(dxf.as_bytes()).unwrap();
    let m2 = dynblock::dynamic_block(&reopened, "DOOR2").expect("the dynamic block survives the DXF round trip");
    assert_eq!(m2.parameters.len(), 1);
    assert_eq!(m2.actions.len(), 1);
    let p = &m2.parameters[0];
    let dynblock::ParameterKind::Linear { base, end, label, from_midpoint, .. } = &p.kind else { panic!("{p:?}") };
    assert_eq!((base.x, base.y, end.x, end.y, *from_midpoint), (0.0, 0.0, 2.0, 0.0, false));
    assert_eq!(label, "Width");
    let a = &m2.actions[0];
    assert_eq!(a.connections, [(p.node, "EndXDelta".to_string()), (p.node, "EndYDelta".to_string())]);
    let dynblock::ActionKind::Stretch { .. } = &a.kind else { panic!("{a:?}") };

    // Dragging the reopened reference's end grip to (105, 0) stretches the swing line (+3 in x)
    // and leaves the leaf's own width line where it was (it isn't in the action's selection).
    let mut reopened = reopened;
    let g = dynblock::grips(&reopened, reference);
    let end = g.iter().find(|g| g.part == GripPart::End).expect("the end grip is shown after reopening");
    dynblock::drag_grip(&mut reopened, reference, end.node, GripPart::End, cadcraft_geom::Vec2::new(105.0, 0.0)).unwrap();
    let EntityKind::Insert(ins) = &reopened.entity(reference).unwrap().kind else { panic!("not a reference") };
    let swung = swing(&reopened, &ins.block);
    assert_eq!((swung.a.x, swung.a.y, swung.b.x, swung.b.y), (5.0, 0.0, 5.0, 1.0), "the swing line moved by the drag");
}

#[test]
fn bparameter_point_bactiontool_move_bsave_round_trips_through_dxf_and_the_grip_works() {
    let (mut s, reference, _swing_in_block) = door_block();

    // Edit the block: author a Point parameter at the swing's corner (2, 0) and pair a Move
    // action that moves the swing line whole (the leaf's own width line is not in the selection).
    s.execute("bedit", &json!({ "name": "DOOR2" })).unwrap();
    let swing_in_editor = {
        let b = s.doc().unwrap();
        let l = b.model.iter().find(|e| matches!(&e.kind, EntityKind::Line(l) if l.a.x == 2.0 && l.b.y == 1.0)).unwrap();
        l.handle
    };
    let r = s.execute("bparameter", &json!({ "type": "point", "at": [2.0, 0.0], "label": "Position1" })).unwrap();
    assert_eq!(r["node"], 1);
    let r = s.execute("bactiontool", &json!({ "type": "move", "handles": [swing_in_editor.hex()], "name": "Move1" })).unwrap();
    assert_eq!(r["node"], 2);
    s.execute("bsave", &json!({})).unwrap();
    s.execute("bclose", &json!({})).unwrap();

    // In the same session, it already reads as a dynamic block with a working grip.
    let m = dynblock::dynamic_block(s.doc().unwrap(), "DOOR2").expect("DOOR2 is now a dynamic block");
    assert_eq!(m.parameters.len(), 1);
    assert_eq!(m.actions.len(), 1);
    let g = dynblock::grips(s.doc().unwrap(), reference);
    let at = g.iter().find(|g| g.part == GripPart::At).expect("the position grip is shown");
    assert_eq!((at.at.x, at.at.y), (102.0, 0.0), "reference at (100,0), parameter at (2,0)");

    // The real proof: write it to DXF bytes and reparse them, exactly as a saved file reopens.
    let dxf = cadcraft_io::write_dxf(s.doc().unwrap());
    let reopened = cadcraft_io::read_dxf(dxf.as_bytes()).unwrap();
    let m2 = dynblock::dynamic_block(&reopened, "DOOR2").expect("the dynamic block survives the DXF round trip");
    assert_eq!(m2.parameters.len(), 1);
    assert_eq!(m2.actions.len(), 1);
    let p = &m2.parameters[0];
    let dynblock::ParameterKind::Point { at, label, .. } = &p.kind else { panic!("{p:?}") };
    assert_eq!((at.x, at.y), (2.0, 0.0));
    assert_eq!(label, "Position1");
    let a = &m2.actions[0];
    let dynblock::ActionKind::Move { .. } = &a.kind else { panic!("{a:?}") };

    // Dragging the reopened reference's position grip to (105, 3) moves the swing line by
    // (+3, +3) and leaves the leaf's own width line where it was (not in the action's selection).
    let mut reopened = reopened;
    let g = dynblock::grips(&reopened, reference);
    let at = g.iter().find(|g| g.part == GripPart::At).expect("the position grip is shown after reopening");
    dynblock::drag_grip(&mut reopened, reference, at.node, GripPart::At, cadcraft_geom::Vec2::new(105.0, 3.0)).unwrap();
    let EntityKind::Insert(ins) = &reopened.entity(reference).unwrap().kind else { panic!("not a reference") };
    let swung = swing(&reopened, &ins.block);
    assert_eq!((swung.a.x, swung.a.y, swung.b.x, swung.b.y), (5.0, 3.0, 5.0, 4.0), "the swing line moved by the drag");
}

#[test]
fn bparameter_visibility_bsave_round_trips_through_dxf_and_set_property_hides_the_line() {
    let (mut s, reference, _swing_in_block) = door_block();

    // Edit the block: author a Visibility parameter with two states — "On" shows the swing line,
    // "Off" hides it (the leaf's own width line is not controlled, so it stays visible either way).
    s.execute("bedit", &json!({ "name": "DOOR2" })).unwrap();
    let swing_in_editor = {
        let b = s.doc().unwrap();
        let l = b.model.iter().find(|e| matches!(&e.kind, EntityKind::Line(l) if l.a.x == 2.0 && l.b.y == 1.0)).unwrap();
        l.handle
    };
    let r = s
        .execute(
            "bparameter",
            &json!({
                "type": "visibility",
                "at": [2.0, 0.0],
                "label": "Visibility1",
                "handles": [swing_in_editor.hex()],
                "states": [
                    { "name": "On", "visible": [swing_in_editor.hex()] },
                    { "name": "Off", "visible": [] },
                ],
            }),
        )
        .unwrap();
    assert_eq!(r["node"], 1);
    s.execute("bsave", &json!({})).unwrap();
    s.execute("bclose", &json!({})).unwrap();

    // In the same session, it already reads as a dynamic block with no paired action.
    let m = dynblock::dynamic_block(s.doc().unwrap(), "DOOR2").expect("DOOR2 is now a dynamic block");
    assert_eq!(m.parameters.len(), 1);
    assert_eq!(m.actions.len(), 0, "a Visibility parameter authors with no paired action");

    // The real proof: write it to DXF bytes and reparse them, exactly as a saved file reopens.
    let dxf = cadcraft_io::write_dxf(s.doc().unwrap());
    let reopened = cadcraft_io::read_dxf(dxf.as_bytes()).unwrap();
    let m2 = dynblock::dynamic_block(&reopened, "DOOR2").expect("the dynamic block survives the DXF round trip");
    assert_eq!(m2.parameters.len(), 1);
    let p = &m2.parameters[0];
    let dynblock::ParameterKind::Visibility { label, states, .. } = &p.kind else { panic!("{p:?}") };
    assert_eq!(label, "Visibility1");
    assert_eq!(states.len(), 2);
    assert_eq!(states[0].name, "On");
    assert_eq!(states[1].name, "Off");

    // Setting Visibility1 to "Off" on the reopened reference hides the swing line and leaves the
    // leaf's own width line (not a controlled object) alone.
    let mut reopened = reopened;
    dynblock::set_property(&mut reopened, reference, "Visibility1", "Off").unwrap();
    let EntityKind::Insert(ins) = &reopened.entity(reference).unwrap().kind else { panic!("not a reference") };
    let b = reopened.block(&ins.block).unwrap();
    let hidden = b.entities.iter().find(|e| matches!(&e.kind, EntityKind::Line(l) if l.a.y != l.b.y)).expect("the swing line's copy");
    assert!(!hidden.common.visible, "the swing line is hidden in the Off state");
    let still_shown = b.entities.iter().find(|e| matches!(&e.kind, EntityKind::Line(l) if l.a.y == l.b.y)).expect("the width line's copy");
    assert!(still_shown.common.visible, "the uncontrolled width line stays visible");
}

#[test]
fn bparameter_rotation_bactiontool_rotate_bsave_round_trips_through_dxf_and_the_grip_works() {
    let (mut s, reference, _swing_in_block) = door_block();

    // Edit the block: author a Rotation parameter about the base point (0,0), its grip starting
    // at (0,1) (radius 1, angle 90 degrees), and pair a Rotate action that turns the swing line
    // (the leaf's own width line is not in the selection, so it stays put).
    s.execute("bedit", &json!({ "name": "DOOR2" })).unwrap();
    let swing_in_editor = {
        let b = s.doc().unwrap();
        let l = b.model.iter().find(|e| matches!(&e.kind, EntityKind::Line(l) if l.a.x == 2.0 && l.b.y == 1.0)).unwrap();
        l.handle
    };
    let r = s.execute("bparameter", &json!({ "type": "rotation", "base": [0.0, 0.0], "end": [0.0, 1.0], "label": "Angle1" })).unwrap();
    assert_eq!(r["node"], 1);
    let r = s.execute("bactiontool", &json!({ "type": "rotate", "handles": [swing_in_editor.hex()], "name": "Rotate1" })).unwrap();
    assert_eq!(r["node"], 2);
    s.execute("bsave", &json!({})).unwrap();
    s.execute("bclose", &json!({})).unwrap();

    // In the same session, it already reads as a dynamic block with a working angle grip.
    let m = dynblock::dynamic_block(s.doc().unwrap(), "DOOR2").expect("DOOR2 is now a dynamic block");
    assert_eq!(m.parameters.len(), 1);
    assert_eq!(m.actions.len(), 1);
    let g = dynblock::grips(s.doc().unwrap(), reference);
    let end = g.iter().find(|g| g.part == GripPart::End).expect("the angle grip is shown");
    assert_eq!((end.at.x, end.at.y), (100.0, 1.0), "reference at (100,0), parameter end at (0,1)");

    // The real proof: write it to DXF bytes and reparse them, exactly as a saved file reopens.
    let dxf = cadcraft_io::write_dxf(s.doc().unwrap());
    let reopened = cadcraft_io::read_dxf(dxf.as_bytes()).unwrap();
    let m2 = dynblock::dynamic_block(&reopened, "DOOR2").expect("the dynamic block survives the DXF round trip");
    assert_eq!(m2.parameters.len(), 1);
    assert_eq!(m2.actions.len(), 1);
    let p = &m2.parameters[0];
    let dynblock::ParameterKind::Rotation { base, end, label, .. } = &p.kind else { panic!("{p:?}") };
    assert_eq!((base.x, base.y, end.x, end.y), (0.0, 0.0, 0.0, 1.0));
    assert_eq!(label, "Angle1");
    let a = &m2.actions[0];
    assert_eq!(a.connections, [(p.node, "AngleDelta".to_string())]);
    let dynblock::ActionKind::Rotate { base: action_base } = &a.kind else { panic!("{a:?}") };
    assert_eq!((action_base.x, action_base.y), (0.0, 0.0));

    // Dragging the reopened reference's angle grip from (100,1) (angle 90 degrees, reference base
    // at (100,0)) to (101,0) (angle 0 degrees) turns the parameter -90 degrees, which rotates the
    // swing line, (2,0)-(2,1) in block coordinates, by -90 degrees about the origin: (x,y) ->
    // (y,-x), giving (0,-2)-(1,-2); the leaf's own width line, not in the selection, is untouched.
    let mut reopened = reopened;
    let g = dynblock::grips(&reopened, reference);
    let end = g.iter().find(|g| g.part == GripPart::End).expect("the angle grip is shown after reopening");
    dynblock::drag_grip(&mut reopened, reference, end.node, GripPart::End, cadcraft_geom::Vec2::new(101.0, 0.0)).unwrap();
    let EntityKind::Insert(ins) = &reopened.entity(reference).unwrap().kind else { panic!("not a reference") };
    let b = reopened.block(&ins.block).unwrap();
    let lines: Vec<Line> = b.entities.iter().filter_map(|e| if let EntityKind::Line(l) = &e.kind { Some(l.clone()) } else { None }).collect();
    // The leaf's own width line is unselected and unchanged, (0,0)-(2,0); the other is the swung
    // one (both its points land at y = -2 after the -90-degree turn, unlike the width line's 0).
    let still = lines.iter().find(|l| l.a.y == 0.0 && l.b.y == 0.0).expect("the leaf's own width line's copy");
    assert_eq!((still.a.x, still.a.y, still.b.x, still.b.y), (0.0, 0.0, 2.0, 0.0), "the un-selected line is unchanged");
    let swung = lines.iter().find(|l| l.a.y != 0.0 || l.b.y != 0.0).expect("the swung swing line's copy");
    assert!((swung.a.x - 0.0).abs() < 1e-9 && (swung.a.y - (-2.0)).abs() < 1e-9, "{swung:?}");
    assert!((swung.b.x - 1.0).abs() < 1e-9 && (swung.b.y - (-2.0)).abs() < 1e-9, "{swung:?}");
}

#[test]
fn bparameter_flip_bactiontool_flip_bsave_round_trips_through_dxf_and_the_toggle_works() {
    let (mut s, reference, _swing_in_block) = door_block();

    // Edit the block: author a Flip parameter whose reflection line is the vertical line x = 2
    // (base (2,0) -> end (2,5)), and pair a Flip action that mirrors the leaf's own width line
    // (the swing line, (2,0)-(2,1), sits ON that line and so is not selected — it would stay put
    // either way, which wouldn't prove the mirror actually ran).
    s.execute("bedit", &json!({ "name": "DOOR2" })).unwrap();
    let width_in_editor = {
        let b = s.doc().unwrap();
        let l = b.model.iter().find(|e| matches!(&e.kind, EntityKind::Line(l) if l.a.x == 0.0 && l.b.x == 2.0 && l.a.y == 0.0)).unwrap();
        l.handle
    };
    let r = s.execute("bparameter", &json!({ "type": "flip", "base": [2.0, 0.0], "end": [2.0, 5.0], "label": "Flip1" })).unwrap();
    assert_eq!(r["node"], 1);
    let r = s.execute("bactiontool", &json!({ "type": "flip", "handles": [width_in_editor.hex()], "name": "Flip1" })).unwrap();
    assert_eq!(r["node"], 2);
    s.execute("bsave", &json!({})).unwrap();
    s.execute("bclose", &json!({})).unwrap();

    // In the same session, it already reads as a dynamic block with a working toggle grip.
    let m = dynblock::dynamic_block(s.doc().unwrap(), "DOOR2").expect("DOOR2 is now a dynamic block");
    assert_eq!(m.parameters.len(), 1);
    assert_eq!(m.actions.len(), 1);
    let g = dynblock::grips(s.doc().unwrap(), reference);
    let toggle = g.iter().find(|g| g.property == "Flip1").expect("the flip grip is shown");
    assert_eq!((toggle.at.x, toggle.at.y), (102.0, 2.5), "reference at (100,0), parameter line midpoint (2, 2.5)");

    // The real proof: write it to DXF bytes and reparse them, exactly as a saved file reopens.
    let dxf = cadcraft_io::write_dxf(s.doc().unwrap());
    let reopened = cadcraft_io::read_dxf(dxf.as_bytes()).unwrap();
    let m2 = dynblock::dynamic_block(&reopened, "DOOR2").expect("the dynamic block survives the DXF round trip");
    assert_eq!(m2.parameters.len(), 1);
    assert_eq!(m2.actions.len(), 1);
    let p = &m2.parameters[0];
    let dynblock::ParameterKind::Flip { base, end, label, base_state_label, flipped_state_label, .. } = &p.kind else { panic!("{p:?}") };
    assert_eq!((base.x, base.y, end.x, end.y), (2.0, 0.0, 2.0, 5.0));
    assert_eq!(label, "Flip1");
    assert_eq!((base_state_label.as_str(), flipped_state_label.as_str()), ("Not flipped", "Flipped"));
    let a = &m2.actions[0];
    assert_eq!(a.connections, [(p.node, "Flip".to_string())]);
    assert!(matches!(a.kind, dynblock::ActionKind::Flip));

    // Setting Flip1 to "Flipped" on the reopened reference mirrors the leaf's own width line
    // about x = 2: (0,0)-(2,0) -> (4,0)-(2,0); the swing line, on the reflection line and not
    // selected, is unchanged.
    let mut reopened = reopened;
    dynblock::set_property(&mut reopened, reference, "Flip1", "Flipped").unwrap();
    let EntityKind::Insert(ins) = &reopened.entity(reference).unwrap().kind else { panic!("not a reference") };
    let b = reopened.block(&ins.block).unwrap();
    let lines: Vec<Line> = b.entities.iter().filter_map(|e| if let EntityKind::Line(l) = &e.kind { Some(l.clone()) } else { None }).collect();
    let unselected = lines.iter().find(|l| l.a.x == 2.0 && l.b.y == 1.0).expect("the swing line's copy");
    assert_eq!((unselected.a.x, unselected.a.y, unselected.b.x, unselected.b.y), (2.0, 0.0, 2.0, 1.0));
    let mirrored = lines.iter().find(|l| (l.a.x - 4.0).abs() < 1e-9 || (l.b.x - 4.0).abs() < 1e-9).expect("the mirrored width line's copy");
    assert!((mirrored.a.x - 4.0).abs() < 1e-9 && (mirrored.a.y - 0.0).abs() < 1e-9, "{mirrored:?}");
    assert!((mirrored.b.x - 2.0).abs() < 1e-9 && (mirrored.b.y - 0.0).abs() < 1e-9, "{mirrored:?}");
}

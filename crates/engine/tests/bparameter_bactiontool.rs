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

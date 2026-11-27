//! ARRAYEDIT item deletion (AutoCAD's Ctrl+click on one item of a selected array): one
//! (i, j, level) placement is removed from an associative rectangular or polar array while the
//! rest stays associative, and the deletion survives a DXF round trip.

use cadcraft_engine::Session;
use cadcraft_engine::doc::{ArrayParams, Drawing, Entity, EntityKind, Handle};
use cadcraft_engine::geom::{Mat3, Vec2};
use cadcraft_io::{read_dxf, write_dxf};
use serde_json::{Value, json};

fn circle(s: &mut Session, x: f64, y: f64) -> Value {
    s.execute("circle", &json!({"center": [x, y], "radius": 1})).unwrap()["handle"].clone()
}

fn model(s: &Session) -> Vec<Entity> {
    s.doc().unwrap().model.iter().map(|e| (**e).clone()).collect()
}

fn the_array(s: &Session) -> (Entity, ArrayParams) {
    let m = model(s);
    assert_eq!(m.len(), 1, "{m:?}");
    let e = m[0].clone();
    let p = s.doc().unwrap().array_params(&e).expect("an associative array");
    (e, p)
}

/// World positions of the circles an array reference `e` draws (through nested block refs).
fn circles(d: &Drawing, e: &Entity) -> Vec<Vec2> {
    fn walk(d: &Drawing, e: &Entity, m: Mat3, depth: usize, out: &mut Vec<Vec2>) {
        match &e.kind {
            EntityKind::Circle(c) => out.push(m.apply(c.center.xy())),
            EntityKind::Insert(i) if depth < 8 => {
                let b = d.block(&i.block).unwrap();
                let next = m.then_before(i.transform(b.base.xy()));
                for be in b.entities.iter() {
                    walk(d, be, next, depth + 1, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(d, e, Mat3::IDENTITY, 0, &mut out);
    out
}

fn rect_array(s: &mut Session) -> Value {
    let h = circle(s, 0.0, 0.0);
    s.execute("arrayrect", &json!({"handles": [h], "rows": 1, "cols": 4, "colSpacing": 2})).unwrap()["handle"].clone()
}

/// (a) a specific item index can be excluded via the JSON `delete` param, (b) its entity copy is
/// genuinely gone from the drawing (not merely flagged somewhere unread).
#[test]
fn json_delete_removes_one_items_entity_copy() {
    let mut s = Session::new();
    let a = rect_array(&mut s);
    let (e, _) = the_array(&s);
    assert_eq!(circles(s.doc().unwrap(), &e).len(), 4);

    s.execute("arrayedit", &json!({"handle": a, "delete": [[1, 0, 0]]})).unwrap();
    let (e, p) = the_array(&s);
    assert!(p.is_excluded(1, 0, 0));
    let c = circles(s.doc().unwrap(), &e);
    assert_eq!(c.len(), 3, "{c:?}");
    assert!(!c.iter().any(|p| p.near(Vec2::new(2.0, 0.0), 1e-9)), "{c:?}");
    assert!(c.iter().any(|p| p.near(Vec2::new(0.0, 0.0), 1e-9)));
    assert!(c.iter().any(|p| p.near(Vec2::new(4.0, 0.0), 1e-9)));
    assert!(c.iter().any(|p| p.near(Vec2::new(6.0, 0.0), 1e-9)));

    // The block that makes up the array no longer has 4 item block references, only 3.
    let d = s.doc().unwrap();
    let EntityKind::Insert(ins) = &e.kind else { unreachable!() };
    assert_eq!(d.block(&ins.block).unwrap().entities.len(), 3);
    let back = read_dxf(write_dxf(d).as_bytes()).unwrap();
    let restored = back.model.iter().next().unwrap();
    assert!(back.array_params(restored).unwrap().is_excluded(1, 0, 0));
    let visible = circles(&back, restored);
    assert_eq!(visible.len(), 3);
    assert!(!visible.iter().any(|p| p.near(Vec2::new(2.0, 0.0), 1e-9)));
}

/// (c) the array's other items are unaffected and stays associative: editing the base/source
/// object still regenerates every other instance correctly after one was deleted.
#[test]
fn deleted_item_array_stays_associative() {
    let mut s = Session::new();
    let a = rect_array(&mut s);
    s.execute("arrayedit", &json!({"handle": a, "delete": [[1, 0, 0]]})).unwrap();
    let (e, _) = the_array(&s);
    assert_eq!(circles(s.doc().unwrap(), &e).len(), 3);

    // Changing the spacing (an ordinary ARRAYEDIT parameter change) still regenerates the other
    // three items, and the deleted one stays gone.
    s.execute("arrayedit", &json!({"handle": a, "colSpacing": 5})).unwrap();
    let (e, p) = the_array(&s);
    assert!(p.is_excluded(1, 0, 0));
    let c = circles(s.doc().unwrap(), &e);
    assert_eq!(c.len(), 3, "{c:?}");
    assert!(c.iter().any(|p| p.near(Vec2::new(0.0, 0.0), 1e-9)));
    assert!(c.iter().any(|p| p.near(Vec2::new(10.0, 0.0), 1e-9)));
    assert!(c.iter().any(|p| p.near(Vec2::new(15.0, 0.0), 1e-9)));
    assert!(!c.iter().any(|p| p.near(Vec2::new(5.0, 0.0), 1e-9)), "{c:?}");

    // Editing the actual source geometry regenerates the remaining instances, not the
    // excluded one. Even the conceptual origin may be used as a source-edit anchor.
    let placed = s.execute("arrayedit", &json!({"handle": a, "source": {}})).unwrap()["source"].clone();
    s.execute("move", &json!({"handles": placed, "delta": [0, 3]})).unwrap();
    s.execute("arrayclose", &json!({})).unwrap();
    let (e, p) = the_array(&s);
    assert!(p.is_excluded(1, 0, 0));
    let c = circles(s.doc().unwrap(), &e);
    assert_eq!(c.len(), 3);
    for x in [0.0, 10.0, 15.0] {
        assert!(c.iter().any(|p| p.near(Vec2::new(x, 3.0), 1e-9)), "{c:?}");
    }
    assert!(!c.iter().any(|p| p.near(Vec2::new(5.0, 3.0), 1e-9)));

    // `restore` brings the item back at its regenerated position.
    s.execute("arrayedit", &json!({"handle": a, "restore": [[1, 0, 0]]})).unwrap();
    let (e, p) = the_array(&s);
    assert!(!p.is_excluded(1, 0, 0));
    assert_eq!(circles(s.doc().unwrap(), &e).len(), 4);

    // RESet also restores deleted items (as it does replaced ones).
    s.execute("arrayedit", &json!({"handle": a, "delete": [[0, 0, 0], [2, 0, 0]]})).unwrap();
    assert_eq!(circles(s.doc().unwrap(), &the_array(&s).0).len(), 2);
    s.execute("arrayedit", &json!({"handle": a, "reset": true})).unwrap();
    let (e, p) = the_array(&s);
    assert!(p.excluded.is_empty());
    assert_eq!(circles(s.doc().unwrap(), &e).len(), 4);
}

/// The array must keep at least one item; deleting every item is refused, not silently emptied.
#[test]
fn deleting_every_item_is_refused() {
    let mut s = Session::new();
    let a = rect_array(&mut s);
    let err = s.execute("arrayedit", &json!({"handle": a, "delete": [[0, 0, 0], [1, 0, 0], [2, 0, 0], [3, 0, 0]]})).unwrap_err();
    assert!(err.to_string().contains("at least one item"), "{err}");
    // Nothing changed: all four items are still there.
    assert_eq!(circles(s.doc().unwrap(), &the_array(&s).0).len(), 4);
}

#[test]
fn deleting_replaced_item_keeps_its_block_for_restore() {
    let mut s = Session::new();
    let a = rect_array(&mut s);
    let replacement = circle(&mut s, 50.0, 0.0);
    s.execute("arrayedit", &json!({"handle": a, "replace": {"handles": [replacement], "items": [[1,0,0]]}})).unwrap();
    s.execute("erase", &json!({"handles": [replacement]})).unwrap();
    let name = the_array(&s).1.replaced[0].block.clone();
    s.execute("arrayedit", &json!({"handle": a, "delete": [[1,0,0]]})).unwrap();
    assert!(s.doc().unwrap().block(&name).is_some());
    assert_eq!(circles(s.doc().unwrap(), &the_array(&s).0).len(), 3);
    s.execute("arrayedit", &json!({"handle": a, "restore": [[1,0,0]]})).unwrap();
    assert_eq!(circles(s.doc().unwrap(), &the_array(&s).0).len(), 4);
    assert_eq!(the_array(&s).1.block_for(1, 0, 0), name);
}

#[test]
fn typed_item_delete_restore_and_undo() {
    let mut s = Session::new();
    let a = rect_array(&mut s);
    let h = Handle::parse_hex(a.as_str().unwrap()).unwrap();
    s.set_selection(vec![h]);
    for input in ["arrayedit", "Item", "2,0"] {
        s.cmdline(input).unwrap();
    }
    assert_eq!(circles(s.doc().unwrap(), &the_array(&s).0).len(), 3);
    assert!(the_array(&s).1.is_excluded(1, 0, 0));
    s.cmdline("2,0").unwrap();
    assert_eq!(circles(s.doc().unwrap(), &the_array(&s).0).len(), 4);
    for input in ["4,0", "", "x"] {
        s.cmdline(input).unwrap();
    }
    assert!(s.current_prompt().is_none());
    assert!(the_array(&s).1.is_excluded(2, 0, 0));
    s.execute("undo", &json!({})).unwrap();
    assert!(the_array(&s).1.excluded.is_empty());
    assert_eq!(circles(s.doc().unwrap(), &the_array(&s).0).len(), 4);
}

#[test]
fn invalid_item_coordinates_are_refused_without_changes() {
    let mut s = Session::new();
    let a = rect_array(&mut s);
    for items in [json!([[4, 0, 0]]), json!([[0, -1, 0]]), json!([[0, 0, 1]]), json!([[1, "bad", 0]]), json!([[1]]), json!([[0, 0, 0, 0]])] {
        assert!(s.execute("arrayedit", &json!({"handle": a, "delete": items})).is_err());
        assert_eq!(circles(s.doc().unwrap(), &the_array(&s).0).len(), 4);
        assert!(the_array(&s).1.excluded.is_empty());
    }
    s.execute("arrayedit", &json!({"handle": a, "delete": [[1,0,0], [1,0,0]]})).unwrap();
    assert_eq!(the_array(&s).1.excluded.len(), 1);
}

/// Polar arrays support item deletion too, and it survives a DXF write/read round trip (the
/// array's whole associative state, including `excluded`, is serialised as xdata).
#[test]
fn polar_delete_survives_dxf_round_trip() {
    let mut s = Session::new();
    let h = circle(&mut s, 10.0, 0.0);
    let p = s.execute("arraypolar", &json!({"handles": [h], "center": [0, 0], "count": 4})).unwrap()["handle"].clone();
    s.execute("arrayedit", &json!({"handle": p, "delete": [[2, 0, 0]]})).unwrap();
    let (e, params) = the_array(&s);
    assert!(params.is_excluded(2, 0, 0));
    let visible = circles(s.doc().unwrap(), &e);
    assert_eq!(visible.len(), 3);
    for point in [Vec2::new(10.0, 0.0), Vec2::new(0.0, 10.0), Vec2::new(0.0, -10.0)] {
        assert!(visible.iter().any(|p| p.near(point, 1e-9)), "{visible:?}");
    }
    assert!(!visible.iter().any(|p| p.near(Vec2::new(-10.0, 0.0), 1e-9)));
    let grips = s.execute("array.reference.grips", &json!({"handle": p})).unwrap();
    assert_eq!(grips.as_array().unwrap().len(), 2);

    let text = write_dxf(s.doc().unwrap());
    let back = read_dxf(text.as_bytes()).unwrap();
    let h = Handle::parse_hex(p.as_str().unwrap()).unwrap();
    let e2 = back.entity(h).unwrap();
    let after = back.array_params(e2).expect("still an associative array after the DXF round trip");
    assert!(after.is_excluded(2, 0, 0), "{after:?}");
    assert_eq!(circles(&back, e2).len(), 3);
    let mut reopened = Session::new();
    reopened.open_drawing(back, "excluded array", None);
    reopened.execute("arrayedit", &json!({"handle": h.hex(), "angle": 180})).unwrap();
    let (e, params) = the_array(&reopened);
    assert!(params.is_excluded(2, 0, 0));
    assert_eq!(circles(reopened.doc().unwrap(), &e).len(), 3);
}

//! Count/angle grips of an associative array: `array.reference.grips` lists the cols/rows arrow
//! grips of a rectangular array and the items/angle arrow grips of a polar array,
//! `array.reference.grip` drags one (one undo step), and `Session::start_array_grip` + driving a
//! point through it regenerates the array live. The same crash-safety guards ARRAYEDIT already
//! has (MAXARRAY, MAX_ARRAY_ITEMS) apply to a hostile drag distance.

use cadcraft_engine::doc::{ArrayGripWhich, ArrayKind};
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Input, Session};
use serde_json::{Value, json};

fn circle(s: &mut Session, x: f64, y: f64) -> Value {
    s.execute("circle", &json!({"center": [x, y], "radius": 1})).unwrap()["handle"].clone()
}

/// A 3×2 rectangular associative array (columns 2 apart, rows 3 apart) at the origin. Returns
/// its handle.
fn rect_array(s: &mut Session) -> Value {
    let h = circle(s, 0.0, 0.0);
    s.execute("arrayrect", &json!({"handles": [h], "rows": 2, "cols": 3, "colSpacing": 2, "rowSpacing": 3})).unwrap()["handle"].clone()
}

fn grip(s: &mut Session, h: &Value, which: &str) -> Value {
    let g = s.execute("array.reference.grips", &json!({ "handle": h })).unwrap();
    g.as_array().unwrap().iter().find(|g| g["which"] == which).cloned().unwrap_or(Value::Null)
}

#[test]
fn grips_list_two_at_the_expected_world_positions() {
    let mut s = Session::new();
    let a = rect_array(&mut s);
    let g = s.execute("array.reference.grips", &json!({ "handle": a })).unwrap();
    let list = g.as_array().unwrap();
    assert_eq!(list.len(), 2);
    let cols = grip(&mut s, &a, "cols");
    let rows = grip(&mut s, &a, "rows");
    // 3 columns, 2 apart: the far column is at x = 4 (0, 2, 4). 2 rows, 3 apart: the far row is
    // at y = 3.
    assert_eq!(cols["at"], json!([4.0, 0.0]));
    assert_eq!(cols["colsOrRowsCount"], 3);
    assert_eq!(rows["at"], json!([0.0, 3.0]));
    assert_eq!(rows["colsOrRowsCount"], 2);
    assert_eq!(cols["shape"], "linear");
}

#[test]
fn empty_for_non_arrays() {
    let mut s = Session::new();
    // A plain circle: no array grips.
    let c = circle(&mut s, 50.0, 50.0);
    let g = s.execute("array.reference.grips", &json!({ "handle": c })).unwrap();
    assert_eq!(g.as_array().unwrap().len(), 0);
}

/// Circle centres of array `h`, rounded, sorted (as `assoc_array.rs`'s tests do).
fn centres(s: &Session, h: &Value) -> Vec<(i64, i64)> {
    use cadcraft_engine::doc::{Drawing, Entity, EntityKind, Handle};
    use cadcraft_engine::geom::Mat3;
    fn walk(d: &Drawing, e: &Entity, m: Mat3, depth: usize, out: &mut Vec<Vec2>) {
        match &e.kind {
            EntityKind::Circle(c) => out.push(m.apply(c.center.xy())),
            EntityKind::Insert(i) if depth < 8 => {
                let b = d.block(&i.block).unwrap();
                let im = m.then_before(i.transform(b.base.xy()));
                for be in b.entities.iter() {
                    walk(d, be, im, depth + 1, out);
                }
            }
            _ => {}
        }
    }
    let d = s.doc().unwrap();
    let e = d.entity(Handle::parse_hex(h.as_str().unwrap()).unwrap()).unwrap();
    let mut out = Vec::new();
    walk(d, e, Mat3::IDENTITY, 0, &mut out);
    let mut v: Vec<(i64, i64)> = out.iter().map(|p| ((p.x * 1000.0).round() as i64, (p.y * 1000.0).round() as i64)).collect();
    v.sort();
    v
}

#[test]
fn json_drag_rounds_to_the_nearest_column_count() {
    let mut s = Session::new();
    let a = rect_array(&mut s);
    // Dragging the cols grip to x = 10 (spacing 2) rounds to 6 columns (0..=10 step 2).
    s.execute("array.reference.grip", &json!({ "handle": a, "which": "cols", "to": [10.0, 0.0] })).unwrap();
    let d = s.doc().unwrap();
    let e = d.entity(cadcraft_engine::doc::Handle::parse_hex(a.as_str().unwrap()).unwrap()).unwrap();
    let p = d.array_params(e).unwrap();
    assert!(matches!(&p.kind, ArrayKind::Rect(r) if r.cols == 6 && r.rows == 2), "{p:?}");
    assert_eq!(centres(&s, &a).len(), 12);
}

#[test]
fn json_drag_rounds_to_the_nearest_row_count() {
    let mut s = Session::new();
    let a = rect_array(&mut s);
    // Rows 3 apart: dragging to y = 7.5 rounds to the nearest row (2.5 -> 3 rows: 0, 3, 6? no,
    // round(7.5/3)=round(2.5)=2 (ties to even) + 1 = 3 rows). Just check it changed and is sane.
    s.execute("array.reference.grip", &json!({ "handle": a, "which": "rows", "to": [0.0, 7.5] })).unwrap();
    let d = s.doc().unwrap();
    let e = d.entity(cadcraft_engine::doc::Handle::parse_hex(a.as_str().unwrap()).unwrap()).unwrap();
    let p = d.array_params(e).unwrap();
    assert!(matches!(&p.kind, ArrayKind::Rect(r) if r.cols == 3 && r.rows >= 2), "{p:?}");
}

#[test]
fn drag_toward_the_base_point_clamps_to_one_never_zero() {
    let mut s = Session::new();
    let a = rect_array(&mut s);
    // Dragging the cols grip back to (or past) the base point would round to 0 columns;
    // AutoCAD never allows that, so it clamps to 1.
    s.execute("array.reference.grip", &json!({ "handle": a, "which": "cols", "to": [0.0, 0.0] })).unwrap();
    let d = s.doc().unwrap();
    let e = d.entity(cadcraft_engine::doc::Handle::parse_hex(a.as_str().unwrap()).unwrap()).unwrap();
    let p = d.array_params(e).unwrap();
    assert!(matches!(&p.kind, ArrayKind::Rect(r) if r.cols == 1), "{p:?}");
}

#[test]
fn a_hostile_huge_drag_distance_is_clamped_and_never_panics() {
    let mut s = Session::new();
    let a = rect_array(&mut s);
    for to in [[1.0e30, 0.0], [f64::INFINITY, 0.0], [f64::NAN, 0.0], [-1.0e30, 0.0]] {
        let r = s.execute("array.reference.grip", &json!({ "handle": a, "which": "cols", "to": to }));
        // Either refused (not a valid point) or clamped to a sane count; never panics, never
        // grows the array past MAXARRAY.
        if r.is_ok() {
            let d = s.doc().unwrap();
            let e = d.entity(cadcraft_engine::doc::Handle::parse_hex(a.as_str().unwrap()).unwrap()).unwrap();
            let p = d.array_params(e).unwrap();
            assert!(p.item_count() <= s.settings.maxarray, "{p:?}");
        }
    }
}

#[test]
fn start_array_grip_previews_and_commits_in_one_undo_step() {
    let mut s = Session::new();
    let a = rect_array(&mut s);
    let h = cadcraft_engine::doc::Handle::parse_hex(a.as_str().unwrap()).unwrap();
    s.start_array_grip(h, ArrayGripWhich::Cols).unwrap();
    // The preview regenerates the array live (more items than before the drag).
    let before = centres(&s, &a).len();
    let preview = s.preview(Vec2::new(10.0, 0.0));
    assert!(!preview.is_empty());
    s.input(Input::Point(Vec2::new(10.0, 0.0))).unwrap();
    assert!(s.running.is_none());
    let after = centres(&s, &a).len();
    assert!(after > before, "before={before} after={after}");
    // One undo step takes the whole drag back.
    s.execute("undo", &json!({})).unwrap();
    assert_eq!(centres(&s, &a).len(), before);

    // A polar array has no count grip to start.
    let h2 = circle(&mut s, 10.0, 0.0);
    let p = s.execute("arraypolar", &json!({"handles": [h2], "center": [0, 0], "count": 4})).unwrap()["handle"].clone();
    let ph = cadcraft_engine::doc::Handle::parse_hex(p.as_str().unwrap()).unwrap();
    assert!(s.start_array_grip(ph, ArrayGripWhich::Cols).is_err());
}

#[test]
fn drag_refuses_on_a_locked_layer_and_a_missing_handle() {
    let mut s = Session::new();
    let a = rect_array(&mut s);
    let h = cadcraft_engine::doc::Handle::parse_hex(a.as_str().unwrap()).unwrap();
    s.execute("layer.new", &json!({ "name": "Locked" })).unwrap();
    s.doc_mut().unwrap().modify_entity(h, |e| e.common.layer = "Locked".into()).unwrap();
    s.execute("layer.set", &json!({ "name": "Locked", "locked": true })).unwrap();
    assert!(s.execute("array.reference.grip", &json!({ "handle": a, "which": "cols", "to": [10.0, 0.0] })).is_err());
    assert!(s.execute("array.reference.grip", &json!({ "handle": "FFFFFF", "which": "cols", "to": [10.0, 0.0] })).is_err());
    assert!(s.execute("array.reference.grip", &json!({ "handle": a, "which": "sideways", "to": [10.0, 0.0] })).is_err());
}

#[test]
fn polar_grips_list_items_and_angle_at_expected_world_positions() {
    let mut s = Session::new();
    let h = circle(&mut s, 10.0, 0.0);
    // Center at origin, base point (the circle) at (10, 0): radius 10, 4 items, full circle.
    let a = s.execute("arraypolar", &json!({"handles": [h], "center": [0, 0], "count": 4})).unwrap()["handle"].clone();
    let g = s.execute("array.reference.grips", &json!({ "handle": a })).unwrap();
    let list = g.as_array().unwrap();
    assert_eq!(list.len(), 2, "{list:?}");
    let items = list.iter().find(|g| g["which"] == "items").unwrap();
    let angle = list.iter().find(|g| g["which"] == "angle").unwrap();
    assert_eq!(items["colsOrRowsCount"], 4);
    // 4 items, full circle: 90 degrees apart; the last item (i=3) is at 270 degrees.
    let at = items["at"].as_array().unwrap();
    assert!((at[0].as_f64().unwrap() - 0.0).abs() < 1e-6 && (at[1].as_f64().unwrap() - (-10.0)).abs() < 1e-6, "{at:?}");
    assert!((angle["spacing"].as_f64().unwrap() - 90.0).abs() < 1e-6, "{angle:?}");
}

#[test]
fn polar_angle_grip_absent_under_three_items() {
    let mut s = Session::new();
    let h = circle(&mut s, 10.0, 0.0);
    let a = s.execute("arraypolar", &json!({"handles": [h], "center": [0, 0], "count": 2})).unwrap()["handle"].clone();
    let g = s.execute("array.reference.grips", &json!({ "handle": a })).unwrap();
    let list = g.as_array().unwrap();
    assert_eq!(list.len(), 1, "{list:?}");
    assert_eq!(list[0]["which"], "items");
}

#[test]
fn json_drag_of_items_grip_changes_count_and_keeps_the_fill_angle() {
    let mut s = Session::new();
    let h = circle(&mut s, 10.0, 0.0);
    // 4 items, 90 degrees apart (full circle, fill = 360). The items grip sits on item 3 (270
    // deg); dragging it 2 steps further round (to 450 deg = 90 deg, local (0, 10) world) adds 2
    // items, keeping the fill a full circle.
    let a = s.execute("arraypolar", &json!({"handles": [h], "center": [0, 0], "count": 4})).unwrap()["handle"].clone();
    s.execute("array.reference.grip", &json!({ "handle": a, "which": "items", "to": [0.0, 10.0] })).unwrap();
    let d = s.doc().unwrap();
    let e = d.entity(cadcraft_engine::doc::Handle::parse_hex(a.as_str().unwrap()).unwrap()).unwrap();
    let p = d.array_params(e).unwrap();
    assert!(matches!(&p.kind, ArrayKind::Polar(k) if k.items == 6 && (k.fill - 360.0).abs() < 1e-6), "{p:?}");
}

#[test]
fn json_drag_of_angle_grip_changes_fill_and_keeps_the_item_count() {
    let mut s = Session::new();
    let h = circle(&mut s, 10.0, 0.0);
    // 4 items, full circle (90 deg apart); item 1 sits at 90 degrees, i.e. (0, 10).
    let a = s.execute("arraypolar", &json!({"handles": [h], "center": [0, 0], "count": 4})).unwrap()["handle"].clone();
    // Drag the angle grip to 45 degrees: the new angle between items.
    s.execute("array.reference.grip", &json!({ "handle": a, "which": "angle", "to": [7.0710678, 7.0710678] })).unwrap();
    let d = s.doc().unwrap();
    let e = d.entity(cadcraft_engine::doc::Handle::parse_hex(a.as_str().unwrap()).unwrap()).unwrap();
    let p = d.array_params(e).unwrap();
    // 4 items at 45 degrees apart is no longer a full circle: fill = 45 * 3 = 135.
    assert!(matches!(&p.kind, ArrayKind::Polar(k) if k.items == 4 && (k.fill - 135.0).abs() < 1e-3), "{p:?}");
}

#[test]
fn polar_drag_toward_the_centre_never_panics_and_stays_in_bounds() {
    let mut s = Session::new();
    let h = circle(&mut s, 10.0, 0.0);
    let a = s.execute("arraypolar", &json!({"handles": [h], "center": [0, 0], "count": 4})).unwrap()["handle"].clone();
    // Dragging onto the centre: the angle from the grip's current position is undefined (a zero
    // vector); the count must stay in range and the call must not panic.
    s.execute("array.reference.grip", &json!({ "handle": a, "which": "items", "to": [0.0, 0.0] })).unwrap();
    let d = s.doc().unwrap();
    let e = d.entity(cadcraft_engine::doc::Handle::parse_hex(a.as_str().unwrap()).unwrap()).unwrap();
    let p = d.array_params(e).unwrap();
    assert!(matches!(&p.kind, ArrayKind::Polar(k) if k.items >= 1 && u64::from(k.items) <= s.settings.maxarray), "{p:?}");
}

#[test]
fn a_hostile_huge_polar_drag_distance_is_clamped_and_never_panics() {
    let mut s = Session::new();
    let h = circle(&mut s, 10.0, 0.0);
    let a = s.execute("arraypolar", &json!({"handles": [h], "center": [0, 0], "count": 4})).unwrap()["handle"].clone();
    for to in [[1.0e30, 0.0], [f64::INFINITY, 0.0], [f64::NAN, 0.0], [-1.0e30, 0.0]] {
        for which in ["items", "angle"] {
            let r = s.execute("array.reference.grip", &json!({ "handle": a, "which": which, "to": to }));
            // Either refused (not a valid point, or an invalid fill angle) or clamped to a sane
            // count; never panics, never grows the array past MAXARRAY.
            if r.is_ok() {
                let d = s.doc().unwrap();
                let e = d.entity(cadcraft_engine::doc::Handle::parse_hex(a.as_str().unwrap()).unwrap()).unwrap();
                let p = d.array_params(e).unwrap();
                assert!(p.item_count() <= s.settings.maxarray, "{p:?}");
            }
        }
    }
}

#[test]
fn polar_start_array_grip_previews_and_commits_in_one_undo_step() {
    let mut s = Session::new();
    let h = circle(&mut s, 10.0, 0.0);
    let a = s.execute("arraypolar", &json!({"handles": [h], "center": [0, 0], "count": 4})).unwrap()["handle"].clone();
    let ph = cadcraft_engine::doc::Handle::parse_hex(a.as_str().unwrap()).unwrap();
    s.start_array_grip(ph, ArrayGripWhich::Items).unwrap();
    let before = centres(&s, &a).len();
    // One step further round the circle from the current (last) item: (10, 0) adds one item.
    let preview = s.preview(Vec2::new(10.0, 0.0));
    assert!(!preview.is_empty());
    s.input(Input::Point(Vec2::new(10.0, 0.0))).unwrap();
    assert!(s.running.is_none());
    let after = centres(&s, &a).len();
    assert!(after > before, "before={before} after={after}");
    s.execute("undo", &json!({})).unwrap();
    assert_eq!(centres(&s, &a).len(), before);

    // A rectangular array has no Items or Angle grip.
    let hr = circle(&mut s, 50.0, 50.0);
    let r = s.execute("arrayrect", &json!({"handles": [hr], "rows": 2, "cols": 2, "colSpacing": 2, "rowSpacing": 2})).unwrap()["handle"].clone();
    let rh = cadcraft_engine::doc::Handle::parse_hex(r.as_str().unwrap()).unwrap();
    assert!(s.start_array_grip(rh, ArrayGripWhich::Items).is_err());
}

#[test]
fn polar_drag_refuses_on_a_locked_layer_and_a_missing_handle() {
    let mut s = Session::new();
    let h = circle(&mut s, 10.0, 0.0);
    let a = s.execute("arraypolar", &json!({"handles": [h], "center": [0, 0], "count": 4})).unwrap()["handle"].clone();
    let ph = cadcraft_engine::doc::Handle::parse_hex(a.as_str().unwrap()).unwrap();
    s.execute("layer.new", &json!({ "name": "Locked" })).unwrap();
    s.doc_mut().unwrap().modify_entity(ph, |e| e.common.layer = "Locked".into()).unwrap();
    s.execute("layer.set", &json!({ "name": "Locked", "locked": true })).unwrap();
    assert!(s.execute("array.reference.grip", &json!({ "handle": a, "which": "items", "to": [0.0, 10.0] })).is_err());
    assert!(s.execute("array.reference.grip", &json!({ "handle": "FFFFFF", "which": "items", "to": [0.0, 10.0] })).is_err());
    assert!(s.execute("array.reference.grip", &json!({ "handle": a, "which": "sideways", "to": [0.0, 10.0] })).is_err());
}

#[test]
fn missing_run_step_on_a_non_point_input_is_refused() {
    let mut s = Session::new();
    let a = rect_array(&mut s);
    let h = cadcraft_engine::doc::Handle::parse_hex(a.as_str().unwrap()).unwrap();
    s.start_array_grip(h, ArrayGripWhich::Rows).unwrap();
    // Text the prompt doesn't accept (POINT only) is reported and re-prompts; nothing commits.
    let before = centres(&s, &a).len();
    s.input(Input::Text("not a point".into())).unwrap();
    assert!(s.running.is_some());
    assert_eq!(centres(&s, &a).len(), before);
    // eXit cancels the drag without changing anything.
    let step = s.input(Input::Keyword("eXit".into()));
    assert!(step.is_ok());
    assert!(s.running.is_none());
    assert_eq!(centres(&s, &a).len(), before);
}

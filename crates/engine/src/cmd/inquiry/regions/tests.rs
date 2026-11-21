use super::*;

fn h(v: &Value) -> Handle {
    Handle::parse_hex(v["handle"].as_str().unwrap()).unwrap()
}

fn region(s: &Session, hd: Handle) -> Region {
    match &s.doc().unwrap().entity(hd).unwrap().kind {
        EntityKind::Region(r) => r.clone(),
        k => panic!("not a region: {k:?}"),
    }
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6 * b.abs().max(1.0)
}

/// A region from a rectangle, and one from a circle.
fn rect_region(s: &mut Session, a: [f64; 2], b: [f64; 2]) -> Handle {
    let r = h(&s.execute("rectang", &json!({"p1": a, "p2": b})).unwrap());
    s.execute("region", &json!({"handles": [r.hex()]})).unwrap();
    r
}

/// #F21: UNION, SUBTRACT and INTERSECT of regions whose boundaries touch along one whole
/// shared edge (adjacent squares) without crossing into each other's interior.
#[test]
fn booleans_on_regions_sharing_one_edge() {
    // UNION of two abutting unit squares merges into one rectangle: the shared wall at x=1
    // disappears, leaving one outer loop of double the area.
    let mut s = Session::new();
    let a = rect_region(&mut s, [0.0, 0.0], [1.0, 1.0]);
    let b = rect_region(&mut s, [1.0, 0.0], [2.0, 1.0]);
    s.execute("union", &json!({"handles": [a.hex(), b.hex()]})).unwrap();
    let r = region(&s, a);
    assert_eq!(r.loops.len(), 1, "{r:?}");
    assert!(near(r.area(), 2.0), "{}", r.area());
    assert!(near(r.perimeter(), 6.0), "the internal shared wall must disappear");
    assert!(r.contains(Vec2::new(0.5, 0.5)) && r.contains(Vec2::new(1.5, 0.5)), "both halves are inside the merged outline");
    assert!(s.doc().unwrap().entity(b).is_none(), "the second square is absorbed");

    // SUBTRACT of a region that only touches along an edge removes nothing: the boundary and
    // area are unchanged, and the subtracted region is still erased.
    let mut s = Session::new();
    let a = rect_region(&mut s, [0.0, 0.0], [2.0, 2.0]);
    let b = rect_region(&mut s, [2.0, 0.0], [3.0, 2.0]);
    s.execute("subtract", &json!({"from": [a.hex()], "remove": [b.hex()]})).unwrap();
    let r = region(&s, a);
    assert_eq!(r.loops.len(), 1, "{r:?}");
    assert!(near(r.area(), 4.0), "{}", r.area());
    assert!(r.contains(Vec2::new(1.0, 1.0)));
    assert!(s.doc().unwrap().entity(b).is_none());

    // INTERSECT of two regions that only touch along an edge shares no area: nothing to keep.
    let mut s = Session::new();
    let a = rect_region(&mut s, [0.0, 0.0], [1.0, 1.0]);
    let b = rect_region(&mut s, [1.0, 0.0], [2.0, 1.0]);
    let result = s.execute("intersect", &json!({"handles": [a.hex(), b.hex()]})).unwrap();
    assert!(result["handle"].is_null(), "{result}");
    assert_eq!(s.doc().unwrap().model.len(), 0);
}

#[test]
fn coincident_regions_are_not_duplicated() {
    for command in ["union", "intersect", "subtract"] {
        let mut s = Session::new();
        let a = rect_region(&mut s, [0.0, 0.0], [2.0, 2.0]);
        let b = rect_region(&mut s, [0.0, 0.0], [2.0, 2.0]);
        let params = if command == "subtract" { json!({"from": [a.hex()], "remove": [b.hex()]}) } else { json!({"handles": [a.hex(), b.hex()]}) };
        let result = s.execute(command, &params).unwrap();
        if command == "subtract" {
            assert!(result["handle"].is_null());
            assert_eq!(s.doc().unwrap().model.len(), 0);
        } else {
            let r = region(&s, a);
            assert_eq!(r.loops.len(), 1);
            assert!(near(r.area(), 4.0));
            assert!(near(r.perimeter(), 8.0));
        }
    }
}

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

/// #F21 narrowed: UNION, SUBTRACT and INTERSECT of two squares whose boundaries properly cross
/// each other's interior (classic overlapping-rectangles case), handled by splitting each
/// boundary at the two crossing points before tracing the result's outline.
#[test]
fn booleans_on_properly_overlapping_squares() {
    // A = (0,0)-(2,2), B = (1,1)-(3,3): A's right/top edges cross B's bottom/left edges at
    // (2,1) and (1,2); the overlap is the unit square (1,1)-(2,2).
    let mut s = Session::new();
    let a = rect_region(&mut s, [0.0, 0.0], [2.0, 2.0]);
    let b = rect_region(&mut s, [1.0, 1.0], [3.0, 3.0]);
    s.execute("union", &json!({"handles": [a.hex(), b.hex()]})).unwrap();
    let r = region(&s, a);
    assert_eq!(r.loops.len(), 1, "{r:?}");
    assert!(near(r.area(), 7.0), "{}", r.area()); // 4 + 4 - 1 overlap
    assert!(r.contains(Vec2::new(0.5, 0.5)) && r.contains(Vec2::new(2.5, 2.5)) && r.contains(Vec2::new(1.5, 1.5)));
    assert!(!r.contains(Vec2::new(2.5, 0.5)), "outside both squares");
    assert!(s.doc().unwrap().entity(b).is_none());

    let mut s = Session::new();
    let a = rect_region(&mut s, [0.0, 0.0], [2.0, 2.0]);
    let b = rect_region(&mut s, [1.0, 1.0], [3.0, 3.0]);
    s.execute("intersect", &json!({"handles": [a.hex(), b.hex()]})).unwrap();
    let r = region(&s, a);
    assert_eq!(r.loops.len(), 1, "{r:?}");
    assert!(near(r.area(), 1.0), "{}", r.area());
    assert!(r.contains(Vec2::new(1.5, 1.5)));
    assert!(!r.contains(Vec2::new(0.5, 0.5)));

    let mut s = Session::new();
    let a = rect_region(&mut s, [0.0, 0.0], [2.0, 2.0]);
    let b = rect_region(&mut s, [1.0, 1.0], [3.0, 3.0]);
    s.execute("subtract", &json!({"from": [a.hex()], "remove": [b.hex()]})).unwrap();
    let r = region(&s, a);
    assert_eq!(r.loops.len(), 1, "{r:?}");
    assert!(near(r.area(), 3.0), "{}", r.area());
    assert!(r.contains(Vec2::new(0.5, 0.5)));
    assert!(!r.contains(Vec2::new(1.5, 1.5)), "the overlap with B is removed");
}

/// A collinear partial overlap (two edges run along the same line for part of their length
/// without matching endpoint-for-endpoint) is still not available: the operands are left alone
/// and the caller gets the clean error, never a panic or a silently wrong result.
#[test]
fn collinear_partial_overlap_is_still_refused() {
    // A = (0,0)-(2,1); B = (1,0)-(3,1): their bottom and top edges run along the same two
    // lines (y=0, y=1) but overlap only over x in [1,2], not endpoint-for-endpoint.
    let mut s = Session::new();
    let a = rect_region(&mut s, [0.0, 0.0], [2.0, 1.0]);
    let b = rect_region(&mut s, [1.0, 0.0], [3.0, 1.0]);
    let err = s.execute("union", &json!({"handles": [a.hex(), b.hex()]})).unwrap_err();
    assert!(err.to_string().contains("not available yet"), "{err}");
    // Nothing changed: both regions are still exactly as they were.
    assert!(s.doc().unwrap().entity(a).is_some());
    assert!(s.doc().unwrap().entity(b).is_some());
}

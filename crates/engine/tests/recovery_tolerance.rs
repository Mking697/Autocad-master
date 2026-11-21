use cadcraft_doc::{EntityKind, Handle};
use cadcraft_geom::Vec2;
use serde_json::{Value, json};

use cadcraft_engine::Session;

fn h(v: &Value) -> Vec<Handle> {
    v["handles"].as_array().unwrap().iter().map(|x| Handle::parse_hex(x.as_str().unwrap()).unwrap()).collect()
}

fn near(a: Vec2, b: Vec2) -> bool {
    a.dist(b) < 1e-9
}

/// LEADER's own existing content (plain text) is unaffected by the new tolerance branch.
#[test]
fn leader_with_plain_text_is_unchanged() {
    let mut s = Session::new();
    let hs = h(&s.execute("leader", &json!({ "points": [[0, 0], [5, 0]], "text": "note" })).unwrap());
    assert_eq!(hs.len(), 2);
    assert!(matches!(&s.doc().unwrap().entity(hs[0]).unwrap().kind, EntityKind::Leader(_)));
    assert!(matches!(&s.doc().unwrap().entity(hs[1]).unwrap().kind, EntityKind::MText(_)));
}

/// LEADER ending in a feature control frame instead of text: the leader geometry is
/// untouched and the frame lands exactly at the leader's last (landing) point.
#[test]
fn leader_ending_in_tolerance_frame() {
    let mut s = Session::new();
    let hs = h(&s
        .execute("leader", &json!({ "points": [[0, 0], [3, 0], [5, 0]], "tolerance": { "symbol": "position", "tol1": "0.5", "datums": ["A"] } }))
        .unwrap());
    assert_eq!(hs.len(), 2, "leader line + frame entity");
    let d = s.doc().unwrap();
    match &d.entity(hs[0]).unwrap().kind {
        EntityKind::Leader(l) => {
            assert_eq!(l.vertices.len(), 3);
            assert!(near(l.vertices.last().unwrap().xy(), Vec2::new(5.0, 0.0)));
        }
        other => panic!("not a leader: {other:?}"),
    }
    match &d.entity(hs[1]).unwrap().kind {
        EntityKind::Tolerance(t) => {
            assert!(near(t.insert.xy(), Vec2::new(5.0, 0.0)), "frame sits at the landing point: {:?}", t.insert);
            assert!(t.text.contains('A'), "datum reference in the frame text: {}", t.text);
        }
        other => panic!("not a tolerance frame: {other:?}"),
    }
}

/// An empty tolerance frame on a leader is rejected the same way TOLERANCE itself rejects it.
#[test]
fn leader_tolerance_rejects_empty_frame() {
    let mut s = Session::new();
    let err = s.execute("leader", &json!({ "points": [[0, 0], [1, 0]], "tolerance": {} })).unwrap_err();
    assert!(err.to_string().contains("empty"), "{err}");
}

#[test]
fn leader_tolerance_validation_is_atomic() {
    let mut s = Session::new();
    for extra in [
        json!({"tolerance": {}}),
        json!({"tolerance": {"symbol": "not-a-symbol"}}),
        json!({"tolerance": {"tol1": "0.5", "style": "missing"}}),
        json!({"tolerance": "not an object"}),
        json!({"tolerance": {"tol1": "0.5"}, "text": "conflicting note"}),
    ] {
        let mut p = extra;
        p["points"] = json!([[0, 0], [1, 0]]);
        assert!(s.execute("leader", &p).is_err(), "{p}");
        assert_eq!(s.doc().unwrap().model.len(), 0, "invalid content must leave no orphan leader");
    }
}

#[test]
fn leader_tolerance_rotation_and_undo_redo() {
    let mut s = Session::new();
    let hs = h(&s
        .execute(
            "leader",
            &json!({
                "points": [[0, 0], [1, 2]], "tolerance": {"tol1": "0.1", "rotation": 90}
            }),
        )
        .unwrap());
    let EntityKind::Tolerance(t) = &s.doc().unwrap().entity(hs[1]).unwrap().kind else { panic!("missing frame") };
    assert!(near(t.dir.xy(), Vec2::new(0.0, 1.0)));
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap().model.len(), 0);
    s.redo().unwrap();
    assert_eq!(s.doc().unwrap().model.len(), 2);
    assert!(s.doc().unwrap().entity(hs[1]).is_some());
}

/// Both JSON and interactive QLEADER use LEADER's shared tolerance-content implementation.
#[test]
fn qleader_has_json_and_interactive_tolerance_content() {
    let mut s = Session::new();
    let hs = h(&s.execute("qleader", &json!({"points": [[2, 0], [3, 0]], "tolerance": {"symbol": "flatness", "tol1": "0.1"}})).unwrap());
    assert_eq!(hs.len(), 2);
    s.cmdline("qleader").unwrap();
    assert_eq!(s.running.as_ref().map(|r| r.id.as_str()), Some("qleader"));
    s.cmdline("0,0").unwrap();
    s.cmdline("1,0").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("tolerance").unwrap();
    s.cmdline("flatness").unwrap();
    s.cmdline("0.1").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("").unwrap();
    assert!(s.running.is_none());
    assert!(s.doc().unwrap().model.iter().any(|e| matches!(e.kind, EntityKind::Tolerance(_))));
}

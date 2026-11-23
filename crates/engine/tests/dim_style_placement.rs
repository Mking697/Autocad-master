//! DIMUPT governs creation, never retroactively discards DIMTEDIT positions.
use cadcraft_engine::{
    Session,
    doc::{Dimension, EntityKind},
    render::DimGeometry,
};
use serde_json::json;

fn last_dim(s: &Session) -> (Dimension, DimGeometry) {
    let doc = s.doc().unwrap();
    let dm = doc
        .model
        .iter()
        .filter_map(|e| match &e.kind {
            EntityKind::Dimension(dm) => Some(dm.clone()),
            _ => None,
        })
        .last()
        .unwrap();
    let geometry = cadcraft_engine::render::dimension_in(doc, &dm);
    (dm, geometry)
}

#[test]
fn dimupt_cursor_controls_creation_text_not_later_edits() {
    let mut s = Session::new();
    s.execute("setvar", &json!({"name":"DIMUPT", "value":1})).unwrap();
    s.execute("dimlinear", &json!({"p1":[0,0], "p2":[10,0], "at":[2,3]})).unwrap();
    let (dm, g) = last_dim(&s);
    assert!(dm.user_text_pos);
    assert!((g.text_pos.x - 2.0).abs() < 1e-9);
    assert!((g.text_pos.y - 3.0).abs() < 1e-9);
    let manual_text_geometry = g.text.clone();
    assert!(!manual_text_geometry.is_empty());
    s.execute("setvar", &json!({"name":"DIMUPT", "value":0})).unwrap();
    s.execute("dimlinear", &json!({"p1":[0,0], "p2":[10,0], "at":[2,3]})).unwrap();
    let (dm, g) = last_dim(&s);
    assert!(!dm.user_text_pos);
    assert!((g.text_pos.x - 5.0).abs() < 1e-9);
    assert_ne!(g.text, manual_text_geometry, "DIMUPT changes the drawn text strokes, not only metadata");
    let h = s.doc().unwrap().model.last().unwrap().handle.hex();
    s.execute("dimtedit", &json!({"handles":[h], "at":[1,4]})).unwrap();
    let (dm, g) = last_dim(&s);
    assert!(dm.user_text_pos);
    assert!((g.text_pos.x - 1.0).abs() < 1e-9);
    assert!((g.text_pos.y - 4.0).abs() < 1e-9);
}

#[test]
fn dimupt_is_a_style_variable_and_dimension_override_in_dxf() {
    let mut s = Session::new();
    s.execute("dimstyle", &json!({"name":"Manual", "DIMUPT":1})).unwrap();
    let listed = s.execute("dimstyle.list", &json!({"name":"Manual"})).unwrap();
    assert_eq!(listed["variables"]["DIMUPT"], true);
    s.execute("dimaligned", &json!({"p1":[0,0], "p2":[10,0], "at":[2,3]})).unwrap();
    let (dm, g) = last_dim(&s);
    assert!(dm.user_text_pos);
    assert!((g.text_pos.x - 2.0).abs() < 1e-9);
    let h = s.doc().unwrap().model.last().unwrap().handle.hex();
    s.execute("dimoverride", &json!({"handles":[h], "DIMUPT":0})).unwrap();
    let (dm, g) = last_dim(&s);
    // A creation variable override is stored but cannot erase an existing manual placement.
    assert!(dm.overrides.contains_key("userPositionedText"));
    assert!(!cadcraft_engine::doc::DimStyle::default().with_overrides(&dm.overrides).user_positioned_text);
    assert!((g.text_pos.x - 2.0).abs() < 1e-9);
    let text = cadcraft_io::write_dxf(s.doc().unwrap());
    let back = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
    assert!(back.dim_style("Manual").unwrap().user_positioned_text);
    let dm = back
        .model
        .iter()
        .find_map(|e| match &e.kind {
            EntityKind::Dimension(dm) => Some(dm),
            _ => None,
        })
        .unwrap();
    assert!(dm.overrides.contains_key("userPositionedText"));
    assert!(!cadcraft_engine::doc::DimStyle::default().with_overrides(&dm.overrides).user_positioned_text);
    assert!((cadcraft_engine::render::dimension_in(&back, dm).text_pos.x - 2.0).abs() < 1e-9);
}

#[test]
fn dimupt_is_used_by_interactive_preview_and_angular_creation() {
    let mut s = Session::new();
    s.execute("setvar", &json!({"name":"DIMUPT", "value":1})).unwrap();
    s.cmdline("dimlinear 0,0 10,0").unwrap();
    let preview = s.preview(cadcraft_engine::geom::Vec2::new(2.0, 3.0));
    let dm = preview
        .iter()
        .find_map(|entity| match &entity.kind {
            EntityKind::Dimension(dm) => Some(dm),
            _ => None,
        })
        .unwrap();
    assert!(dm.user_text_pos);
    assert_eq!(dm.text_mid.xy(), cadcraft_engine::geom::Vec2::new(2.0, 3.0));
    s.cmdline("2,3").unwrap();
    assert!((last_dim(&s).1.text_pos.x - 2.0).abs() < 1e-9);
    s.execute("dimangular", &json!({"vertex":[0,0], "p1":[10,0], "p2":[0,10], "at":[4,2]})).unwrap();
    let (dm, g) = last_dim(&s);
    assert!(dm.user_text_pos);
    assert_eq!(g.text_pos, cadcraft_engine::geom::Vec2::new(4.0, 2.0));
}

#[test]
fn dimupt_rejects_hostile_values_without_changing_the_drawing() {
    let mut s = Session::new();
    for value in [json!(-1), json!(2), json!(1e300), json!("garbage"), json!("NaN"), json!(null), json!([]), json!({})] {
        let before = s.execute("dimstyle.list", &json!({"name":"Standard"})).unwrap();
        let result = s.execute("setvar", &json!({"name":"DIMUPT", "value":value}));
        assert!(matches!(result, Err(cadcraft_engine::EngineError::BadParams { .. })), "{value}: {result:?}");
        assert_eq!(s.execute("dimstyle.list", &json!({"name":"Standard"})).unwrap(), before);
        assert!(s.doc().unwrap().dim_overrides().is_empty());
    }
}

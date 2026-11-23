//! A multileader's spline leader and block content (#408) survive save and reopen: they are
//! written as a native MULTILEADER (#459).

use cadcraft_doc::{Block, Common, Drawing, Entity, EntityKind, Handle, Insert, Line, MLeader, Space};
use cadcraft_geom::Vec3;

#[test]
fn block_style_defaults_round_trip_in_native_dxf_groups() {
    let mut d = Drawing::new_metric();
    let mut blk = Block::new("TAG");
    blk.entities.push(Entity::new(Handle(0x100), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(2.0, 2.0, 0.0) })));
    d.blocks.insert(blk.name.clone(), std::sync::Arc::new(blk));
    let style: cadcraft_doc::MLeaderStyle = serde_json::from_value(serde_json::json!({
        "name": "Symbols", "block": "TAG", "blockScale": 2.5, "blockCenter": false,
        "blockAttribs": {"NUM": "Ω \\U+0041 \\P"}
    }))
    .unwrap();
    d.mleader_styles.push(style);
    let bytes = cadcraft_io::write(&d, "a.dxf").unwrap();
    let back = cadcraft_io::read(&bytes, "a.dxf").unwrap();
    let got = back.mleader_styles.iter().find(|s| s.name == "Symbols").unwrap();
    let fields = serde_json::to_value(got).unwrap();
    assert_eq!(fields["block"], "TAG");
    assert_eq!(fields["blockScale"], 2.5);
    assert_eq!(fields["blockCenter"], false);
    assert_eq!(fields["blockAttribs"]["NUM"], "Ω \\U+0041 \\P");
    // Inspect the standard MLEADERSTYLE object rather than trusting our own reader alone.
    let tags = cadcraft_dxf::parse(&bytes).unwrap();
    assert!(tags.iter().any(|t| t.code == 343 && t.str() != "0"));
    assert!(tags.iter().any(|t| t.code == 47 && t.f64() == 2.5));
    // The native DWG bridge must retain the new style's block pointer and scale too.
    let dwg = cadcraft_io::write(&d, "style.dwg").unwrap();
    let reopened = cadcraft_io::read(&dwg, "style.dwg").unwrap();
    assert_eq!(reopened.mleader_styles.iter().find(|s| s.name == "Symbols"), d.mleader_styles.iter().find(|s| s.name == "Symbols"));
}

#[test]
fn spline_leader_and_block_content_are_written() {
    let mut d = Drawing::new_metric();
    let mut blk = Block::new("TAG");
    blk.entities.push(Entity::new(Handle(0x100), EntityKind::Line(Line { a: Vec3::new(0.0, 0.0, 0.0), b: Vec3::new(1.0, 1.0, 0.0) })));
    d.blocks.insert(blk.name.clone(), std::sync::Arc::new(blk));
    let m = MLeader {
        leaders: vec![vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(5.0, 5.0, 0.0)]],
        landing: Vec3::new(10.0, 0.0, 0.0),
        dogleg: 1.0,
        text: None,
        style: "Standard".into(),
        arrow_size: 1.0,
        spline: true,
        block: Some(Insert {
            block: "TAG".into(),
            insert: Vec3::new(11.0, 0.0, 0.0),
            scale: Vec3::new(1.0, 1.0, 1.0),
            rotation: 0.0,
            attribs: Vec::new(),
            cols: 1,
            rows: 1,
            col_spacing: 0.0,
            row_spacing: 0.0,
            clip: None,
        }),
    };
    d.add(&Space::Model, Common::default(), EntityKind::MLeader(m)).unwrap();
    let bytes = cadcraft_io::write(&d, "a.dxf").unwrap();
    let back = cadcraft_io::read(&bytes, "a.dxf").unwrap();
    assert!(back.model.iter().any(|e| matches!(&e.kind, EntityKind::MLeader(m) if m.spline
        && m.leaders.first().is_some_and(|l| l.len() == 2)
        && m.block.as_ref().is_some_and(|i| i.block == "TAG" && i.insert == Vec3::new(11.0, 0.0, 0.0)))));
}

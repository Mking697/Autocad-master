//! Synthetic XCLIP command regressions; no reference application files.
use cadcraft_doc::{EntityKind, Handle};
use cadcraft_engine::Session;
use serde_json::{Value, json};

fn reference(s: &mut Session) -> Handle {
    s.cmdline("line 0,0 10,0").unwrap();
    s.cmdline("").unwrap();
    let hs = s.doc().unwrap().model.iter().map(|e| e.handle.hex()).collect::<Vec<_>>();
    s.execute("block", &json!({"name":"CLIP_TEST", "base":[0,0], "handles":hs, "keep":"convert"})).unwrap();
    s.doc().unwrap().model.iter().find(|e| matches!(e.kind, EntityKind::Insert(_))).unwrap().handle
}
fn clip(s: &Session, h: Handle) -> Value {
    let EntityKind::Insert(i) = &s.doc().unwrap().entity(h).unwrap().kind else { panic!("not insert") };
    serde_json::to_value(i).unwrap()
}
#[test]
fn xclip_rectangle_toggle_delete_undo() {
    let mut s = Session::new();
    let h = reference(&mut s);
    s.execute("xclip", &json!({"handle":h.hex(), "p1":[2,-1], "p2":[8,1]})).unwrap();
    assert!(clip(&s, h)["clip"].is_object());
    s.execute("xclip", &json!({"handle":h.hex(), "option":"off"})).unwrap();
    assert_eq!(clip(&s, h)["clip"]["on"], false);
    s.execute("xclip", &json!({"handle":h.hex(), "option":"on"})).unwrap();
    assert_eq!(clip(&s, h)["clip"]["on"], true);
    s.execute("xclip", &json!({"handle":h.hex(), "option":"delete"})).unwrap();
    assert!(clip(&s, h)["clip"].is_null());
    s.execute("undo", &json!({})).unwrap();
    assert!(clip(&s, h)["clip"].is_object());
    s.execute("redo", &json!({})).unwrap();
    assert!(clip(&s, h)["clip"].is_null());
}

fn stored(s: &Session, h: Handle) -> cadcraft_doc::InsertClip {
    let EntityKind::Insert(i) = &s.doc().unwrap().entity(h).unwrap().kind else { panic!("not insert") };
    i.clip.clone().unwrap()
}

#[test]
fn xclip_polygon_reuses_viewport_region_and_inverts_independently() {
    let mut s = Session::new();
    let h = reference(&mut s);
    let other = s.execute("insert", &json!({"name":"CLIP_TEST","at":[20,0]})).unwrap();
    let other = Handle::parse_hex(other["handle"].as_str().unwrap()).unwrap();
    s.execute("xclip", &json!({"handle":h.hex(),"points":[[0,0],[10,0],[10,5],[5,5],[5,10],[0,10]],"invert":true})).unwrap();
    let c = stored(&s, h);
    let points = cadcraft_render::clip::boundary_polygon(&EntityKind::LwPolyline(c.boundary)).unwrap();
    let region = cadcraft_render::clip::PolyClip::new(&points).unwrap();
    assert!(region.contains(cadcraft_geom::Vec2::new(2.0, 8.0)));
    assert!(!region.contains(cadcraft_geom::Vec2::new(8.0, 8.0)));
    assert!(c.inverted);
    assert!(clip(&s, other)["clip"].is_null());
    s.execute("xclip", &json!({"handle":h.hex(),"option":"invert"})).unwrap();
    assert!(!stored(&s, h).inverted);
    let before = clip(&s, h);
    let out = s.execute("xclip", &json!({"handle":h.hex(),"option":"?"})).unwrap();
    assert_eq!(out["clip"], before["clip"]);
    let encoded = serde_json::to_string(s.doc().unwrap().entity(h).unwrap()).unwrap();
    let decoded: cadcraft_doc::Entity = serde_json::from_str(&encoded).unwrap();
    assert_eq!(s.doc().unwrap().entity(h).unwrap().as_ref(), &decoded);
}

#[test]
fn xclip_generate_tracks_reference_transform_and_current_properties() {
    let mut s = Session::new();
    let h = reference(&mut s);
    s.execute("xclip", &json!({"handle":h.hex(),"p1":[2,-1],"p2":[8,1]})).unwrap();
    s.execute("move", &json!({"handle":h.hex(),"delta":[10,20]})).unwrap();
    s.execute("scale", &json!({"handle":h.hex(),"base":[10,20],"factor":2})).unwrap();
    s.execute("rotate", &json!({"handle":h.hex(),"base":[10,20],"angle":90})).unwrap();
    let common = s.current_common();
    let out = s.execute("xclip", &json!({"handle":h.hex(),"option":"polyline"})).unwrap();
    let polyline = Handle::parse_hex(out["polyline"].as_str().unwrap()).unwrap();
    let e = s.doc().unwrap().entity(polyline).unwrap();
    assert_eq!(e.common, common);
    let EntityKind::LwPolyline(pl) = &e.kind else { panic!("not polyline") };
    assert!(pl.closed);
    let b = cadcraft_geom::Bounds2::from_points(pl.vertices.iter().map(|v| v.p));
    assert!(b.min.near(cadcraft_geom::Vec2::new(8.0, 24.0), 1e-8), "{b:?}");
    assert!(b.max.near(cadcraft_geom::Vec2::new(12.0, 36.0), 1e-8), "{b:?}");
    s.execute("xclip", &json!({"handle":h.hex(),"object":polyline.hex()})).unwrap();
    assert!(s.doc().unwrap().entity(polyline).is_some(), "source polyline is not consumed");
    assert_eq!(stored(&s, h).boundary.vertices.len(), 4);
}

#[test]
fn xclip_typed_rectangle_polygon_depth_and_cancel() {
    let mut s = Session::new();
    let h = reference(&mut s);
    s.set_selection(vec![h]);
    s.cmdline("xclip n r 2,-1 8,1").unwrap();
    assert!(stored(&s, h).on, "{:?}", s.log);
    s.set_selection(vec![h]);
    s.cmdline("xclip off").unwrap();
    assert!(!stored(&s, h).on);
    s.set_selection(vec![h]);
    s.cmdline("xclip n i").unwrap();
    assert!(stored(&s, h).inverted);
    s.set_selection(vec![h]);
    s.cmdline("xclip c 3 2").unwrap();
    assert_eq!((stored(&s, h).front, stored(&s, h).back), (Some(3.0), Some(2.0)));
    s.set_selection(vec![h]);
    s.cmdline("xclip c r").unwrap();
    assert_eq!((stored(&s, h).front, stored(&s, h).back), (None, None));
    let before = clip(&s, h);
    s.set_selection(vec![h]);
    s.cmdline("xclip n r 0,0").unwrap();
    s.input(cadcraft_engine::Input::Cancel).unwrap();
    assert_eq!(before, clip(&s, h));
    s.set_selection(vec![h]);
    s.cmdline("xclip n p 0,0 10,0 0,10").unwrap();
    s.cmdline("").unwrap();
    assert_eq!(stored(&s, h).boundary.vertices.len(), 3);
}

#[test]
fn xclip_open_polyline_is_copied_and_curved_polyline_is_rejected() {
    let mut s = Session::new();
    let h = reference(&mut s);
    s.execute("xclip", &json!({"handle":h.hex(),"p1":[2,-1],"p2":[8,1]})).unwrap();
    let out = s.execute("xclip", &json!({"handle":h.hex(),"option":"polyline"})).unwrap();
    let ph = Handle::parse_hex(out["polyline"].as_str().unwrap()).unwrap();
    s.doc_mut()
        .unwrap()
        .modify_entity(ph, |e| {
            if let EntityKind::LwPolyline(p) = &mut e.kind {
                p.closed = false;
            }
        })
        .unwrap();
    let source = s.doc().unwrap().entity(ph).unwrap().clone();
    s.execute("xclip", &json!({"handle":h.hex(),"object":ph.hex()})).unwrap();
    assert_eq!(source, s.doc().unwrap().entity(ph).unwrap().clone());
    assert!(stored(&s, h).boundary.closed);
    s.doc_mut()
        .unwrap()
        .modify_entity(ph, |e| {
            if let EntityKind::LwPolyline(p) = &mut e.kind {
                p.vertices.first_mut().unwrap().bulge = 0.5;
            }
        })
        .unwrap();
    let before = clip(&s, h);
    assert!(s.execute("xclip", &json!({"handle":h.hex(),"object":ph.hex()})).is_err());
    assert_eq!(before, clip(&s, h));
}

#[test]
fn xclip_xref_nonzero_base_rotated_mirrored_and_nonuniform_reference() {
    let mut s = Session::new();
    let original = reference(&mut s);
    let d = s.doc_mut().unwrap();
    let block = std::sync::Arc::make_mut(d.blocks.get_mut("CLIP_TEST").unwrap());
    block.xref_path = Some("synthetic-missing-source.dxf".into());
    block.base = cadcraft_geom::Vec3::new(3.0, 4.0, 0.0);
    let r = s.execute("insert", &json!({"name":"CLIP_TEST","at":[100,50],"scale":[2,-3],"rotation":30})).unwrap();
    let h = Handle::parse_hex(r["handle"].as_str().unwrap()).unwrap();
    s.execute("xclip", &json!({"handle":h.hex(),"p1":[95,40],"p2":[110,65]})).unwrap();
    let out = s.execute("xclip", &json!({"handle":h.hex(),"option":"polyline"})).unwrap();
    let ph = Handle::parse_hex(out["polyline"].as_str().unwrap()).unwrap();
    let EntityKind::LwPolyline(p) = &s.doc().unwrap().entity(ph).unwrap().kind else { panic!("not polyline") };
    let b = cadcraft_geom::Bounds2::from_points(p.vertices.iter().map(|v| v.p));
    assert!(b.min.near(cadcraft_geom::Vec2::new(95.0, 40.0), 1e-8));
    assert!(b.max.near(cadcraft_geom::Vec2::new(110.0, 65.0), 1e-8));
    assert!(clip(&s, original)["clip"].is_null());
    assert_eq!(s.doc().unwrap().block("CLIP_TEST").unwrap().xref_path.as_deref(), Some("synthetic-missing-source.dxf"));
}

#[test]
fn xclip_generated_polyline_uses_current_truecolor() {
    let mut s = Session::new();
    let h = reference(&mut s);
    s.execute("xclip", &json!({"handle":h.hex(),"p1":[2,-1],"p2":[8,1]})).unwrap();
    s.doc_mut().unwrap().header.set_i64("CECOLOR_RGB", 0x123456);
    let r = s.execute("xclip", &json!({"handle":h.hex(),"option":"polyline"})).unwrap();
    let ph = Handle::parse_hex(r["polyline"].as_str().unwrap()).unwrap();
    assert_eq!(s.doc().unwrap().entity(ph).unwrap().common.color, cadcraft_doc::color::Color::True(cadcraft_doc::color::Rgb::from_u32(0x123456)));
}

#[test]
fn hostile_xclip_params_never_panic_or_mutate_the_reference() {
    let mut s = Session::new();
    let h = reference(&mut s);
    s.execute("xclip", &json!({"handle":h.hex(),"p1":[2,-1],"p2":[8,1]})).unwrap();
    let big = vec![[0.0, 0.0]; 20_000];
    let cases = vec![
        json!({}),
        json!({"option":"bogus"}),
        json!({"points":[]}),
        json!({"points":[[0,0],[1,1],[2,2]]}),
        json!({"points":[[0,0],[10,10],[0,10],[10,0]]}),
        json!({"points":[[0,0],[10,0],[10,10],[0,10],[10,0]]}),
        json!({"points":[[0,0],[1,"bad"],[0,1]]}),
        json!({"points":[[0,0],[1e308,0],[0,1e308]]}),
        json!({"points":big}),
        json!({"p1":[0,0],"p2":[0,0]}),
        json!({"object":"FFFFFFFFFFFFFFFF"}),
        json!({"object":h.hex()}),
        json!({"option":"clipdepth","front":-1}),
        json!({"option":"clipdepth","back":1e308}),
        json!({"option":"clipdepth","front":"bad"}),
        json!({"handle":-1}),
        json!({"handles":[h.hex(),h.hex()]}),
    ];
    for mut p in cases {
        if p.get("handle").is_none() && p.get("handles").is_none() {
            p["handle"] = json!(h.hex());
        }
        let before = serde_json::to_value(s.doc().unwrap().model.iter().collect::<Vec<_>>()).unwrap();
        let err = s.execute("xclip", &p).unwrap_err();
        assert!(!matches!(err, cadcraft_engine::EngineError::Internal(..)), "{p}: {err}");
        assert_eq!(before, serde_json::to_value(s.doc().unwrap().model.iter().collect::<Vec<_>>()).unwrap(), "{p}");
    }
}

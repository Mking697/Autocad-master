//! MATCHPROP's standalone Settings command shares typed settings without changing JSON defaults.
use cadcraft_engine::{EngineError, Input, Session};
use serde_json::json;

#[test]
fn settings_feed_typed_matchprop_but_json_matchprop_still_defaults_to_all() {
    let mut s = Session::new();
    s.execute("line", &json!({"points": [[0,0], [10,0]]})).unwrap();
    let src = s.doc().unwrap().model.last().unwrap().handle;
    s.execute("properties.set", &json!({"handle": src.hex(), "color": "red", "layer": "Source"})).unwrap();
    s.execute("line", &json!({"points": [[0,5], [10,5]]})).unwrap();
    let dst = s.doc().unwrap().model.last().unwrap().handle;
    s.execute("matchprop.settings", &json!({"properties": {"color": false}})).unwrap();
    s.start("matchprop").unwrap();
    s.input(Input::Pick(vec![src])).unwrap();
    s.input(Input::Pick(vec![dst])).unwrap();
    s.input(Input::Enter).unwrap();
    {
        let e = s.doc().unwrap().entity(dst).unwrap();
        assert_eq!(e.common.layer, "Source", "an enabled property really copied");
        assert_ne!(e.common.color, s.doc().unwrap().entity(src).unwrap().common.color, "a disabled property did not copy");
    }
    s.execute("matchprop", &json!({"source": src.hex(), "targets": [dst.hex()]})).unwrap();
    assert_eq!(s.doc().unwrap().entity(dst).unwrap().common.color, s.doc().unwrap().entity(src).unwrap().common.color);
    // The original keyword prompt also writes exactly the state the standalone dialog reads.
    s.start("matchprop").unwrap();
    s.input(Input::Pick(vec![src])).unwrap();
    s.cmdline("Settings").unwrap();
    s.cmdline("None").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("").unwrap();
    assert!(s.execute("matchprop.settings", &json!({})).unwrap()["properties"].as_object().unwrap().values().all(|v| v == false));
}

#[test]
fn empty_and_hostile_settings_requests_never_panic_or_partially_apply() {
    let mut s = Session::new();
    let initial = s.execute("matchprop.settings", &json!({})).unwrap();
    for p in [json!(null), json!({}), json!({"properties": {}})] {
        assert_eq!(s.execute("matchprop.settings", &p).unwrap(), initial);
    }
    let drawing = s.doc().unwrap().clone();
    for p in [
        json!([]),
        json!(true),
        json!("color"),
        json!(1e308),
        json!({"properties": null}),
        json!({"properties": []}),
        json!({"properties": {"color": false, "zzUnknown": true}}),
        json!({"properties": {"color": false, "layer": 1e308}}),
        json!({"properties": {"color": [false]}}),
        json!({"properties": {"material": true}}),
        json!({"properties": {"shadow": true}}),
        json!({"properties": {"centreObject": true}}),
    ] {
        let e = s.execute("matchprop.settings", &p).unwrap_err();
        assert!(!matches!(e, EngineError::Internal(..)), "guard caught a panic: {e}");
        assert_eq!(s.execute("matchprop.settings", &json!({})).unwrap(), initial);
        assert_eq!(s.doc().unwrap(), &drawing);
    }
}

//! RECOVER salvages a complete ASCII DXF prefix without changing OPEN's strict reader.
use cadcraft_engine::Session;
use cadcraft_engine::cmd::file::{IoHooks, base64_encode, set_io};
use serde_json::json;

#[test]
fn recover_salvages_complete_records_before_a_truncated_tail() {
    set_io(IoHooks {
        read: |b, n| cadcraft_io::read(b, n).map_err(|e| e.to_string()),
        write: |d, n| cadcraft_io::write(d, n).map_err(|e| e.to_string()),
        plot: None,
    });
    cadcraft_engine::cmd::file::set_recover(|b, n| cadcraft_io::recovery::recover(b, n).map_err(|e| e.to_string()));
    let broken = b"0\nSECTION\n2\nENTITIES\n0\nLINE\n5\n10\n10\n1\n20\n2\n11\n3\n21\n4\n0\nCIRCLE\n10\n9\n20";
    assert!(cadcraft_io::read(broken, "broken.dxf").is_err());
    let mut s = Session::new();
    s.execute("circle", &json!({"center": [0, 0], "radius": 2})).unwrap();
    let previous = s.state().unwrap().doc.clone();
    let result = s.execute("recover", &json!({"data": base64_encode(broken), "name": "broken.dxf"}));
    assert!(result.is_ok(), "RECOVER should salvage the complete line: {result:?}");
    let d = s.doc().unwrap();
    assert_eq!(d.model.len(), 1, "the incomplete circle must not be invented");
    assert!(matches!(&d.model.iter().next().unwrap().kind, cadcraft_engine::doc::EntityKind::Line(_)));
    assert!(d.read_notes.iter().any(|n| n.contains("RECOVER") && n.contains("discard")));
    assert!(s.state().unwrap().is_dirty(), "a salvaged drawing needs an explicit save");
    assert_eq!(previous.model.len(), 1, "the previously open drawing stays intact");
    assert!(std::sync::Arc::ptr_eq(&previous, &s.docs[0].doc));
    assert!(result.unwrap()["readNotes"].as_array().is_some_and(|notes| !notes.is_empty()));
    let active = s.active;
    let count = s.docs.len();
    let before_failure = s.state().unwrap().doc.clone();
    let error = s.execute("recover", &json!({"data": base64_encode(b"not a drawing"), "name": "bad.dxf"})).unwrap_err();
    assert!(!matches!(error, cadcraft_engine::EngineError::Internal(..)), "invalid input is not an internal/panic failure");
    assert_eq!(s.active, active);
    assert_eq!(s.docs.len(), count);
    assert!(std::sync::Arc::ptr_eq(&before_failure, &s.state().unwrap().doc));
}

//! Real recovery bytes, not a stubbed writer. DWG preference must not sacrifice foreign data.
use std::sync::Arc;
use std::time::Duration;

use cadcraft_doc::{EntityKind, Handle};
use cadcraft_engine::Session;
use cadcraft_engine::autosave::{self, Autosaver, Job};
use cadcraft_engine::cmd::file::{IoHooks, set_io};
use serde_json::json;

#[test]
fn autosave_format_preserves_foreign_entities_for_dwg_paths() {
    set_io(IoHooks {
        read: |b, n| cadcraft_io::read(b, n).map_err(|e| e.to_string()),
        write: |d, n| cadcraft_io::write(d, n).map_err(|e| e.to_string()),
        plot: None,
    });
    let source = b"0\nSECTION\n2\nENTITIES\n0\nCUSTOM_FRAME\n5\n2B\n100\nAcDbEntity\n8\n0\n100\nCustomFrame\n1\nforeign-payload\n0\nENDSEC\n0\nEOF\n";
    let dir = std::env::temp_dir().join(format!("cadcraft-format-{}", autosave::instance_id()));
    let mut s = Session::new();
    for doc_path in [None, Some("part.dxf"), Some("part.DWG"), Some("template.DwT")] {
        s.state_mut().unwrap().doc = Arc::new(cadcraft_io::read(source, "source.dxf").unwrap());
        s.state_mut().unwrap().path = doc_path.map(str::to_string);
        let mut a = Autosaver::new("81-a");
        a.plan(&s, 0.0, 1, &dir.to_string_lossy());
        let jobs = a.plan(&s, 60.0, 1, &dir.to_string_lossy());
        assert_eq!(jobs.len(), 1);
        let Job::Write { path, hint, .. } = &jobs[0] else { panic!("expected write") };
        let path = path.clone();
        assert_eq!(hint, if matches!(doc_path, Some("part.DWG" | "template.DwT")) { "autosave.dwg" } else { "autosave.dxf" });
        a.perform(jobs);
        for _ in 0..500 {
            a.reap();
            if std::path::Path::new(&path).exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let bytes = std::fs::read(&path).unwrap();
        assert!(!bytes.starts_with(b"AC10"), "lossy DWG must not replace recovery: {doc_path:?}");
        assert!(bytes.starts_with(b"  0\r\nSECTION\r\n"), "ASCII DXF signature: {doc_path:?}");
        assert!(bytes.trim_ascii_end().ends_with(b"EOF"));
        let reopened = cadcraft_io::read(&bytes, &path).unwrap();
        assert!(matches!(&reopened.entity(Handle(0x2B)).unwrap().kind, EntityKind::Unknown(u) if u.dxf_type == "CUSTOM_FRAME"));
        assert!(String::from_utf8(bytes).unwrap().contains("foreign-payload"));
        a.shutdown();
        assert!(!std::path::Path::new(&path).exists());
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn autosave_format_hint_follows_each_drawing_not_the_active_tab() {
    let mut s = Session::new();
    s.state_mut().unwrap().path = Some("part.dwg".into());
    s.execute("circle", &json!({"center": [0, 0], "radius": 1})).unwrap();
    s.execute("new", &json!({})).unwrap();
    s.state_mut().unwrap().path = Some("part.dxf".into());
    s.execute("circle", &json!({"center": [1, 1], "radius": 2})).unwrap();
    let mut a = Autosaver::new("81-b");
    a.plan(&s, 0.0, 1, "unused");
    let jobs = a.plan(&s, 60.0, 1, "unused");
    let mut hints: Vec<_> = jobs
        .iter()
        .filter_map(|j| match j {
            Job::Write { hint, .. } => Some(hint.as_str()),
            _ => None,
        })
        .collect();
    hints.sort_unstable();
    assert_eq!(hints, ["autosave.dwg", "autosave.dxf"]);
}

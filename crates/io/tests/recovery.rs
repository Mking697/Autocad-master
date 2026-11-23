//! Conservative salvage: complete records only, explicit warnings, no speculative resync.
use cadcraft_io::recovery::{MAX_RECOVERY_BYTES, recover};

const LINE: &str = "0\nSECTION\n2\nENTITIES\n0\nLINE\n5\n10\n10\n1\n20\n2\n11\n3\n21\n4\n";

#[test]
fn valid_input_uses_the_normal_reader_without_a_salvage_warning() {
    let text = format!("{LINE}0\nENDSEC\n0\nEOF\n");
    let d = recover(text.as_bytes(), "good.dxf").unwrap();
    assert_eq!(d.model.len(), 1);
    assert!(d.read_notes.iter().all(|n| !n.starts_with("RECOVER:")));
}

#[test]
fn malformed_codes_and_partial_values_discard_the_entire_damaged_record() {
    for tail in ["0\nCIRCLE\n10\n9\n20", "0\nCIRCLE\n10\n9\nbad\n4\n", "0"] {
        for crlf in [false, true] {
            let mut text = format!("{LINE}{tail}");
            if crlf {
                text = text.replace('\n', "\r\n");
            }
            let before = text.clone();
            let d = recover(text.as_bytes(), "broken.dxf").unwrap();
            assert_eq!(text, before, "the supplied source is never rewritten");
            assert_eq!(d.model.len(), 1);
            assert!(matches!(d.model.iter().next().map(|e| &e.kind), Some(cadcraft_doc::EntityKind::Line(_))));
            assert!(d.read_notes.iter().any(|n| n.starts_with("RECOVER:") && n.contains("discarded")));
        }
    }
}

#[test]
fn refuses_non_entity_damage_binary_and_empty_prefixes() {
    for bytes in [
        b"not a drawing".as_slice(),
        b"0\nSECTION\n2\nENTITIES\n0\nCIRCLE\n10",
        b"0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1",
        b"AutoCAD Binary DXF\r\n\x1a\0\x00",
        b"AC1032\0garbage",
    ] {
        let r = std::panic::catch_unwind(|| recover(bytes, "bad.dxf"));
        assert!(r.is_ok(), "a bad drawing must not panic");
        assert!(r.unwrap().is_err(), "must not invent a recovered drawing");
    }
}

#[test]
fn recovery_budget_is_checked_before_parsing() {
    let bytes = vec![0; MAX_RECOVERY_BYTES + 1];
    let result = recover(&bytes, "oversize.dxf");
    assert!(result.unwrap_err().to_string().contains("budget"));
}

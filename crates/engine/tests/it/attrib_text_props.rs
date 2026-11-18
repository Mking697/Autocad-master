//! -ATTEDIT Position, Height, Angle and Style of one attribute (F1 narrowed).

use cadcraft_engine::Session;
use cadcraft_engine::doc::{EntityKind, Handle};
use serde_json::json;

/// Block B (attribute T at [0,1], default "one") inserted at 100,0.
fn setup() -> (Session, Handle) {
    let mut s = Session::new();
    s.execute("attdef", &json!({"tag": "T", "default": "one", "at": [0, 1]})).unwrap();
    s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
    let hs: Vec<String> = s.doc().unwrap().model.iter().map(|e| e.handle.hex()).collect();
    s.execute("block", &json!({"name": "B", "base": [0, 0], "handles": hs, "keep": "delete"})).unwrap();
    s.execute("style", &json!({"name": "Narrow"})).unwrap();
    let r = s.execute("insert", &json!({"name": "B", "at": [100, 0]})).unwrap();
    (s, Handle::parse_hex(r["handle"].as_str().unwrap()).unwrap())
}

fn text(s: &Session, h: Handle, tag: &str) -> cadcraft_engine::doc::Text {
    let EntityKind::Insert(i) = &s.doc().unwrap().entity(h).unwrap().kind else { panic!("not an insert") };
    i.attribs.iter().find(|a| a.tag == tag).unwrap().text.clone()
}

#[test]
fn dash_attedit_sets_position_height_angle_and_style_of_one_attribute() {
    let (mut s, h) = setup();
    for t in ["-attedit", "y", "", "T", "", "105,0", ""] {
        s.cmdline(t).unwrap();
    }
    assert!(s.prompt_text().contains("[Value/Position/Height/Angle/Style/Layer/Color/Next] <N>"), "{}", s.prompt_text());

    s.cmdline("p").unwrap();
    assert!(s.prompt_text().contains("Specify new text insertion point"), "{}", s.prompt_text());
    s.cmdline("120,5").unwrap();
    assert!(s.prompt_text().contains("Enter an option"), "{}", s.prompt_text());

    s.cmdline("h").unwrap();
    assert!(s.prompt_text().contains("Specify new height"), "{}", s.prompt_text());
    s.cmdline("3").unwrap();

    s.cmdline("a").unwrap();
    assert!(s.prompt_text().contains("Specify new rotation angle"), "{}", s.prompt_text());
    s.cmdline("45").unwrap();

    s.cmdline("s").unwrap();
    assert!(s.prompt_text().contains("Enter new text style name"), "{}", s.prompt_text());
    // An unknown style is refused and asked again.
    s.cmdline("nope").unwrap();
    assert!(s.log.iter().any(|l| l.contains("Style \"nope\" not found")), "{:?}", s.log);
    s.cmdline("Narrow").unwrap();

    s.cmdline("n").unwrap();
    assert!(s.running.is_none(), "{}", s.prompt_text());

    let t = text(&s, h, "T");
    assert_eq!((t.insert.x, t.insert.y), (120.0, 5.0));
    assert_eq!(t.height, 3.0);
    assert!((t.rotation.to_degrees() - 45.0).abs() < 1e-9);
    assert_eq!(t.style, "Narrow");
}

/// Enter at Position/Height/Angle/Style leaves that property unchanged.
#[test]
fn dash_attedit_text_props_enter_keeps_the_current_value() {
    let (mut s, h) = setup();
    let before = text(&s, h, "T");
    for t in ["-attedit", "y", "", "T", "", "105,0", ""] {
        s.cmdline(t).unwrap();
    }
    for opt in ["p", "h", "a", "s"] {
        s.cmdline(opt).unwrap();
        s.cmdline("").unwrap();
    }
    s.cmdline("n").unwrap();
    assert!(s.running.is_none());
    assert_eq!(text(&s, h, "T"), before);
}

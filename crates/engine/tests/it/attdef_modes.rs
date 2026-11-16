//! ATTDEF's Verify, Preset and Lock position modes (F17): the JSON command, the -ATTDEF
//! command-line modes toggle, and their effect at INSERT (Preset skips its prompt and uses the
//! default; Verify asks a second time to confirm the value, in the typed/-INSERT path).

use cadcraft_engine::Session;
use cadcraft_engine::doc::{AttribModes, EntityKind};
use serde_json::json;

fn attdef_modes(s: &Session, tag: &str) -> (bool, bool, AttribModes) {
    let d = s.doc().unwrap();
    let b = d.block("B").unwrap();
    let a = b
        .entities
        .iter()
        .find_map(|e| if let EntityKind::AttDef(a) = &e.kind { (a.tag == tag).then(|| a.clone()) } else { None })
        .unwrap_or_else(|| panic!("no attdef {tag}"));
    (a.invisible, a.constant, a.modes)
}

#[test]
fn attdef_json_accepts_and_stores_verify_preset_and_lock_position() {
    let mut s = Session::new();
    s.execute(
        "attdef",
        &json!({ "tag": "A", "default": "a", "at": [0, 0], "invisible": true, "constant": true, "verify": true, "preset": true, "lockPosition": true }),
    )
    .unwrap();
    s.execute("attdef", &json!({ "tag": "B", "default": "b", "at": [0, 1] })).unwrap();
    let handles: Vec<String> = s.doc().unwrap().model.iter().map(|e| e.handle.hex()).collect();
    s.execute("block", &json!({ "name": "B", "base": [0, 0], "keep": "delete", "handles": handles })).unwrap();

    let (inv, con, modes) = attdef_modes(&s, "A");
    assert!(inv && con);
    assert_eq!(modes, AttribModes { verify: true, preset: true, lock_position: true });
    let (inv, con, modes) = attdef_modes(&s, "B");
    assert!(!inv && !con);
    assert_eq!(modes, AttribModes::default());
}

#[test]
fn attdef_modes_round_trip_through_dxf_save_and_reload() {
    let mut s = Session::new();
    s.execute("attdef", &json!({ "tag": "A", "default": "a", "at": [0, 0], "verify": true, "preset": true, "lockPosition": true })).unwrap();
    let handles: Vec<String> = s.doc().unwrap().model.iter().map(|e| e.handle.hex()).collect();
    s.execute("block", &json!({ "name": "B", "base": [0, 0], "keep": "delete", "handles": handles })).unwrap();
    let dxf = cadcraft_io::write(s.doc().unwrap(), "x.dxf").unwrap();
    let d = cadcraft_io::read(&dxf, "x.dxf").unwrap();
    let a = d.block("B").unwrap().entities.iter().find_map(|e| if let EntityKind::AttDef(a) = &e.kind { Some(a.modes) } else { None }).unwrap();
    assert_eq!(a, AttribModes { verify: true, preset: true, lock_position: true });
}

#[test]
fn dash_attdef_toggles_modes_through_its_menu() {
    let mut s = Session::new();
    // Toggle Verify and Preset, leave Invisible/Constant/Lock position off, then Done (Enter).
    s.script("ATTDEF\nV\nP\n\nTAG\nMy Prompt\ndefault\n0,0\n").unwrap();
    assert!(s.running.is_none(), "ATTDEF finished: {}", s.prompt_text());
    let handles: Vec<String> = s.doc().unwrap().model.iter().map(|e| e.handle.hex()).collect();
    s.execute("block", &json!({ "name": "B", "base": [0, 0], "keep": "delete", "handles": handles })).unwrap();
    let (inv, con, modes) = attdef_modes(&s, "TAG");
    assert!(!inv && !con);
    assert_eq!(modes, AttribModes { verify: true, preset: true, lock_position: false });
}

/// A block with a plain, a Preset and a Verify attribute, defined at the origin.
fn block_with_attribute_modes(s: &mut Session) {
    s.execute("attdef", &json!({ "tag": "NORMAL", "default": "n", "at": [0, 1] })).unwrap();
    s.execute("attdef", &json!({ "tag": "PRESET", "default": "p", "preset": true, "at": [0, 2] })).unwrap();
    s.execute("attdef", &json!({ "tag": "VERIFY", "default": "v", "verify": true, "at": [0, 3] })).unwrap();
    let handles: Vec<String> = s.doc().unwrap().model.iter().map(|e| e.handle.hex()).collect();
    s.execute("block", &json!({ "name": "B", "base": [0, 0], "keep": "delete", "handles": handles })).unwrap();
}

fn attrib_value(s: &Session, tag: &str) -> Option<String> {
    s.doc()
        .unwrap()
        .model
        .iter()
        .find_map(|e| if let EntityKind::Insert(i) = &e.kind { i.attribs.iter().find(|a| a.tag == tag).map(|a| a.text.value.clone()) } else { None })
}

#[test]
fn insert_skips_the_prompt_for_a_preset_attribute_and_uses_its_default() {
    let mut s = Session::new();
    block_with_attribute_modes(&mut s);
    // Only two text prompts are answered: NORMAL, then VERIFY (asked twice to confirm). If
    // PRESET were still prompted, the command would still be running after these three lines.
    s.script("-INSERT B 10,0 1 1 0\nnval\nvval\nvval\n").unwrap();
    assert!(s.running.is_none(), "INSERT finished: {}", s.prompt_text());
    assert_eq!(attrib_value(&s, "NORMAL").as_deref(), Some("nval"));
    assert_eq!(attrib_value(&s, "PRESET").as_deref(), Some("p"));
    assert_eq!(attrib_value(&s, "VERIFY").as_deref(), Some("vval"));
}

#[test]
fn insert_reprompts_to_verify_and_the_second_answer_wins() {
    let mut s = Session::new();
    block_with_attribute_modes(&mut s);
    s.script("-INSERT B 10,0 1 1 0\nnval\nfirst\ncorrected\n").unwrap();
    assert!(s.running.is_none(), "INSERT finished: {}", s.prompt_text());
    assert_eq!(attrib_value(&s, "VERIFY").as_deref(), Some("corrected"));
}

#[test]
fn insert_verify_reprompt_keeps_the_value_on_enter() {
    let mut s = Session::new();
    block_with_attribute_modes(&mut s);
    // Enter (blank line) at the verify re-prompt keeps the just-typed value.
    s.script("-INSERT B 10,0 1 1 0\nnval\nkept\n\n").unwrap();
    assert!(s.running.is_none(), "INSERT finished: {}", s.prompt_text());
    assert_eq!(attrib_value(&s, "VERIFY").as_deref(), Some("kept"));
}

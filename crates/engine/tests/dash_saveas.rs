//! -SAVEAS typed at the command line (FF3): asks for format, version and file name as plain
//! text, matching AutoCAD's classic dash-prefixed commands, instead of opening the native Save
//! dialog ("saveas"/"ui.saveas" do that — see tests/save_version.rs for their JSON-form tests).

use cadcraft_doc::Drawing;
use cadcraft_engine::Session;
use cadcraft_engine::cmd::file::{IoHooks, SaveAs, set_io, set_write_as};

/// The writer with options: the bytes name the options it got.
fn with_options(_: &Drawing, _: &str, o: &SaveAs) -> Result<Vec<u8>, String> {
    Ok(format!("{}|{}", o.version.map(|v| v.year()).unwrap_or("-"), o.binary).into_bytes())
}

fn install() {
    set_io(IoHooks { read: |_, _| Ok(Drawing::new_metric()), write: |_, _| Ok(b"plain".to_vec()), plot: None });
    set_write_as(with_options);
}

#[test]
fn dash_saveas_prompts_for_format_version_binary_and_name() {
    install();
    let dir = std::env::temp_dir().join(format!("cadcraft-dash-saveas-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = |name: &str| dir.join(name).to_string_lossy().to_string();
    let read = |p: &str| std::fs::read(p).unwrap();
    let mut s = Session::new();
    s.cmdline("circle 0,0 5").unwrap();

    // DXF, a chosen version, binary, and a name with no extension (the command appends .dxf).
    s.cmdline("-saveas").unwrap();
    assert_eq!(s.current_prompt().unwrap().message, "Enter file format");
    s.cmdline("dxf").unwrap();
    assert_eq!(s.current_prompt().unwrap().message, "Enter file version");
    s.cmdline("2010").unwrap();
    assert_eq!(s.current_prompt().unwrap().message, "Save as binary DXF");
    s.cmdline("yes").unwrap();
    assert_eq!(s.current_prompt().unwrap().message, "Enter file name");
    let stem = path("a");
    s.cmdline(&stem).unwrap();
    assert!(s.running.is_none(), "the command finished");
    assert_eq!(read(&format!("{stem}.dxf")), b"2010|true");
    assert_eq!(s.state().unwrap().path.as_deref(), Some(format!("{stem}.dxf").as_str()));

    // DWG, defaults (Enter at format and version), no binary prompt for DWG, a name with its own
    // extension (kept as given).
    s.cmdline("-saveas").unwrap();
    s.cmdline("").unwrap(); // Enter: default format (Dwg)
    s.cmdline("").unwrap(); // Enter: default version (2018)
    assert_eq!(s.current_prompt().unwrap().message, "Enter file name", "DWG skips the binary prompt");
    let out = path("b.dwg");
    s.cmdline(&out).unwrap();
    assert_eq!(read(&out), b"2018|false");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dash_saveas_rejects_bad_format_and_version_without_advancing() {
    install();
    let mut s = Session::new();
    s.cmdline("-saveas").unwrap();
    s.cmdline("bogus").unwrap();
    assert_eq!(s.current_prompt().unwrap().message, "Enter file format", "a bad format re-asks");
    s.cmdline("dwg").unwrap();
    s.cmdline("1999").unwrap();
    assert_eq!(s.current_prompt().unwrap().message, "Enter file version", "a bad version re-asks");
    s.cmdline("2018").unwrap();
    assert_eq!(s.current_prompt().unwrap().message, "Enter file name");
    s.cmdline("").unwrap(); // empty name cancels
    assert!(s.running.is_none(), "an empty name cancels the command");
}

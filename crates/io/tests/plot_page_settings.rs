//! Plot page settings retained in CADCraft's LAYOUT xdata, not an invented plot-layout bit.

use cadcraft_doc::{Drawing, Layout};
use cadcraft_io::{SaveOptions, Version, read, write_with};

fn drawing() -> Drawing {
    let mut d = Drawing::new_metric();
    d.layouts = vec![Layout::new("Transparent", 1), Layout::new("Opaque", 2)];
    d.layouts[0].page.plot_transparency = true;
    d.layouts[0].view = Some((cadcraft_geom::Vec2::new(12.0, 34.0), 56.0));
    d
}

#[test]
fn plot_transparency_survives_dxf_save_reopen_and_toggle() {
    let d = drawing();
    for version in [Version::R2000, Version::R2018] {
        for binary in [false, true] {
            let options = SaveOptions { version: Some(version), binary };
            let bytes = write_with(&d, "plot.dxf", &options).unwrap();
            let mut back = read(&bytes, "plot.dxf").unwrap();
            assert!(back.layout("Transparent").unwrap().page.plot_transparency);
            assert!(!back.layout("Opaque").unwrap().page.plot_transparency);
            assert_eq!(back.layout("Transparent").unwrap().view, d.layout("Transparent").unwrap().view);
            back.layouts[0].page.plot_transparency = false;
            let second = write_with(&back, "plot.dxf", &options).unwrap();
            assert!(!read(&second, "plot.dxf").unwrap().layout("Transparent").unwrap().page.plot_transparency);
        }
    }
}

#[test]
fn malformed_or_foreign_plot_transparency_xdata_keeps_default() {
    let d = drawing();
    let bytes = write_with(&d, "plot.dxf", &SaveOptions::default()).unwrap();
    let text = String::from_utf8(bytes).unwrap().replace("\r\n", "\n");
    // Insert before the existing view list, so this works even on a writer without the new flag.
    let marker = "1000\nPSVIEW\n";
    assert!(text.contains(marker));
    for payload in [
        "1000\nPLOTTRANSPARENCY\n1002\n{\n1070\n2\n1002\n}\n",
        "1000\nPLOTTRANSPARENCY\n1002\n{\n1070\n-1\n1002\n}\n",
        "1000\nPLOTTRANSPARENCY\n1002\n{\n1000\ntrue\n1002\n}\n",
        "1000\nPLOTTRANSPARENCY\n1002\n{\n1070\n1\n1070\n1\n1002\n}\n",
        "1000\nPLOTTRANSPARENCY\n1070\n1\n",
    ] {
        let mut opaque = d.clone();
        opaque.layouts[0].page.plot_transparency = false;
        let base = String::from_utf8(write_with(&opaque, "plot.dxf", &SaveOptions::default()).unwrap()).unwrap().replace("\r\n", "\n");
        let hostile = base.replace(marker, &format!("{payload}{marker}"));
        let back = read(hostile.as_bytes(), "plot.dxf").unwrap();
        assert!(!back.layout("Transparent").unwrap().page.plot_transparency, "{payload}");
    }
    let foreign = text.replace("1001\nCADCRAFT\n", "1001\nFOREIGN\n");
    assert!(!read(foreign.as_bytes(), "plot.dxf").unwrap().layout("Transparent").unwrap().page.plot_transparency);
}

#[test]
fn plot_transparency_survives_dwg_save_reopen() {
    let d = drawing();
    let options = SaveOptions { version: Some(Version::R2018), binary: false };
    let bytes = write_with(&d, "plot.dwg", &options).unwrap();
    let back = read(&bytes, "plot.dwg").unwrap();
    assert!(back.layout("Transparent").unwrap().page.plot_transparency);
    assert!(!back.layout("Opaque").unwrap().page.plot_transparency);
}

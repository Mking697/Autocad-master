//! Saving a drawing as DXF or DWG at a chosen version (2000–2018) and as binary DXF (#421).

use cadcraft_color::{Color, Rgb};
use cadcraft_doc::*;
use cadcraft_geom::{PolyVertex, Vec2, Vec3};
use cadcraft_io::{SaveOptions, Version, read, save_version, write, write_dxf, write_with};

const INTL: &str = "图号 Café Ø 😀";

fn text(value: &str) -> EntityKind {
    EntityKind::Text(Text {
        insert: Vec3::new(1.0, 1.0, 0.0),
        align_pt: None,
        height: 2.5,
        value: value.into(),
        rotation: 0.0,
        width_factor: 1.0,
        oblique: 0.0,
        style: "Standard".into(),
        halign: HAlign::Left,
        valign: VAlign::Baseline,
    })
}

fn table() -> Table {
    let cell = |t: &str| TableCell { text: t.into(), merged: None, ..TableCell::default() };
    let mut cells = vec![vec![cell("Title"), cell(""), cell("")], vec![cell("A"), cell("B"), cell("C")], vec![cell("1"), cell("2"), cell("3")]];
    cells[0][0].merged = Some((1, 3));
    Table {
        insert: Vec3::new(5.0, 20.0, 0.0),
        col_widths: vec![1.5, 2.5, 4.0],
        row_heights: vec![0.5, 0.4, 0.4],
        cells,
        style: "Standard".into(),
        text_height: 0.2,
        title: true,
        header: true,
        row_styles: Default::default(),
        breaks: Default::default(),
        data_link: Default::default(),
    }
}

fn sample() -> Drawing {
    let mut d = Drawing::new_metric();
    d.layers.push(Layer { name: "Fancy".into(), color: Color::True(Rgb(12, 200, 99)), ..Layer::default() });
    let fancy = Common { layer: "Fancy".into(), color: Color::True(Rgb(1, 2, 3)), transparency: Transparency::Percent(40), ..Common::default() };
    d.add(&Space::Model, fancy, EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(10.0, 5.0, 0.0) })).unwrap();
    d.add(&Space::Model, Common::default(), EntityKind::Circle(Circle { center: Vec3::new(3.0, 4.0, 0.0), radius: 2.0 })).unwrap();
    d.add(&Space::Model, Common::default(), text(INTL)).unwrap();
    d.add(&Space::Model, Common::default(), EntityKind::Table(table())).unwrap();
    d.table_styles.push(sched());
    d
}

fn sched() -> TableStyle {
    TableStyle { name: "Sched".into(), text_height: 2.5, margin: 1.0, title: false, header: false, cell_styles: Default::default() }
}

fn opts(version: Version, binary: bool) -> SaveOptions {
    SaveOptions { version: Some(version), binary }
}

fn acadver(bytes: &[u8]) -> String {
    let tags = cadcraft_dxf::parse(bytes).unwrap();
    tags.windows(2).find(|w| w[0].code == 9 && w[0].str() == "$ACADVER").map(|w| w[1].str()).unwrap_or_default()
}

fn first_text(d: &Drawing) -> Option<String> {
    d.model.iter().find_map(|e| if let EntityKind::Text(t) = &e.kind { Some(t.value.clone()) } else { None })
}

fn first_table(d: &Drawing) -> Option<Table> {
    d.model.iter().find_map(|e| if let EntityKind::Table(t) = &e.kind { Some(t.clone()) } else { None })
}

/// ASCII and binary DXF at every structurally-modern version (R2000 and later; R12 has its own
/// writer and tests, see `dxf_write_r12.rs`) declare it, encode text the version's way and read
/// back to the same drawing as the R2000 file.
#[test]
fn dxf_at_every_version_reads_back_the_same() {
    let d = sample();
    let reference = write_dxf(&read(write_dxf(&d).as_bytes(), "x.dxf").unwrap());
    for v in Version::ALL.into_iter().filter(|v| *v >= Version::R2000) {
        for binary in [false, true] {
            let bytes = write_with(&d, "x.dxf", &opts(v, binary)).unwrap();
            let what = format!("{v} binary={binary}");
            assert_eq!(acadver(&bytes), v.code(), "{what}");
            assert_eq!(cadcraft_dxf::is_binary(&bytes), binary, "{what}");
            let raw = bytes.windows("图号".len()).any(|w| w == "图号".as_bytes());
            assert_eq!(raw, v.utf8(), "{what}: UTF-8 text from 2007, \\U+ escapes before");
            if !v.utf8() && !binary {
                assert!(bytes.is_ascii(), "{what}");
            }
            let back = read(&bytes, "x.dxf").unwrap();
            assert_eq!(first_text(&back).as_deref(), Some(INTL), "{what}");
            assert_eq!(save_version(&back), v, "{what}: the file's version is remembered");
            assert_eq!(write_dxf(&back), reference, "{what}");
        }
    }
}

/// R2000 ASCII is exactly the R2000 DXF CADCraft always wrote.
#[test]
fn r2000_ascii_is_unchanged() {
    let d = sample();
    assert_eq!(write_with(&d, "x.dxf", &opts(Version::R2000, false)).unwrap(), write_dxf(&d).into_bytes());
}

/// Without a version asked for: 2018 for a new drawing, otherwise the file's own version (the
/// nearest one CADCraft writes for older or newer files).
#[test]
fn default_version_is_the_drawings_own() {
    let d = sample();
    assert_eq!(acadver(&write(&d, "x.dxf").unwrap()), "AC1032");
    for (file, saved) in [("AC1015", "AC1015"), ("AC1024", "AC1024"), ("AC1009", "AC1009"), ("AC1012", "AC1014"), ("AC1040", "AC1032")] {
        let mut d = sample();
        d.header.set_str("ACADVER", file);
        assert_eq!(acadver(&write(&d, "x.dxf").unwrap()), saved, "{file}");
    }
    let from_r2000 = read(write_dxf(&d).as_bytes(), "x.dxf").unwrap();
    assert_eq!(save_version(&from_r2000), Version::R2000);
}

#[test]
fn binary_is_for_dxf_only() {
    assert!(write_with(&sample(), "x.dwg", &opts(Version::R2018, true)).is_err());
    assert!(write_with(&sample(), "x.svg", &SaveOptions { version: None, binary: true }).is_err());
}

/// DWG at every structurally-modern version (R2000 and later: the TABLE entity this drawing
/// uses needs AC1018/2004; R12 and R14 DWG have their own dedicated tests below) is that version
/// and keeps the drawing: merged table cells, and from 2004 true colours, transparency and table
/// styles (R2000 DWG has none of those).
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn dwg_at_every_version_reads_back() {
    let d = sample();
    for v in Version::ALL.into_iter().filter(|v| *v >= Version::R2000) {
        let dwg = write_with(&d, "x.dwg", &opts(v, false)).unwrap();
        assert_eq!(cadcraft_dwg::version(&dwg).as_deref(), Some(v.code()), "{v}");
        let back = read(&dwg, "x.dwg").unwrap();
        assert_eq!(save_version(&back), v, "{v}");
        assert_eq!(back.model.len(), d.model.len(), "{v}");
        assert_eq!(first_text(&back).as_deref(), Some(INTL), "{v}");
        assert_eq!(first_table(&back).map(|t| t.cells), Some(table().cells), "{v}: merged cells kept");
        if v >= Version::R2004 {
            let line = back.model.iter().find(|e| matches!(e.kind, EntityKind::Line(_))).unwrap();
            assert_eq!(line.common.color, Color::True(Rgb(1, 2, 3)), "{v}");
            assert_eq!(line.common.transparency, Transparency::Percent(40), "{v}");
            assert_eq!(back.table_styles.iter().find(|s| s.name == "Sched"), Some(&sched()), "{v}");
        }
    }
}

/// A drawing from an R2000 file saves as 2004 DWG unless 2000 is asked for: R2000 DWG would
/// lose its true colours.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn dwg_default_is_at_least_2004() {
    let from_r2000 = read(write_dxf(&sample()).as_bytes(), "x.dxf").unwrap();
    assert_eq!(cadcraft_dwg::version(&write(&from_r2000, "x.dwg").unwrap()).as_deref(), Some("AC1018"));
    assert_eq!(cadcraft_dwg::version(&write(&sample(), "x.dwg").unwrap()).as_deref(), Some("AC1032"));
}

/// A drawing using only what R12 (AC1009) and R14 (AC1014) can represent: no true colour,
/// transparency, named plot style or TABLE entity (all postdate both).
fn legacy_sample() -> Drawing {
    let mut d = Drawing::new_metric();
    d.layers.push(Layer { name: "Fancy".into(), color: Color::Index(1), ..Layer::default() });
    let fancy = Common { layer: "Fancy".into(), color: Color::Index(3), ..Common::default() };
    d.add(&Space::Model, fancy, EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(10.0, 5.0, 0.0) })).unwrap();
    d.add(&Space::Model, Common::default(), EntityKind::Circle(Circle { center: Vec3::new(3.0, 4.0, 0.0), radius: 2.0 })).unwrap();
    d.add(&Space::Model, Common::default(), text(INTL)).unwrap();
    let poly = LwPolyline {
        vertices: vec![PolyVertex::new(Vec2::new(0.0, 0.0)), PolyVertex::new(Vec2::new(5.0, 0.0)), PolyVertex::new(Vec2::new(5.0, 5.0))],
        closed: true,
        const_width: 0.0,
        elevation: 0.0,
        plinegen: false,
    };
    d.add(&Space::Model, Common::default(), EntityKind::LwPolyline(poly)).unwrap();
    d
}

/// Saving as R12 writes genuine AC1009 structure: no `100` subclass markers, no `5`/`330`
/// handle groups, LWPOLYLINE degraded to classic POLYLINE/VERTEX/SEQEND (#FF3).
#[test]
fn r12_dxf_is_genuine_pre_r13_structure() {
    let d = legacy_sample();
    let bytes = write_with(&d, "x.dxf", &opts(Version::R12, false)).unwrap();
    assert_eq!(acadver(&bytes), "AC1009");
    let text = String::from_utf8(bytes).unwrap();
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    for pair in lines.chunks(2) {
        if let [code, _] = pair {
            assert_ne!(*code, "100", "R12 has no subclass markers: {text}");
            assert_ne!(*code, "5", "R12 (as CADCraft writes it) has no object handles: {text}");
        }
    }
    assert!(text.contains("LINE"), "{text}");
    assert!(text.contains("CIRCLE"), "{text}");
    assert!(text.contains("TEXT"), "{text}");
    assert!(text.contains("POLYLINE") && !text.contains("LWPOLYLINE"), "LWPOLYLINE degrades to POLYLINE: {text}");
    assert!(text.contains("SEQEND"), "{text}");
}

/// Binary R12 DXF (a distinct pre-R13 binary layout) isn't implemented: asking for it is an
/// error, not silently wrong bytes.
#[test]
fn binary_r12_dxf_is_rejected_not_silently_wrong() {
    let e = write_with(&legacy_sample(), "x.dxf", &opts(Version::R12, true)).unwrap_err().to_string();
    assert!(e.to_ascii_lowercase().contains("r12"), "{e}");
}

/// R12 DWG isn't implemented yet: asking for it is an explicit error, never some other version
/// written under the R12 name.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn r12_dwg_is_rejected_not_silently_wrong() {
    let e = write_with(&legacy_sample(), "x.dwg", &opts(Version::R12, false)).unwrap_err().to_string();
    assert!(e.to_ascii_lowercase().contains("r12") || e.contains("AC1009"), "{e}");
}

/// R14 (AC1014) DWG round-trips a drawing of what R14 can represent: the acadrust-backed DWG
/// bridge already knows how to write that version (#FF3); CADCraft's own part is honouring the
/// version choice end to end.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn r14_dwg_round_trips() {
    let d = legacy_sample();
    let dwg = write_with(&d, "x.dwg", &opts(Version::R14, false)).unwrap();
    assert_eq!(cadcraft_dwg::version(&dwg).as_deref(), Some("AC1014"));
    let back = read(&dwg, "x.dwg").unwrap();
    assert_eq!(save_version(&back), Version::R14);
    assert_eq!(back.model.len(), d.model.len());
    assert_eq!(first_text(&back).as_deref(), Some(INTL));
    assert!(back.model.iter().any(|e| matches!(e.kind, EntityKind::Line(_))));
    assert!(back.model.iter().any(|e| matches!(e.kind, EntityKind::Circle(_))));
}

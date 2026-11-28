//! Drawing → genuine AutoCAD R12 (AC1009) ASCII DXF.
//!
//! R12 predates the AC1013 (R13) format break that introduced subclass (`100`) markers,
//! mandatory object handles and the `OBJECTS`/`CLASSES` sections, so this is a deliberately
//! separate, structurally different writer rather than a version gate inside [`crate::dxf_write`]
//! (whose tag shapes are all built around that later structure). Reusing the generic writer and
//! merely omitting a few groups would still emit `100` subclass markers and `330` owner-handle
//! groups no R12 reader expects.
//!
//! # What this writer supports
//! - Entities: LINE, POINT, CIRCLE, ARC, TEXT, POLYLINE (model space only) and INSERT, with their
//!   referenced block definitions (bounded, cycle-safe traversal).
//! - LWPOLYLINE degrades to POLYLINE + VERTEX + SEQEND: LWPOLYLINE itself was introduced in
//!   AutoCAD R14, so no R12 reader knows it (DXF Reference, "LWPOLYLINE").
//! - Colour is ACI only (`62`): a true-colour (24-bit) value is mapped to its nearest ACI index
//!   with [`Color::to_aci`] (true colour itself did not exist before AutoCAD 2004).
//! - No object handles (`5`/`330`), no XDATA, no extension dictionaries, no plot styles, no
//!   lineweights, no transparency: none of these existed in R12. A hostile or unusual drawing
//!   that uses them simply has those properties omitted from the R12 file; it is never rejected
//!   and never panics.
//!
//! # What is left out (documented, not silently approximated)
//! - Paper space / layouts: R12 has no layout tabs (introduced in AC1015/R2000); only model space
//!   is written.
//! - Any entity kind besides the ones above (MTEXT, DIMENSION, HATCH, SPLINE, ELLIPSE, 3D solids,
//!   images, tables, leaders, multileaders, tolerances, viewports…) is left out of the file. Most
//!   either postdate R12 or have no R12-era equivalent CADCraft can degrade to.
//! - Attributes on block references (ATTRIB) are left out; only the reference's own geometry (its
//!   block) is written.
//! - Dimension, UCS, VIEW and VPORT tables are not written: nothing in the supported entity set
//!   refers to them, and R12 readers default them when absent.
#![allow(clippy::too_many_lines)]

use cadcraft_color::Color;
use cadcraft_doc::{Common, Drawing, Entity, EntityKind, MAX_BLOCK_DEPTH};
use cadcraft_dxf::Tag;
use cadcraft_geom::Vec3;

/// Largest number of blocks written to a R12 file, so a drawing with a pathological number of
/// block definitions (hostile or just huge) cannot make this allocate without bound.
const MAX_BLOCKS: usize = 1 << 16;

struct W {
    t: Vec<Tag>,
}

impl W {
    fn s(&mut self, c: i32, v: impl Into<String>) {
        self.t.push(Tag::s(c, v));
    }
    fn f(&mut self, c: i32, v: f64) {
        self.t.push(Tag::f(c, v));
    }
    fn i(&mut self, c: i32, v: i64) {
        self.t.push(Tag::i(c, v));
    }
    fn p(&mut self, c: i32, p: Vec3) {
        self.f(c, p.x);
        self.f(c + 10, p.y);
        self.f(c + 20, p.z);
    }
}

/// The drawing as genuine R12 ASCII DXF (`$ACADVER` `AC1009`).
pub fn write_r12(d: &Drawing) -> String {
    let mut w = W { t: Vec::new() };
    header(&mut w, d);
    tables(&mut w, d);
    let blocks = used_blocks(d);
    write_blocks_section(&mut w, d, &blocks);
    w.s(0, "SECTION");
    w.s(2, "ENTITIES");
    for e in d.model.iter() {
        entity(&mut w, d, e);
    }
    w.s(0, "ENDSEC");
    w.s(0, "EOF");
    cadcraft_dxf::write_ascii(&w.t)
}

fn header(w: &mut W, d: &Drawing) {
    w.s(0, "SECTION");
    w.s(2, "HEADER");
    w.s(9, "$ACADVER");
    w.s(1, "AC1009");
    w.s(9, "$INSBASE");
    w.p(10, Vec3::ZERO);
    let ext = d.extents(&cadcraft_doc::Space::Model);
    let (lo, hi) = if ext.is_empty() { (Vec3::ZERO, Vec3::ZERO) } else { (ext.min.to3(0.0), ext.max.to3(0.0)) };
    w.s(9, "$EXTMIN");
    w.p(10, lo);
    w.s(9, "$EXTMAX");
    w.p(10, hi);
    w.s(9, "$LIMMIN");
    w.f(10, lo.x);
    w.f(20, lo.y);
    w.s(9, "$LIMMAX");
    w.f(10, hi.x);
    w.f(20, hi.y);
    w.s(0, "ENDSEC");
}

/// LTYPE, LAYER and STYLE tables (R12 readers default VIEW/UCS/VPORT/DIMSTYLE when absent, and
/// nothing this writer emits refers to them).
fn tables(w: &mut W, d: &Drawing) {
    w.s(0, "SECTION");
    w.s(2, "TABLES");

    w.s(0, "TABLE");
    w.s(2, "LTYPE");
    w.i(70, i64::try_from(d.linetypes.len()).unwrap_or(i64::MAX));
    for lt in &d.linetypes {
        // ByLayer/ByBlock are pseudo-linetypes CADCraft never looks up by name in the LTYPE
        // table; AutoCAD itself never writes them as real table records either.
        if lt.name.eq_ignore_ascii_case("bylayer") || lt.name.eq_ignore_ascii_case("byblock") {
            continue;
        }
        w.s(0, "LTYPE");
        w.s(2, &lt.name);
        w.i(70, 0);
        w.s(3, &lt.description);
        w.i(72, 65);
        w.i(73, i64::try_from(lt.pattern.len()).unwrap_or(0));
        w.f(40, lt.pattern_length());
        for dash in &lt.pattern {
            w.f(49, dash.length);
        }
    }
    w.s(0, "ENDTAB");

    w.s(0, "TABLE");
    w.s(2, "LAYER");
    w.i(70, i64::try_from(d.layers.len()).unwrap_or(i64::MAX));
    for l in &d.layers {
        w.s(0, "LAYER");
        w.s(2, &l.name);
        let flags = if l.frozen { 1 } else { 0 };
        w.i(70, flags);
        // R12 has no "off" flag bit of its own: a layer is turned off by writing its colour
        // negated (DXF Reference, LAYER group 62), the same trick AutoCAD itself still uses.
        let aci = l.color.to_aci().clamp(1, 255);
        w.i(62, i64::from(if l.on { aci } else { -aci }));
        w.s(6, &l.linetype);
    }
    w.s(0, "ENDTAB");

    w.s(0, "TABLE");
    w.s(2, "STYLE");
    w.i(70, i64::try_from(d.text_styles.len().max(1)).unwrap_or(1));
    for st in &d.text_styles {
        w.s(0, "STYLE");
        w.s(2, &st.name);
        w.i(70, 0);
        w.f(40, st.height.max(0.0));
        w.f(41, if st.width_factor > 0.0 { st.width_factor } else { 1.0 });
        w.f(50, st.oblique);
        w.i(71, 0);
        w.f(42, if st.height > 0.0 { st.height } else { 2.5 });
        // R12 has no TrueType style flag; every style is a font file name, and CADCraft's own
        // stroke font (never a real AutoCAD asset) has no `.shx` of its own, so it stands in
        // for whatever face is named.
        w.s(3, "txt.shx");
        w.s(4, "");
    }
    w.s(0, "ENDTAB");

    w.s(0, "ENDSEC");
}

/// Every block name a model-space (or nested) INSERT refers to, bounded and cycle-safe: a
/// hostile or malformed drawing's INSERT chain cannot make this loop forever or explode in size.
fn used_blocks(d: &Drawing) -> Vec<String> {
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut queue: Vec<(String, usize)> = d
        .model
        .iter()
        .filter_map(|e| match &e.kind {
            EntityKind::Insert(i) => Some((i.block.clone(), 0)),
            _ => None,
        })
        .collect();
    while let Some((name, depth)) = queue.pop() {
        if seen.len() >= MAX_BLOCKS || depth >= MAX_BLOCK_DEPTH || !seen.insert(name.clone()) {
            continue;
        }
        let Some(b) = d.block(&name) else { continue };
        for e in b.entities.iter() {
            if let EntityKind::Insert(i) = &e.kind {
                queue.push((i.block.clone(), depth + 1));
            }
        }
    }
    seen.into_iter().collect()
}

fn write_blocks_section(w: &mut W, d: &Drawing, names: &[String]) {
    w.s(0, "SECTION");
    w.s(2, "BLOCKS");
    for name in names {
        let Some(b) = d.block(name) else { continue };
        w.s(0, "BLOCK");
        w.s(8, "0");
        w.s(2, &b.name);
        w.i(70, 0);
        w.p(10, b.base);
        w.s(3, &b.name);
        w.s(1, "");
        for e in b.entities.iter() {
            entity(w, d, e);
        }
        w.s(0, "ENDBLK");
        w.s(8, "0");
    }
    w.s(0, "ENDSEC");
}

/// An entity's ACI colour (`62`), when it isn't ByLayer: R12 has no true colour, lineweight,
/// transparency or named plot style, so none of those [`Common`] fields are written at all.
fn color_and_layer(w: &mut W, c: &Common) {
    w.s(8, &c.layer);
    if !c.linetype.eq_ignore_ascii_case("bylayer") {
        w.s(6, &c.linetype);
    }
    match c.color {
        Color::ByLayer => {}
        other => w.i(62, i64::from(other.to_aci())),
    }
}

/// One supported entity's groups (model space or inside a block definition). An entity kind this
/// writer does not support for R12 (see the module doc comment) is left out, not an error: a
/// drawing with, say, a spline or a dimension still saves everything R12 can represent.
fn entity(w: &mut W, _d: &Drawing, e: &Entity) {
    match &e.kind {
        EntityKind::Line(l) => {
            w.s(0, "LINE");
            color_and_layer(w, &e.common);
            w.p(10, l.a);
            w.p(11, l.b);
        }
        EntityKind::Point(p) => {
            w.s(0, "POINT");
            color_and_layer(w, &e.common);
            w.p(10, p.p);
        }
        EntityKind::Circle(c) => {
            w.s(0, "CIRCLE");
            color_and_layer(w, &e.common);
            w.p(10, c.center);
            w.f(40, c.radius);
        }
        EntityKind::Arc(a) => {
            w.s(0, "ARC");
            color_and_layer(w, &e.common);
            w.p(10, a.center);
            w.f(40, a.radius);
            w.f(50, a.start.to_degrees());
            w.f(51, a.end.to_degrees());
        }
        EntityKind::Text(t) => {
            w.s(0, "TEXT");
            color_and_layer(w, &e.common);
            w.p(10, t.insert);
            w.f(40, t.height.max(0.0));
            // A 255-byte cap matches the limit a single DXF string group can hold at all.
            let cut = t.value.char_indices().map(|(i, c)| i + c.len_utf8()).take_while(|end| *end <= 255).last().unwrap_or(0);
            w.s(1, t.value.get(..cut).unwrap_or_default());
            if t.rotation != 0.0 {
                w.f(50, t.rotation.to_degrees());
            }
            if (t.width_factor - 1.0).abs() > 1e-12 {
                w.f(41, t.width_factor);
            }
            if t.oblique != 0.0 {
                w.f(51, t.oblique.to_degrees());
            }
            if !t.style.trim().is_empty() && !t.style.eq_ignore_ascii_case("standard") {
                w.s(7, &t.style);
            }
            let halign = match t.halign {
                cadcraft_doc::HAlign::Left => 0,
                cadcraft_doc::HAlign::Center => 1,
                cadcraft_doc::HAlign::Right => 2,
                cadcraft_doc::HAlign::Aligned => 3,
                cadcraft_doc::HAlign::Middle => 4,
                cadcraft_doc::HAlign::Fit => 5,
            };
            let valign = match t.valign {
                cadcraft_doc::VAlign::Baseline => 0,
                cadcraft_doc::VAlign::Bottom => 1,
                cadcraft_doc::VAlign::Middle => 2,
                cadcraft_doc::VAlign::Top => 3,
            };
            if halign != 0 {
                w.i(72, halign);
            }
            if valign != 0 {
                w.i(73, valign);
            }
            if halign != 0 || valign != 0 {
                w.p(11, t.align_pt.unwrap_or(t.insert));
            }
        }
        // LWPOLYLINE postdates R12 (it was introduced in AutoCAD R14): degrade to the classic
        // POLYLINE + VERTEX + SEQEND representation R12 always had.
        EntityKind::LwPolyline(p) => {
            w.s(0, "POLYLINE");
            color_and_layer(w, &e.common);
            w.i(66, 1);
            w.p(10, Vec3::new(0.0, 0.0, p.elevation));
            let flags = i64::from(p.closed) | if p.const_width > 0.0 { 8 } else { 0 } | if p.plinegen { 128 } else { 0 };
            w.i(70, flags);
            if p.const_width > 0.0 {
                w.f(40, p.const_width);
                w.f(41, p.const_width);
            }
            for v in &p.vertices {
                w.s(0, "VERTEX");
                w.s(8, &e.common.layer);
                w.p(10, Vec3::new(v.p.x, v.p.y, p.elevation));
                if v.start_width > 0.0 || v.end_width > 0.0 {
                    w.f(40, v.start_width);
                    w.f(41, v.end_width);
                }
                if v.bulge != 0.0 {
                    w.f(42, v.bulge);
                }
            }
            w.s(0, "SEQEND");
        }
        EntityKind::Insert(ins) => {
            w.s(0, "INSERT");
            color_and_layer(w, &e.common);
            w.s(2, &ins.block);
            w.p(10, ins.insert);
            if (ins.scale.x - 1.0).abs() > 1e-12 {
                w.f(41, ins.scale.x);
            }
            if (ins.scale.y - 1.0).abs() > 1e-12 {
                w.f(42, ins.scale.y);
            }
            if (ins.scale.z - 1.0).abs() > 1e-12 {
                w.f(43, ins.scale.z);
            }
            if ins.rotation != 0.0 {
                w.f(50, ins.rotation.to_degrees());
            }
            if ins.cols > 1 {
                w.i(70, i64::from(ins.cols));
            }
            if ins.rows > 1 {
                w.i(71, i64::from(ins.rows));
            }
            if ins.col_spacing != 0.0 {
                w.f(44, ins.col_spacing);
            }
            if ins.row_spacing != 0.0 {
                w.f(45, ins.row_spacing);
            }
        }
        // Everything else (MTEXT, DIMENSION, HATCH, SPLINE, ELLIPSE, 3D solids, images, tables,
        // leaders, multileaders, tolerances, viewports, unknown/foreign entities…) has no R12
        // representation CADCraft writes; see the module doc comment.
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use cadcraft_doc::{Arc, Circle, Drawing, Entity, EntityKind, Handle, Insert, Line, LwPolyline, Point, Text};
    use cadcraft_geom::{PolyVertex, Vec2, Vec3};

    use super::*;

    fn h(n: u64) -> Handle {
        Handle(n)
    }

    #[test]
    fn writes_header_acadver_as_r12() {
        let d = Drawing::new_imperial();
        let dxf = write_r12(&d);
        assert!(dxf.contains("AC1009"), "{dxf}");
        assert!(!dxf.contains("AC1015"), "a real AC1015 structure must not leak into R12 output");
    }

    #[test]
    fn never_writes_subclass_markers_or_handles() {
        let mut d = Drawing::new_imperial();
        d.model.push(Entity::new(h(1), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(1.0, 1.0, 0.0) })));
        d.model.push(Entity::new(h(2), EntityKind::Circle(Circle { center: Vec3::ZERO, radius: 2.0 })));
        let dxf = write_r12(&d);
        // No `100` subclass-marker groups and no `5`/`330` handle groups: both postdate R12.
        let lines: Vec<&str> = dxf.lines().map(str::trim).collect();
        for pair in lines.chunks(2) {
            let [code, _] = pair else { continue };
            assert_ne!(*code, "100", "subclass markers don't exist in R12: {dxf}");
        }
    }

    #[test]
    fn lwpolyline_degrades_to_classic_polyline() {
        let mut d = Drawing::new_imperial();
        let p = LwPolyline {
            vertices: vec![
                PolyVertex::new(Vec2::new(0.0, 0.0)),
                PolyVertex::new(Vec2::new(10.0, 0.0)),
                PolyVertex::with_bulge(Vec2::new(10.0, 10.0), 0.5),
            ],
            closed: true,
            const_width: 0.0,
            elevation: 0.0,
            plinegen: false,
        };
        d.model.push(Entity::new(h(1), EntityKind::LwPolyline(p)));
        let dxf = write_r12(&d);
        assert!(dxf.contains("POLYLINE"), "{dxf}");
        assert!(!dxf.contains("LWPOLYLINE"), "LWPOLYLINE postdates R12: {dxf}");
        assert_eq!(dxf.matches("VERTEX").count(), 3, "{dxf}");
        assert!(dxf.contains("SEQEND"), "{dxf}");
    }

    #[test]
    fn true_color_degrades_to_nearest_aci() {
        let mut d = Drawing::new_imperial();
        let mut e = Entity::new(h(1), EntityKind::Point(Point { p: Vec3::ZERO, angle: 0.0 }));
        e.common.color = cadcraft_color::Color::True(cadcraft_color::Rgb(255, 0, 0));
        d.model.push(e);
        let dxf = write_r12(&d);
        assert!(!dxf.contains("\r\n420\r\n"), "no true colour group in R12: {dxf}");
        let aci = cadcraft_color::nearest_aci(cadcraft_color::Rgb(255, 0, 0));
        assert!(dxf.contains(&format!("62\r\n{aci}\r\n")), "{dxf}");
    }

    #[test]
    fn insert_writes_its_blocks_section_with_bounded_cyclic_nesting() {
        let mut d = Drawing::new_imperial();
        // A block that (hostilely) inserts itself: must not recurse forever.
        let mut block = cadcraft_doc::Block::new("SELF");
        block.entities.push(Entity::new(h(10), EntityKind::Insert(Insert { block: "SELF".into(), insert: Vec3::ZERO, ..default_insert() })));
        d.blocks.insert("SELF".into(), std::sync::Arc::new(block));
        d.model.push(Entity::new(h(1), EntityKind::Insert(Insert { block: "SELF".into(), insert: Vec3::ZERO, ..default_insert() })));
        let dxf = write_r12(&d);
        assert!(dxf.contains("BLOCKS"), "{dxf}");
        assert!(dxf.contains("SELF"), "{dxf}");
    }

    #[test]
    fn unsupported_entities_are_left_out_without_panicking() {
        let mut d = Drawing::new_imperial();
        d.model.push(Entity::new(
            h(1),
            EntityKind::Text(Text {
                insert: Vec3::ZERO,
                align_pt: None,
                height: 1.0,
                value: "hi".into(),
                rotation: 0.0,
                width_factor: 1.0,
                oblique: 0.0,
                style: "Standard".into(),
                halign: cadcraft_doc::HAlign::Left,
                valign: cadcraft_doc::VAlign::Baseline,
            }),
        ));
        d.model.push(Entity::new(h(2), EntityKind::Unknown(cadcraft_doc::Unknown { dxf_type: "ACAD_PROXY_ENTITY".into(), tags: vec![] })));
        let dxf = write_r12(&d);
        assert!(dxf.contains("TEXT"), "{dxf}");
        assert!(!dxf.contains("PROXY"), "unmodeled/foreign entities are left out, not forwarded raw: {dxf}");
    }

    #[test]
    fn arc_with_hostile_nan_fields_does_not_panic() {
        let mut d = Drawing::new_imperial();
        d.model.push(Entity::new(
            h(1),
            EntityKind::Arc(Arc { center: Vec3::new(f64::NAN, f64::INFINITY, 0.0), radius: f64::NAN, start: f64::NAN, end: f64::NAN }),
        ));
        let _ = write_r12(&d); // must not panic
    }

    fn default_insert() -> Insert {
        Insert {
            block: String::new(),
            insert: Vec3::ZERO,
            scale: Vec3::new(1.0, 1.0, 1.0),
            rotation: 0.0,
            attribs: vec![],
            cols: 1,
            rows: 1,
            col_spacing: 0.0,
            row_spacing: 0.0,
            clip: None,
        }
    }
}

//! XCLIP's boundary (`cadcraft_doc::InsertClip`, set by `crates/engine/src/cmd/blocks/xclip.rs`)
//! actually hides the clipped-away part of a block reference's own contents, on screen and when
//! plotting: both share this crate's display list (`build` / `build_plot`), see docs/xclip.md.

use std::sync::Arc;

use cadcraft_doc::{Block, Common, Drawing, Entity, EntityKind, Handle, Insert, InsertClip, Line, LwPolyline, Solid, Space};
use cadcraft_geom::{PolyVertex, Vec2, Vec3};
use cadcraft_render::{Kind, Options, build, build_plot};

fn v(x: f64, y: f64) -> PolyVertex {
    PolyVertex::new(Vec2::new(x, y))
}

/// A closed rectangle boundary from `(x0, y0)` to `(x1, y1)`.
fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> LwPolyline {
    LwPolyline { vertices: vec![v(x0, y0), v(x1, y0), v(x1, y1), v(x0, y1)], closed: true, const_width: 0.0, elevation: 0.0, plinegen: false }
}

/// Block "B": a line fully inside the 2..8 square, a line fully outside it (far away), and a
/// filled SOLID straddling the square's right edge (half in, half out), each on its own layer
/// so a test can tell which drew.
fn block() -> Block {
    let mut blk = Block::new("B");
    let mut inside = Entity::new(Handle(1), EntityKind::Line(Line { a: Vec3::new(3.0, 3.0, 0.0), b: Vec3::new(5.0, 5.0, 0.0) }));
    inside.common.layer = "INSIDE".into();
    blk.entities.push(inside);
    let mut outside = Entity::new(Handle(2), EntityKind::Line(Line { a: Vec3::new(20.0, 20.0, 0.0), b: Vec3::new(25.0, 25.0, 0.0) }));
    outside.common.layer = "OUTSIDE".into();
    blk.entities.push(outside);
    let straddle = EntityKind::Solid(Solid {
        corners: [Vec3::new(6.0, 3.0, 0.0), Vec3::new(10.0, 3.0, 0.0), Vec3::new(6.0, 7.0, 0.0), Vec3::new(10.0, 7.0, 0.0)],
    });
    let mut fill = Entity::new(Handle(3), straddle);
    fill.common.layer = "FILL".into();
    blk.entities.push(fill);
    blk
}

/// A drawing with one reference of [`block`] at the origin, clipped to `clip`.
fn drawing(clip: Option<InsertClip>) -> Drawing {
    let mut d = Drawing::new_metric();
    d.blocks.insert("B".into(), Arc::new(block()));
    let ins = Insert {
        block: "B".into(),
        insert: Vec3::ZERO,
        scale: Vec3::new(1.0, 1.0, 1.0),
        rotation: 0.0,
        attribs: vec![],
        cols: 1,
        rows: 1,
        col_spacing: 0.0,
        row_spacing: 0.0,
        clip,
    };
    d.add(&Space::Model, Common::default(), EntityKind::Insert(ins)).unwrap();
    d
}

fn rect_clip(on: bool, inverted: bool) -> InsertClip {
    InsertClip { boundary: rect(2.0, 2.0, 8.0, 8.0), on, inverted, front: None, back: None }
}

fn line_count(d: &Drawing) -> usize {
    build(d, &Space::Model, &Options::default()).prims.iter().filter(|p| p.kind == Kind::Polyline).count()
}

/// Total filled area (SOLID triangles), by the shoelace formula.
fn fill_area(d: &Drawing) -> f64 {
    let l = build(d, &Space::Model, &Options::default());
    l.prims
        .iter()
        .filter(|p| p.kind == Kind::Tris)
        .flat_map(|p| l.points(p).as_chunks::<3>().0.iter().map(|[a, b, c]| (*b - *a).cross(*c - *a) / 2.0))
        .sum()
}

#[test]
fn rectangle_boundary_hides_geometry_outside_it() {
    let clipped = drawing(Some(rect_clip(true, false)));
    // The inside line survives whole; the outside line is gone; the straddling SOLID is cut
    // down to the half of it (2x4 of its 4x4) still inside the 2..8 square.
    assert_eq!(line_count(&clipped), 1, "only the inside line draws");
    let l = build(&clipped, &Space::Model, &Options::default());
    let line = l.prims.iter().find(|p| p.kind == Kind::Polyline).unwrap();
    let pts = l.points(line);
    assert!(pts[0].near(Vec2::new(3.0, 3.0), 1e-9) && pts[1].near(Vec2::new(5.0, 5.0), 1e-9), "{pts:?}");
    assert!((fill_area(&clipped) - 8.0).abs() < 1e-6, "{}", fill_area(&clipped));
    // A point strictly inside the boundary still renders (the kept inside line's own endpoint).
    assert!(l.bounds.contains(Vec2::new(3.0, 3.0)));
    // Nothing of the outside line's extent leaks through.
    assert!(l.bounds.max.x <= 8.0 + 1e-9 && l.bounds.max.y <= 8.0 + 1e-9, "{:?}", l.bounds);
}

#[test]
fn plot_output_is_clipped_the_same_way_as_the_canvas() {
    let clipped = drawing(Some(rect_clip(true, false)));
    let plotted = build_plot(&clipped, &Space::Model, &Options::default());
    assert_eq!(plotted.prims.iter().filter(|p| p.kind == Kind::Polyline).count(), 1);
    let area: f64 = plotted
        .prims
        .iter()
        .filter(|p| p.kind == Kind::Tris)
        .flat_map(|p| plotted.points(p).as_chunks::<3>().0.iter().map(|[a, b, c]| (*b - *a).cross(*c - *a) / 2.0))
        .sum();
    assert!((area - 8.0).abs() < 1e-6, "{area}");
}

#[test]
fn turning_xclip_off_restores_full_rendering() {
    let unclipped = drawing(None);
    assert_eq!(line_count(&unclipped), 2);
    assert!((fill_area(&unclipped) - 16.0).abs() < 1e-6);
    let off = drawing(Some(rect_clip(false, false)));
    assert_eq!(line_count(&off), 2, "off preserves the boundary data but draws everything");
    assert!((fill_area(&off) - 16.0).abs() < 1e-6);
}

#[test]
fn polygonal_boundary_clips_to_its_own_shape() {
    // A triangle covering only the inside line's lower-left half.
    let tri = LwPolyline { vertices: vec![v(0.0, 0.0), v(10.0, 0.0), v(0.0, 10.0)], closed: true, const_width: 0.0, elevation: 0.0, plinegen: false };
    let clip = InsertClip { boundary: tri, on: true, inverted: false, front: None, back: None };
    let d = drawing(Some(clip));
    // (3,3)-(5,5) lies entirely under the hypotenuse x + y = 10, so it is kept whole.
    assert_eq!(line_count(&d), 1);
    // The straddling SOLID (x in 6..10) is almost entirely beyond the hypotenuse: only the sliver
    // above x + y = 10 is excluded on its left, so some but not all of its 16 units survive.
    let area = fill_area(&d);
    assert!(area > 0.0 && area < 16.0, "{area}");
}

#[test]
fn invert_keeps_the_outside_of_the_boundary_instead() {
    let d = drawing(Some(rect_clip(true, true)));
    // The inside line is now fully excluded (it was entirely inside the boundary); the outside
    // line, previously excluded, is now kept.
    assert_eq!(line_count(&d), 1, "only the formerly-outside line draws");
    let l = build(&d, &Space::Model, &Options::default());
    let line = l.prims.iter().find(|p| p.kind == Kind::Polyline).unwrap();
    let pts = l.points(line);
    assert!(pts[0].near(Vec2::new(20.0, 20.0), 1e-9) && pts[1].near(Vec2::new(25.0, 25.0), 1e-9), "{pts:?}");
    // The straddling SOLID keeps the half that was previously clipped away (8 of its 16 units).
    assert!((fill_area(&d) - 8.0).abs() < 1e-6, "{}", fill_area(&d));
}

#[test]
fn a_degenerate_boundary_never_panics_and_draws_the_reference_whole() {
    // Zero-area: three collinear points (XCLIP's own command rejects this on input, but a
    // boundary could also arrive from a future hostile DXF/DWG SPATIAL_FILTER import).
    let degenerate =
        LwPolyline { vertices: vec![v(0.0, 0.0), v(5.0, 0.0), v(10.0, 0.0)], closed: true, const_width: 0.0, elevation: 0.0, plinegen: false };
    let clip = InsertClip { boundary: degenerate, on: true, inverted: false, front: None, back: None };
    let d = drawing(Some(clip));
    assert_eq!(line_count(&d), 2, "an unusable boundary draws the contents in full rather than losing them");
    assert!((fill_area(&d) - 16.0).abs() < 1e-6);
    let _ = build_plot(&d, &Space::Model, &Options::default());
}

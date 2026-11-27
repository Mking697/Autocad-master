//! The count grips of an associative array, as AutoCAD shows them on a selected array.
//! Rectangular: an arrow at the far end of the column axis (drag it to change `cols`) and one at
//! the far end of the row axis (drag it to change `rows`). Polar: an Items grip at the last item
//! (dragging it changes the item count, keeping the fill angle; AutoCAD's "this grip changes the
//! number of items if the array doesn't fill 360°") and, once there are 3 or more items, an
//! Angle Between grip at the second item (dragging it changes the angle between items, keeping
//! the count, recomputing the fill angle) — positions match "To Work With Polar Arrays" in
//! Autodesk's AutoCAD help (the Angle Between grip sits on the array's second item).

use cadcraft_geom::Vec2;
use serde::Serialize;

use super::{ArrayKind, PolarArray, RectArray};
use crate::dynblock::GripShape;
use crate::{Drawing, EntityKind, Handle};

/// Which quantity an associative array's count/angle grip changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ArrayGripWhich {
    /// The grip at the far end of the column axis: dragging it changes `cols`.
    Cols,
    /// The grip at the far end of the row axis: dragging it changes `rows`.
    Rows,
    /// The grip at the last item of a polar array: dragging it changes `items`, the fill angle
    /// kept.
    Items,
    /// The grip at the second item of a polar array (3+ items): dragging it changes the angle
    /// between items, the item count kept (the fill angle is recomputed from it).
    Angle,
}

impl ArrayGripWhich {
    pub fn name(self) -> &'static str {
        match self {
            ArrayGripWhich::Cols => "cols",
            ArrayGripWhich::Rows => "rows",
            ArrayGripWhich::Items => "items",
            ArrayGripWhich::Angle => "angle",
        }
    }
    pub fn from_name(s: &str) -> Option<ArrayGripWhich> {
        match s.trim().to_ascii_lowercase().as_str() {
            "cols" => Some(ArrayGripWhich::Cols),
            "rows" => Some(ArrayGripWhich::Rows),
            "items" => Some(ArrayGripWhich::Items),
            "angle" => Some(ArrayGripWhich::Angle),
            _ => None,
        }
    }
}

/// One count/angle grip of an associative array reference.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArrayGrip {
    pub which: ArrayGripWhich,
    /// Always [`GripShape::Linear`] (an arrow): kept so the UI can share the dynamic block
    /// grips' outline-drawing code.
    pub shape: GripShape,
    /// Where it is, in drawing (world) coordinates.
    #[serde(serialize_with = "point")]
    pub at: Vec2,
    /// The way the arrow points (a unit vector in drawing coordinates).
    #[serde(serialize_with = "point")]
    pub dir: Vec2,
    /// The current column (or row, or polar item) count, for the UI to show while dragging.
    pub cols_or_rows_count: u32,
    /// The spacing along this grip's axis: drawing units for a rectangular array's Cols/Rows
    /// grip, degrees (the current angle between items) for a polar array's Items/Angle grip.
    pub spacing: f64,
}

fn point<S: serde::Serializer>(p: &Vec2, s: S) -> Result<S::Ok, S::Error> {
    [p.x, p.y].serialize(s)
}

/// The count grips of associative array reference `insert`: `cols` and `rows` for a rectangular
/// array, placed at the far end of each axis in world coordinates; `items` (always) and `angle`
/// (once there are 3+ items) for a polar array; an empty list for any other object (including a
/// non-associative block reference).
pub fn array_grips(d: &Drawing, insert: Handle) -> Vec<ArrayGrip> {
    let Some(e) = d.entity(insert) else { return Vec::new() };
    let Some(p) = d.array_params(e) else { return Vec::new() };
    let Some(EntityKind::Insert(ins)) = d.entity(insert).map(|e| &e.kind) else { return Vec::new() };
    let Some(blk) = d.block(&ins.block) else { return Vec::new() };
    let xf = ins.transform(blk.base.xy());
    let world = |v: Vec2| xf.apply(v);
    let toward = |u: Vec2| {
        let v = xf.apply(u) - xf.apply(Vec2::ZERO);
        if v.len() > 1e-12 { v * (1.0 / v.len()) } else { Vec2::ZERO }
    };
    let mut out = match &p.kind {
        ArrayKind::Rect(r) => {
            let RectArray { cols, rows, col_spacing, row_spacing, axis_angle, .. } = *r;
            let (ux, uy) = (Vec2::from_angle(axis_angle), Vec2::from_angle(axis_angle).perp());
            let cols_at = ux * (col_spacing * f64::from(cols.saturating_sub(1)));
            let rows_at = uy * (row_spacing * f64::from(rows.saturating_sub(1)));
            vec![
                ArrayGrip {
                    which: ArrayGripWhich::Cols,
                    shape: GripShape::Linear,
                    at: world(cols_at),
                    dir: toward(ux),
                    cols_or_rows_count: cols,
                    spacing: col_spacing,
                },
                ArrayGrip {
                    which: ArrayGripWhich::Rows,
                    shape: GripShape::Linear,
                    at: world(rows_at),
                    dir: toward(uy),
                    cols_or_rows_count: rows,
                    spacing: row_spacing,
                },
            ]
        }
        ArrayKind::Polar(k) => polar_grips(&p, k, &world, &toward),
    };
    out.retain(|g| g.at.is_finite() && g.dir.is_finite());
    out
}

/// The Items grip (last item, always) and Angle Between grip (second item, once there are 3+
/// items) of polar array `k`, as the Autodesk help places them ("To Work With Polar Arrays": the
/// Angle Between grip is on the second item, shown only for 3 or more items).
fn polar_grips(p: &super::ArrayParams, k: &PolarArray, world: &impl Fn(Vec2) -> Vec2, toward: &impl Fn(Vec2) -> Vec2) -> Vec<ArrayGrip> {
    let items = p.all_items().unwrap_or_default();
    let at_i = |i: u32| items.iter().find(|it| it.i == i && it.j == 0 && it.level == 0).map(|it| it.insert.xy());
    // The tangent at a point around the circle, in the direction of travel (the fill's sign).
    let tangent = |pt: Vec2| -> Vec2 {
        let r = pt - k.center;
        if r.len() > 1e-9 {
            let t = r.perp().normalized();
            if k.fill < 0.0 { Vec2::ZERO - t } else { t }
        } else {
            Vec2::ZERO
        }
    };
    let step = k.angle_between();
    let mut out = Vec::new();
    if let Some(last) = at_i(k.items.saturating_sub(1)) {
        out.push(ArrayGrip {
            which: ArrayGripWhich::Items,
            shape: GripShape::Linear,
            at: world(last),
            dir: toward(tangent(last)),
            cols_or_rows_count: k.items,
            spacing: step,
        });
    }
    if k.items >= 3
        && let Some(second) = at_i(1)
    {
        out.push(ArrayGrip {
            which: ArrayGripWhich::Angle,
            shape: GripShape::Linear,
            at: world(second),
            dir: toward(tangent(second)),
            cols_or_rows_count: k.items,
            spacing: step,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{Block, Common, Entity, Insert};

    /// A model space with a 3×2 rectangular associative array at `base`, items 2 apart on both
    /// axes, with the array block named `*U1` and its source `*U2`.
    fn drawing_with_array(base: Vec2) -> (Drawing, Handle) {
        let mut d = Drawing::default();
        let mut src = Block::new("*U2");
        src.entities.push(Entity {
            handle: Handle(1),
            common: Common::default(),
            kind: EntityKind::Point(crate::Point { p: cadcraft_geom::Vec3::ZERO, angle: 0.0 }),
        });
        d.blocks.insert("*U2".into(), Arc::new(src));
        let p = super::super::ArrayParams {
            source: "*U2".into(),
            kind: ArrayKind::Rect(RectArray { cols: 3, rows: 2, col_spacing: 2.0, row_spacing: 3.0, axis_angle: 0.0, row_elevation: 0.0 }),
            levels: 1,
            level_spacing: 0.0,
            replaced: Vec::new(),
            excluded: Vec::new(),
            editing: None,
        };
        let mut blk = Block::new("*U1");
        let items = p.item_entities(&mut d).unwrap_or_default();
        for e in items {
            blk.entities.push(e);
        }
        blk.array = Some(p);
        d.blocks.insert("*U1".into(), Arc::new(blk));
        let h = Handle(100);
        d.model.push(Entity {
            handle: h,
            common: Common::default(),
            kind: EntityKind::Insert(Insert {
                block: "*U1".into(),
                insert: base.to3(0.0),
                scale: cadcraft_geom::Vec3::new(1.0, 1.0, 1.0),
                rotation: 0.0,
                attribs: Vec::new(),
                cols: 1,
                rows: 1,
                col_spacing: 0.0,
                row_spacing: 0.0,
                clip: None,
            }),
        });
        (d, h)
    }

    /// A model space with a polar associative array at `base`, `items` items around `center`
    /// (local, relative to the base point) with fill angle `fill`, with the array block named
    /// `*U1` and its source `*U2`.
    fn drawing_with_polar_array(base: Vec2, center: Vec2, items: u32, fill: f64) -> (Drawing, Handle) {
        let mut d = Drawing::default();
        let mut src = Block::new("*U2");
        src.entities.push(Entity {
            handle: Handle(1),
            common: Common::default(),
            kind: EntityKind::Point(crate::Point { p: cadcraft_geom::Vec3::ZERO, angle: 0.0 }),
        });
        d.blocks.insert("*U2".into(), Arc::new(src));
        let p = super::super::ArrayParams {
            source: "*U2".into(),
            kind: ArrayKind::Polar(PolarArray { center, items, fill, rotate: true, rows: 1, row_spacing: 0.0, row_elevation: 0.0 }),
            levels: 1,
            level_spacing: 0.0,
            replaced: Vec::new(),
            excluded: Vec::new(),
            editing: None,
        };
        let mut blk = Block::new("*U1");
        let its = p.item_entities(&mut d).unwrap_or_default();
        for e in its {
            blk.entities.push(e);
        }
        blk.array = Some(p);
        d.blocks.insert("*U1".into(), Arc::new(blk));
        let h = Handle(100);
        d.model.push(Entity {
            handle: h,
            common: Common::default(),
            kind: EntityKind::Insert(Insert {
                block: "*U1".into(),
                insert: base.to3(0.0),
                scale: cadcraft_geom::Vec3::new(1.0, 1.0, 1.0),
                rotation: 0.0,
                attribs: Vec::new(),
                cols: 1,
                rows: 1,
                col_spacing: 0.0,
                row_spacing: 0.0,
                clip: None,
            }),
        });
        (d, h)
    }

    #[test]
    fn two_grips_at_the_far_ends_of_each_axis() {
        let (d, h) = drawing_with_array(Vec2::new(10.0, 0.0));
        let gs = array_grips(&d, h);
        assert_eq!(gs.len(), 2);
        assert_eq!(gs[0].which, ArrayGripWhich::Cols);
        assert!(gs[0].at.near(Vec2::new(14.0, 0.0), 1e-9), "{:?}", gs[0].at);
        assert_eq!(gs[0].cols_or_rows_count, 3);
        assert_eq!(gs[1].which, ArrayGripWhich::Rows);
        assert!(gs[1].at.near(Vec2::new(10.0, 3.0), 1e-9), "{:?}", gs[1].at);
        assert_eq!(gs[1].cols_or_rows_count, 2);
    }

    #[test]
    fn empty_for_non_arrays() {
        let (mut d, h) = drawing_with_array(Vec2::ZERO);
        assert!(!array_grips(&d, h).is_empty());
        // A plain (non-array) block reference: no grips.
        let plain = Handle(200);
        d.model.push(Entity {
            handle: plain,
            common: Common::default(),
            kind: EntityKind::Insert(Insert {
                block: "*U2".into(),
                insert: cadcraft_geom::Vec3::ZERO,
                scale: cadcraft_geom::Vec3::new(1.0, 1.0, 1.0),
                rotation: 0.0,
                attribs: Vec::new(),
                cols: 1,
                rows: 1,
                col_spacing: 0.0,
                row_spacing: 0.0,
                clip: None,
            }),
        });
        assert!(array_grips(&d, plain).is_empty());
    }

    #[test]
    fn polar_items_grip_always_on_the_last_item() {
        // 4 items, full circle, base point (1, 0) relative to the array's own origin (local
        // centre at (0, 0) because `base` is the insert point here, not the centre offset).
        let (d, h) = drawing_with_polar_array(Vec2::new(10.0, 0.0), Vec2::new(-1.0, 0.0), 4, 360.0);
        let gs = array_grips(&d, h);
        // 4 items, full circle: both the Items grip and the Angle Between grip are shown.
        assert_eq!(gs.len(), 2);
        let items_grip = gs.iter().find(|g| g.which == ArrayGripWhich::Items).unwrap();
        assert_eq!(items_grip.cols_or_rows_count, 4);
        // Centre at (-1, 0) relative to the base point, radius 1: item 3 (three quarter-turns
        // round) lands at local (-1, -1), i.e. (9, -1) once the base point is added back.
        assert!(items_grip.at.near(Vec2::new(9.0, -1.0), 1e-6), "{:?}", items_grip.at);
        let angle_grip = gs.iter().find(|g| g.which == ArrayGripWhich::Angle).unwrap();
        assert!((angle_grip.spacing - 90.0).abs() < 1e-6, "{:?}", angle_grip.spacing);
    }

    #[test]
    fn polar_count_grips_stay_at_deleted_grid_positions() {
        let (mut d, h) = drawing_with_polar_array(Vec2::ZERO, Vec2::new(-10.0, 0.0), 4, 360.0);
        let before = array_grips(&d, h);
        let mut p = d.array_params(d.entity(h).unwrap()).unwrap();
        p.exclude((1, 0, 0));
        p.exclude((3, 0, 0));
        let mut block = Block::new("*U1");
        for entity in p.item_entities(&mut d).unwrap() {
            block.entities.push(entity);
        }
        block.array = Some(p);
        d.blocks.insert("*U1".into(), Arc::new(block));
        assert_eq!(array_grips(&d, h), before);
    }

    #[test]
    fn polar_angle_grip_absent_under_three_items() {
        let (d, h) = drawing_with_polar_array(Vec2::ZERO, Vec2::new(-1.0, 0.0), 2, 180.0);
        let gs = array_grips(&d, h);
        assert_eq!(gs.len(), 1);
        assert_eq!(gs[0].which, ArrayGripWhich::Items);
    }

    #[test]
    fn polar_grips_never_panic_at_zero_radius() {
        // The array's centre coincides with the base point (radius 0): a degenerate but valid
        // array; grips must still be computed without panicking (dir falls back to zero).
        let (d, h) = drawing_with_polar_array(Vec2::ZERO, Vec2::ZERO, 4, 360.0);
        let gs = array_grips(&d, h);
        assert!(gs.iter().all(|g| g.at.is_finite() && g.dir.is_finite()));
    }
}

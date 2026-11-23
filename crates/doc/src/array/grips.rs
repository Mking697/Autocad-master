//! The count grips of an associative rectangular array, as AutoCAD shows them on a selected
//! array: an arrow at the far end of the column axis (drag it to change `cols`) and one at the
//! far end of the row axis (drag it to change `rows`). Polar arrays have no count grips here
//! (AutoCAD's polar count/angle grips are a separate, not yet implemented, shape); this module
//! covers the rectangular case only.

use cadcraft_geom::Vec2;
use serde::Serialize;

use super::{ArrayKind, RectArray};
use crate::dynblock::GripShape;
use crate::{Drawing, EntityKind, Handle};

/// Which direction a rectangular array's count grip changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ArrayGripWhich {
    /// The grip at the far end of the column axis: dragging it changes `cols`.
    Cols,
    /// The grip at the far end of the row axis: dragging it changes `rows`.
    Rows,
}

impl ArrayGripWhich {
    pub fn name(self) -> &'static str {
        match self {
            ArrayGripWhich::Cols => "cols",
            ArrayGripWhich::Rows => "rows",
        }
    }
    pub fn from_name(s: &str) -> Option<ArrayGripWhich> {
        match s.trim().to_ascii_lowercase().as_str() {
            "cols" => Some(ArrayGripWhich::Cols),
            "rows" => Some(ArrayGripWhich::Rows),
            _ => None,
        }
    }
}

/// One count grip of a rectangular associative array reference.
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
    /// The current column (or row) count, for the UI to show while dragging.
    pub cols_or_rows_count: u32,
    /// The spacing along this grip's axis, in the array's own units.
    pub spacing: f64,
}

fn point<S: serde::Serializer>(p: &Vec2, s: S) -> Result<S::Ok, S::Error> {
    [p.x, p.y].serialize(s)
}

/// The count grips of associative array reference `insert`: `cols` and `rows` for a rectangular
/// array, placed at the far end of each axis in world coordinates; an empty list for a polar
/// array or any other object (including a non-associative block reference).
pub fn array_grips(d: &Drawing, insert: Handle) -> Vec<ArrayGrip> {
    let Some(e) = d.entity(insert) else { return Vec::new() };
    let Some(p) = d.array_params(e) else { return Vec::new() };
    let ArrayKind::Rect(r) = &p.kind else { return Vec::new() };
    let Some(EntityKind::Insert(ins)) = d.entity(insert).map(|e| &e.kind) else { return Vec::new() };
    let Some(blk) = d.block(&ins.block) else { return Vec::new() };
    let xf = ins.transform(blk.base.xy());
    let world = |v: Vec2| xf.apply(v);
    let toward = |u: Vec2| {
        let v = xf.apply(u) - xf.apply(Vec2::ZERO);
        if v.len() > 1e-12 { v * (1.0 / v.len()) } else { Vec2::ZERO }
    };
    let RectArray { cols, rows, col_spacing, row_spacing, axis_angle, .. } = *r;
    let (ux, uy) = (Vec2::from_angle(axis_angle), Vec2::from_angle(axis_angle).perp());
    let cols_at = ux * (col_spacing * f64::from(cols.saturating_sub(1)));
    let rows_at = uy * (row_spacing * f64::from(rows.saturating_sub(1)));
    let mut out = vec![
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
    ];
    out.retain(|g| g.at.is_finite() && g.dir.is_finite());
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
    fn empty_for_non_arrays_and_polar_arrays() {
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
        // A polar array: no count grips here (out of scope).
        if let Some(b) = d.blocks.get_mut("*U1") {
            let b = Arc::make_mut(b);
            if let Some(a) = &mut b.array {
                a.kind = ArrayKind::Polar(super::super::PolarArray {
                    center: Vec2::ZERO,
                    items: 4,
                    fill: 360.0,
                    rotate: true,
                    rows: 1,
                    row_spacing: 0.0,
                    row_elevation: 0.0,
                });
            }
        }
        assert!(array_grips(&d, h).is_empty());
    }
}

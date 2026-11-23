//! Count/angle grips of an associative array: `array.reference.grips` lists the cols/rows arrows
//! of a rectangular array and the items/angle arrows of a polar array; `array.reference.grip`
//! drags one (one undo step); [`Session::start_array_grip`] starts the drag the UI's click on a
//! grip begins, with the array regenerated under the cursor as its preview.
//!
//! Polar semantics match AutoCAD's ("To Work With Polar Arrays", Autodesk help): the Items grip
//! (at the last item) changes the item count while the fill angle is kept — the per-item angle
//! this implies is recomputed, which is why it can end up different from before the drag; the
//! Angle Between grip (at the second item, 3+ items only) changes the angle between items while
//! the item count is kept, recomputing the fill angle from it (`fill_from_between`, the same
//! formula ARRAYEDIT's "Angle between" option and JSON `angleBetween` param use).

use cadcraft_doc::{ArrayGripWhich, ArrayKind, Drawing, EntityKind, Handle, MAX_ARRAY_ITEMS, array_grips};
use cadcraft_geom::Vec2;
use serde_json::{Value, json};

use super::edit::{array_of, fill_from_between, rebuild};
use crate::cmd::{CommandSpec, bad, point_param, str_param};
use crate::{EngineError, Input, Interactive, Prompt, Result, Running, Session, Step};

pub(super) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("array.reference.grips", "Array Count Grips", run_grips)
            .params(
                "{handle} → [{which: cols|rows|items|angle, shape: linear, at: [x,y], dir: [x,y], colsOrRowsCount, spacing}] (empty: not an associative array)",
            )
            .enabled(crate::cmd::has_doc)
            .noundo(),
        CommandSpec::new("array.reference.grip", "Drag Array Count Grip", run_grip)
            .params(
                "{handle, which: \"cols\"|\"rows\"|\"items\"|\"angle\", to: [x, y] (drawing coordinates)} → {handle}: cols/rows round to the nearest whole count along that axis; items rounds the polar item count keeping the fill angle; angle sets the polar angle between items (and its fill angle), keeping the item count; all clamped to at least 1 and at most MAXARRAY",
            )
            .enabled(crate::cmd::has_doc),
    ]
}

/// The count/angle grips of an associative array reference (an empty list for any other object).
pub(super) fn run_grips(s: &mut Session, p: &Value) -> Result<Value> {
    let h = super::super::curves::handle_param(p, "handle").ok_or_else(|| bad("array.reference.grips", "`handle` is required"))?;
    serde_json::to_value(array_grips(s.doc()?, h)).map_err(|e| bad("array.reference.grips", e.to_string()))
}

/// Drag a count/angle grip of an associative array to a point (one undo step).
pub(super) fn run_grip(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "array.reference.grip";
    let h = super::super::curves::handle_param(p, "handle").ok_or_else(|| bad(cmd, "`handle` is required"))?;
    let which = str_param(p, "which")
        .and_then(ArrayGripWhich::from_name)
        .ok_or_else(|| bad(cmd, "`which` must be \"cols\", \"rows\", \"items\" or \"angle\""))?;
    let to = point_param(p, "to").ok_or_else(|| bad(cmd, "`to` [x, y] is required"))?;
    drag(s, h, which, to).map_err(|e| bad(cmd, e))?;
    Ok(json!({ "handle": h.hex() }))
}

/// The largest count `which`'s axis may take without the array exceeding [`MAX_ARRAY_ITEMS`] or
/// MAXARRAY, given the other axis holds `other` items and the array has `levels` levels.
fn max_for_axis(s: &Session, other: u32, levels: u32) -> u64 {
    let denom = u64::from(other).saturating_mul(u64::from(levels)).max(1);
    (MAX_ARRAY_ITEMS / denom).max(1).min((s.settings.maxarray / denom).max(1))
}

/// The column (or row) count a drag of `len` drawing units along the axis, `spacing` apart,
/// rounds to: the nearest whole count, clamped to `[1, max]` (AutoCAD never allows 0, and a
/// hostile drag distance never grows the array past the limit). Never panics: float-to-int casts
/// in Rust saturate rather than overflow.
fn rounded_count(len: f64, spacing: f64, max: u64) -> u32 {
    if !len.is_finite() || spacing.abs() < 1e-12 {
        return 1;
    }
    let raw = (len.abs() / spacing.abs()).round() + 1.0;
    let raw = if raw.is_finite() { raw } else { max as f64 };
    let clamped = raw.clamp(1.0, max.min(u64::from(u32::MAX)) as f64);
    clamped as u32
}

/// The polar item count a drag from the grip's current position (`cur_items`) by `diff_deg`
/// degrees around the circle rounds to, holding the current angle-between-items (`step_deg`)
/// fixed: `cur_items + round(diff_deg / step_deg)`, clamped to `[1, max]`. Measured relative to
/// the grip's own current position (not the array's base point) so a drag of a few items, in
/// either direction, is never ambiguous with wrapping round the circle. Never panics (a
/// degenerate `step_deg` near zero, or a non-finite `diff_deg`, falls back to the unchanged
/// count rather than dividing by zero or casting NaN).
fn stepped_items(cur_items: u32, diff_deg: f64, step_deg: f64, max: u64) -> u32 {
    let max32 = max.min(u64::from(u32::MAX)) as f64;
    if !diff_deg.is_finite() || step_deg.abs() < 1e-9 {
        return (f64::from(cur_items)).clamp(1.0, max32) as u32;
    }
    let steps = (diff_deg / step_deg).round();
    let raw = f64::from(cur_items) + if steps.is_finite() { steps } else { 0.0 };
    let raw = if raw.is_finite() { raw } else { max32 };
    raw.clamp(1.0, max32) as u32
}

/// The angle in degrees, swept counterclockwise, from local point `from` to local point `to`, as
/// seen from `center`: `atan2` of both, wrapped to `(-180, 180]` so a drag distance never
/// overflows a multi-turn angle. Finite whenever `from` and `to` are (callers check the drag
/// point before calling this).
fn angle_between_points(from: Vec2, to: Vec2, center: Vec2) -> f64 {
    let a0 = (from.y - center.y).atan2(from.x - center.x);
    let a1 = (to.y - center.y).atan2(to.x - center.x);
    let raw = a1 - a0;
    raw.sin().atan2(raw.cos()).to_degrees()
}

/// Local point of polar array item `i` (row 0, level 0); the array's base point (local (0, 0))
/// when there is no such item (never panics on a hostile/empty item list).
fn polar_item_local(p: &cadcraft_doc::ArrayParams, i: u32) -> Vec2 {
    p.items().unwrap_or_default().into_iter().find(|it| it.i == i && it.j == 0 && it.level == 0).map(|it| it.insert.xy()).unwrap_or(Vec2::ZERO)
}

/// Drag grip `which` of array `h` to `to` in the session's drawing.
fn drag(s: &mut Session, h: Handle, which: ArrayGripWhich, to: Vec2) -> std::result::Result<(), String> {
    if !matches!(s.doc().map_err(|e| e.to_string())?.entity(h).map(|e| &e.kind), Some(EntityKind::Insert(_))) {
        return Err("`handle` is not a block reference".into());
    }
    if super::super::curves::is_locked(s, h) {
        return Err("the array is on a locked layer".into());
    }
    let d = s.doc().map_err(|e| e.to_string())?;
    let (ins, blk, mut p) = array_of(d, h).map_err(|e| e.to_string())?;
    let xf = ins.transform(blk.base.xy());
    let inv = xf.inverse().ok_or_else(|| "the array's scale is zero".to_string())?;
    let local = inv.apply(to);
    if !local.is_finite() {
        return Err("requires a valid point".into());
    }
    match (which, p.kind.clone()) {
        (ArrayGripWhich::Cols | ArrayGripWhich::Rows, ArrayKind::Rect(r)) => {
            let (ux, uy) = (Vec2::from_angle(r.axis_angle), Vec2::from_angle(r.axis_angle).perp());
            let n = match which {
                ArrayGripWhich::Cols => rounded_count(local.dot(ux), r.col_spacing, max_for_axis(s, r.rows, p.levels)),
                _ => rounded_count(local.dot(uy), r.row_spacing, max_for_axis(s, r.cols, p.levels)),
            };
            if let ArrayKind::Rect(rr) = &mut p.kind {
                match which {
                    ArrayGripWhich::Cols => rr.cols = n,
                    _ => rr.rows = n,
                }
            }
        }
        (ArrayGripWhich::Items, ArrayKind::Polar(k)) => {
            let current = polar_item_local(&p, k.items.saturating_sub(1));
            let diff_deg = angle_between_points(current, local, k.center);
            let step_deg = k.angle_between();
            let n = stepped_items(k.items, diff_deg, step_deg, max_for_axis(s, k.rows, p.levels));
            if let ArrayKind::Polar(kk) = &mut p.kind {
                kk.items = n; // The fill angle is kept; the per-item angle is recomputed from it.
            }
        }
        (ArrayGripWhich::Angle, ArrayKind::Polar(k)) => {
            let diff_deg = angle_between_points(Vec2::ZERO, local, k.center);
            let fill = fill_from_between(diff_deg, k.items).map_err(|e| e.to_string())?;
            if let ArrayKind::Polar(kk) = &mut p.kind {
                kk.fill = fill; // The item count is kept.
            }
        }
        _ => return Err("that grip does not apply to this array".into()),
    }
    rebuild(s, h, p, None).map_err(|e| e.to_string())
}

/// The world position of grip `which` of array `h`, when it has one.
fn grip_at(d: &Drawing, h: Handle, which: ArrayGripWhich) -> Option<Vec2> {
    array_grips(d, h).into_iter().find(|g| g.which == which).map(|g| g.at)
}

/// An array count grip being dragged.
struct ArrayGripEdit {
    handle: Handle,
    which: ArrayGripWhich,
    /// Where the grip is (the rubber band's start).
    from: Vec2,
}

impl Interactive for ArrayGripEdit {
    fn name(&self) -> &'static str {
        "GRIP"
    }

    fn begin(&mut self, _s: &mut Session) -> Result<Step> {
        Ok(Step::Continue)
    }

    fn prompt(&self, _s: &Session) -> Prompt {
        Prompt::new("** STRETCH ** Specify stretch point", crate::Accept::POINT).kw(&["eXit"]).base(self.from)
    }

    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        let to = match i {
            Input::Point(p) => p,
            Input::Deferred(d) => d.at,
            Input::Keyword(k) if k == "eXit" => return Ok(Step::Done),
            Input::Enter | Input::Cancel => return Ok(Step::Done),
            _ => return Err(EngineError::Other("Requires a point.".into())),
        };
        drag(s, self.handle, self.which, to).map_err(EngineError::Other)?;
        Ok(Step::Done)
    }

    /// The rubber band from the grip.
    fn preview(&self, _s: &Session, c: Vec2) -> Vec<EntityKind> {
        vec![crate::cmd::helpers::line(self.from, c)]
    }
}

impl Session {
    /// Start dragging count grip `which` of associative rectangular array `handle` (the UI's
    /// click on an array grip starts here).
    pub fn start_array_grip(&mut self, handle: Handle, which: ArrayGripWhich) -> Result<()> {
        if self.running.is_some() {
            self.cancel();
        }
        let from = grip_at(self.doc()?, handle, which).ok_or_else(|| EngineError::Other("no such grip".into()))?;
        let edit = ArrayGripEdit { handle, which, from };
        let machine = crate::guard::Guarded::wrap(Box::new(edit));
        let st = self.state()?;
        let (before, selection_before) = (st.doc.clone(), st.selection.clone());
        self.running = Some(Running { id: "grip".into(), machine, before, selection_before });
        self.pending_window = None;
        self.point_mods.clear();
        self.echo("** STRETCH **");
        Ok(())
    }
}

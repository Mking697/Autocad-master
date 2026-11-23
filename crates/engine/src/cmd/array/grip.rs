//! Count grips of an associative rectangular array: `array.reference.grips` lists the cols/rows
//! arrow grips, `array.reference.grip` drags one (one undo step), and
//! [`Session::start_array_grip`] starts the drag the UI's click on a grip begins, with the array
//! regenerated under the cursor as its preview. Polar arrays are explicitly out of scope: they
//! have no count grips here (see `docs/gaps.md` F5).

use cadcraft_doc::{ArrayGripWhich, ArrayKind, Drawing, EntityKind, Handle, MAX_ARRAY_ITEMS, array_grips};
use cadcraft_geom::Vec2;
use serde_json::{Value, json};

use super::edit::{array_of, rebuild};
use crate::cmd::{CommandSpec, bad, point_param, str_param};
use crate::{EngineError, Input, Interactive, Prompt, Result, Running, Session, Step};

pub(super) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("array.reference.grips", "Array Count Grips", run_grips)
            .params("{handle} → [{which: cols|rows, shape: linear, at: [x,y], dir: [x,y], colsOrRowsCount, spacing}] (empty: not a rectangular associative array)")
            .enabled(crate::cmd::has_doc)
            .noundo(),
        CommandSpec::new("array.reference.grip", "Drag Array Count Grip", run_grip)
            .params("{handle, which: \"cols\"|\"rows\", to: [x, y] (drawing coordinates)} → {handle}: rounds to the nearest whole count along that axis, clamped to at least 1 and at most MAXARRAY")
            .enabled(crate::cmd::has_doc),
    ]
}

/// The count grips of an associative array reference (an empty list for any other object, or a
/// polar array).
pub(super) fn run_grips(s: &mut Session, p: &Value) -> Result<Value> {
    let h = super::super::curves::handle_param(p, "handle").ok_or_else(|| bad("array.reference.grips", "`handle` is required"))?;
    serde_json::to_value(array_grips(s.doc()?, h)).map_err(|e| bad("array.reference.grips", e.to_string()))
}

/// Drag a count grip of an associative rectangular array to a point (one undo step).
pub(super) fn run_grip(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "array.reference.grip";
    let h = super::super::curves::handle_param(p, "handle").ok_or_else(|| bad(cmd, "`handle` is required"))?;
    let which = str_param(p, "which").and_then(ArrayGripWhich::from_name).ok_or_else(|| bad(cmd, "`which` must be \"cols\" or \"rows\""))?;
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
    let ArrayKind::Rect(r) = p.kind.clone() else { return Err("not a rectangular array (polar array grips are not available yet)".into()) };
    let xf = ins.transform(blk.base.xy());
    let inv = xf.inverse().ok_or_else(|| "the array's scale is zero".to_string())?;
    let local = inv.apply(to);
    if !local.is_finite() {
        return Err("requires a valid point".into());
    }
    let (ux, uy) = (Vec2::from_angle(r.axis_angle), Vec2::from_angle(r.axis_angle).perp());
    let n = match which {
        ArrayGripWhich::Cols => rounded_count(local.dot(ux), r.col_spacing, max_for_axis(s, r.rows, p.levels)),
        ArrayGripWhich::Rows => rounded_count(local.dot(uy), r.row_spacing, max_for_axis(s, r.cols, p.levels)),
    };
    if let ArrayKind::Rect(rr) = &mut p.kind {
        match which {
            ArrayGripWhich::Cols => rr.cols = n,
            ArrayGripWhich::Rows => rr.rows = n,
        }
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

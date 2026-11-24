//! Array count grips on the canvas, as AutoCAD shows them on a selected associative rectangular
//! or polar array: cols/rows arrows for a rectangular array, Items/Angle Between arrows for a
//! polar one (`cadcraft_doc::array_grips` emits whichever the array kind has; this module and
//! [`crate::canvas`]'s hookup are generic over `ArrayGripWhich`, no per-kind filtering). Dragging
//! runs the engine's array grip edit ([`cadcraft_engine::Session::start_array_grip`]); the arrow
//! outline is shared with `crate::dyngrips`.

use std::sync::Arc;

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke};

use crate::CadApp;
use crate::canvas::Xf;
use crate::dyngrips::arrow_outline;
use crate::theme::Tokens;
use cadcraft_engine::doc::array_grips as array_grips_of;
use cadcraft_engine::doc::{ArrayGrip, Handle};

/// Most selected arrays whose grips are shown.
const MAX_ARRAYS: usize = 200;

/// The array count grips of the selection, worked out once per drawing revision and selection.
#[derive(Clone, Default)]
pub struct Cache {
    key: (u64, u64, Vec<Handle>),
    grips: Arc<Vec<(Handle, ArrayGrip)>>,
}

/// The selected arrays' count grips (cached).
pub(crate) fn of_selection(app: &mut CadApp) -> Arc<Vec<(Handle, ArrayGrip)>> {
    let sel = app.session.selection();
    let Ok(st) = app.session.state() else { return Arc::default() };
    let key = (st.uid, st.revision, sel.clone());
    if app.canvas.array_grips.key != key {
        let grips: Vec<(Handle, ArrayGrip)> = sel
            .iter()
            .filter(|h| st.doc.entity(**h).is_some_and(|e| !st.doc.layer(&e.common.layer).is_some_and(|l| l.locked)))
            .take(MAX_ARRAYS)
            .flat_map(|h| array_grips_of(&st.doc, *h).into_iter().map(move |g| (*h, g)))
            .collect();
        app.canvas.array_grips = Cache { key, grips: Arc::new(grips) };
    }
    app.canvas.array_grips.grips.clone()
}

/// Whether `p` is on grip `g` (its box, a little larger, as for other grips).
fn hit(xf: &Xf, g: &ArrayGrip, gs: f32, p: Pos2) -> bool {
    let r = Rect::from_points(&arrow_outline(xf, g.at, g.dir, gs)).expand(3.0);
    r.contains(p)
}

/// Draw the count grips of the selected arrays; the one under the cursor warm, the one being
/// dragged hot.
pub(crate) fn draw(app: &mut CadApp, painter: &Painter, xf: &Xf, rect: Rect, hover: Option<Pos2>) {
    let t = Tokens::get();
    let gs = app.session.settings.gripsize as f32;
    let hot = app.canvas.array_hot;
    for (h, g) in of_selection(app).iter() {
        if !rect.contains(xf.to_screen(g.at)) {
            continue;
        }
        let pts = arrow_outline(xf, g.at, g.dir, gs);
        let is_hot = hot.as_ref().is_some_and(|(hh, w)| hh == h && *w == g.which);
        let warm = !is_hot && hover.is_some_and(|p| hit(xf, g, gs, p));
        let fill = if is_hot {
            t.grip_hot
        } else if warm {
            Color32::from_rgb(0xff, 0x7f, 0x9f)
        } else {
            t.grip_dyn
        };
        painter.add(Shape::convex_polygon(pts, fill, Stroke::new(1.0, Color32::from_rgb(0x10, 0x30, 0x80))));
    }
}

/// The array count grip under `hover`, if any: (array, grip).
pub(crate) fn at(app: &mut CadApp, xf: &Xf, hover: Option<Pos2>) -> Option<(Handle, ArrayGrip)> {
    let p = hover?;
    let gs = app.session.settings.gripsize as f32;
    of_selection(app).iter().find(|(_, g)| hit(xf, g, gs, p)).cloned()
}

/// A click on array count grip `g` of array `h`: start dragging it.
pub(crate) fn click(app: &mut CadApp, (h, g): (Handle, ArrayGrip)) {
    app.canvas.hot_grips.clear();
    match app.session.start_array_grip(h, g.which) {
        Ok(()) => app.canvas.array_hot = Some((h, g.which)),
        Err(e) => app.session.echo(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::{ArrayGripWhich, EntityKind};
    use cadcraft_engine::Session;
    use serde_json::json;

    /// A polar array of `count` items (360° fill) in model space, through the real `point` and
    /// `arraypolar` JSON commands: this exercises the same association/rebuild path a user's
    /// ARRAYPOLAR does, not a hand-built fixture.
    fn polar_array(app: &mut CadApp, count: u32) -> Handle {
        app.run("point", json!({"at": [1.0, 0.0]})).unwrap();
        let src = app.session.doc().unwrap().model.iter().next().unwrap().handle.hex();
        app.run("arraypolar", json!({"handles": [src], "center": [0.0, 0.0], "count": count})).unwrap();
        app.session.doc().unwrap().model.iter().find(|e| matches!(e.kind, EntityKind::Insert(_))).unwrap().handle
    }

    /// Polar count/angle grips are not filtered out anywhere in this module or `canvas.rs`'s
    /// hookup: this proves they are found and clickable through the exact lookup the canvas uses
    /// (`of_selection`/`at`/`click`), closing F5's "polar arrays have no count grips" claim in
    /// `docs/gaps.md` — the engine and doc-model sides already had them and tests; only the UI
    /// wiring (and the stale comment claiming otherwise) had never been exercised end to end.
    #[test]
    fn polar_array_items_and_angle_grips_are_found_and_clickable() {
        let mut app = CadApp::new(Session::new(), crate::Services::default());
        let h = polar_array(&mut app, 4); // 4 items: both Items and Angle Between grips show.
        app.session.set_selection(vec![h]);

        let grips = of_selection(&mut app);
        assert_eq!(grips.len(), 2, "{grips:?}");
        assert!(grips.iter().any(|(_, g)| g.which == ArrayGripWhich::Items));
        assert!(grips.iter().any(|(_, g)| g.which == ArrayGripWhich::Angle));

        let items = grips.iter().find(|(_, g)| g.which == ArrayGripWhich::Items).cloned().unwrap();
        click(&mut app, items.clone());
        assert_eq!(app.canvas.array_hot, Some((items.0, ArrayGripWhich::Items)));
        assert_eq!(app.session.running.as_ref().map(|r| r.id.as_str()), Some("grip"));
    }
}

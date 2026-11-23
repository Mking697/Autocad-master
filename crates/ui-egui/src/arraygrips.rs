//! Array count grips on the canvas, as AutoCAD shows them on a selected associative rectangular
//! array: an arrow at the far end of the column axis (drag it to change `cols`) and one at the
//! far end of the row axis (drag it to change `rows`). Dragging runs the engine's array grip edit
//! ([`cadcraft_engine::Session::start_array_grip`]). Polar arrays have no count grips here (see
//! `docs/gaps.md` F5); the arrow outline is shared with [`crate::dyngrips`].

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

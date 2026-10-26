//! BOQ (Bill of Quantities): a CADCraft-original command (AutoCAD has no direct equivalent —
//! users normally do this via Data Extraction or external tools). Scans a drawing's entities and
//! reports draft quantity totals for estimating: length (lines, polylines, arcs), area (closed
//! polylines, circles), and block reference counts, grouped by layer. This is a first, basic
//! step toward automatic quantity takeoff from a drawing; rates/costing are layered on top of
//! this data outside the engine (JSON in, spreadsheet/report out).

use std::collections::BTreeMap;

use cadcraft_doc::{EntityKind, Space};
use cadcraft_geom::Polyline;
use serde_json::{Value, json};

use super::{CommandSpec, bad};
use crate::{Result, Session};

/// BOQ's JSON parameters.
pub(super) const PARAMS: &str = "{space?: current|model|all (default all)|<layout>} → \
    {layers: {layer: {lengthTotal, areaTotal, blocks: {name: count}}}, entityCount, message}";

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("boq", "Bill of Quantities", run_boq)
            .menu(&["Tools", "Bill of Quantities"])
            .params(PARAMS)
            .noundo(),
    ]
}

#[derive(Default)]
struct LayerTotals {
    length: f64,
    area: f64,
    blocks: BTreeMap<String, u64>,
}

fn run_boq(s: &mut Session, p: &Value) -> Result<Value> {
    let d = s.doc()?;
    let scope = match p.get("space") {
        None | Some(Value::Null) => None,
        Some(Value::String(t)) => Some(t.trim().to_ascii_lowercase()),
        Some(v) => return Err(bad("boq", format!("`space` must be current, model, all or a layout name, got {v}"))),
    };

    let mut spaces: Vec<Space> = Vec::new();
    match scope.as_deref() {
        None | Some("all") => {
            spaces.push(Space::Model);
            for l in &d.layouts {
                spaces.push(Space::Paper(l.name.clone()));
            }
        }
        Some("current") => spaces.push(s.space()),
        Some("model") => spaces.push(Space::Model),
        Some(name) => {
            let l = d.layouts.iter().find(|l| l.name.eq_ignore_ascii_case(name)).ok_or_else(|| bad("boq", format!("no layout named \"{name}\"")))?;
            spaces.push(Space::Paper(l.name.clone()));
        }
    }

    let mut totals: BTreeMap<String, LayerTotals> = BTreeMap::new();
    let mut entity_count: u64 = 0;

    for sp in &spaces {
        let Some(store) = d.space(sp) else { continue };
        for e in store.iter() {
            entity_count += 1;
            let t = totals.entry(e.common.layer.clone()).or_default();
            match &e.kind {
                EntityKind::Line(l) => {
                    t.length += (l.b - l.a).len();
                }
                EntityKind::LwPolyline(pl) => {
                    let open = Polyline { vertices: pl.vertices.clone(), closed: pl.closed };
                    t.length += open.len();
                    if pl.closed {
                        let closed = Polyline { vertices: pl.vertices.clone(), closed: true };
                        t.area += closed.area().abs();
                    }
                }
                EntityKind::Arc(a) => {
                    let sweep = (a.end - a.start).rem_euclid(cadcraft_geom::TAU);
                    t.length += a.radius * sweep;
                }
                EntityKind::Circle(c) => {
                    t.area += c.radius * c.radius * cadcraft_geom::PI;
                }
                EntityKind::Insert(i) => {
                    let copies = u64::from(i.cols.max(1)).saturating_mul(u64::from(i.rows.max(1)));
                    *t.blocks.entry(i.block.clone()).or_insert(0) += copies;
                }
                _ => {}
            }
        }
    }

    let layers: BTreeMap<String, Value> = totals
        .iter()
        .map(|(layer, t)| {
            (layer.clone(), json!({ "lengthTotal": t.length, "areaTotal": t.area, "blocks": t.blocks }))
        })
        .collect();

    let mut lines = vec![format!("BOQ: {entity_count} entities across {} space(s)", spaces.len())];
    for (layer, t) in &totals {
        let mut parts = Vec::new();
        if t.length > 0.0 {
            parts.push(format!("length {:.3}", t.length));
        }
        if t.area > 0.0 {
            parts.push(format!("area {:.3}", t.area));
        }
        for (b, n) in &t.blocks {
            parts.push(format!("{n}x {b}"));
        }
        if !parts.is_empty() {
            lines.push(format!("  {layer}: {}", parts.join(", ")));
        }
    }

    Ok(json!({ "layers": layers, "entityCount": entity_count, "message": lines.join("\n") }))
}

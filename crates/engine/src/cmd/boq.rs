//! BOQ (Bill of Quantities): a CADCraft-original command (AutoCAD has no direct equivalent —
//! users normally do this via Data Extraction or external tools). Scans a drawing's entities and
//! reports draft quantity totals for estimating: length (lines, polylines, arcs), area (closed
//! polylines, circles), and block reference counts, grouped by layer. This is a first, basic
//! step toward automatic quantity takeoff from a drawing. An optional `rates` map turns those
//! quantities into a per-layer cost and a grand total, and an optional `exportCsv` path writes
//! the same breakdown to a CSV file, so a basic estimate can be produced without leaving the
//! engine (JSON in, spreadsheet/report out).

use std::collections::BTreeMap;

use cadcraft_doc::{EntityKind, Space};
use cadcraft_geom::Polyline;
use serde_json::{Value, json};

use super::{CommandSpec, bad};
use crate::{Result, Session};

/// BOQ's JSON parameters.
pub(super) const PARAMS: &str = "{space?: current|model|all (default all)|<layout>, \
    rates?: {<layer>|default: {ratePerLength?, ratePerArea?, ratePerBlock?, blockRates?: {<block>: rate}}}, \
    exportCsv?: path} → \
    {layers: {layer: {lengthTotal, areaTotal, blocks: {name: count}}}, costByLayer?, totalCost?, \
    csvPath?, entityCount, message}";

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

/// A parsed `rates` entry for one layer (or `"default"`).
#[derive(Default, Clone)]
struct LayerRate {
    per_length: Option<f64>,
    per_area: Option<f64>,
    per_block: Option<f64>,
    /// Per-block overrides of `per_block`, keyed by block name.
    block_rates: BTreeMap<String, f64>,
}

impl LayerRate {
    /// The cost of `count` copies of block `name`: its own rate if given, else this layer's
    /// `ratePerBlock`, else no cost.
    fn block_cost(&self, name: &str, count: u64) -> f64 {
        let rate = self.block_rates.get(name).copied().or(self.per_block);
        rate.map_or(0.0, |r| r * count as f64)
    }

    /// This layer's total cost for `t`'s quantities.
    fn cost(&self, t: &LayerTotals) -> f64 {
        let mut cost = self.per_length.unwrap_or(0.0) * t.length + self.per_area.unwrap_or(0.0) * t.area;
        for (name, count) in &t.blocks {
            cost += self.block_cost(name, *count);
        }
        cost
    }
}

/// A single `rates[layer]` entry: `{ratePerLength?, ratePerArea?, ratePerBlock?, blockRates?}`.
fn parse_rate(cmd: &str, layer: &str, v: &Value) -> Result<LayerRate> {
    let Value::Object(o) = v else { return Err(bad(cmd, format!("`rates.{layer}` must be an object"))) };
    let num = |key: &str| -> Result<Option<f64>> {
        match o.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::Number(n)) => Ok(n.as_f64()),
            Some(v) => Err(bad(cmd, format!("`rates.{layer}.{key}` must be a number, got {v}"))),
        }
    };
    let per_length = num("ratePerLength")?;
    let per_area = num("ratePerArea")?;
    let per_block = num("ratePerBlock")?;
    let mut block_rates = BTreeMap::new();
    match o.get("blockRates") {
        None | Some(Value::Null) => {}
        Some(Value::Object(bo)) => {
            for (name, rv) in bo {
                let Value::Number(n) = rv else {
                    return Err(bad(cmd, format!("`rates.{layer}.blockRates.{name}` must be a number, got {rv}")));
                };
                let Some(r) = n.as_f64() else {
                    return Err(bad(cmd, format!("`rates.{layer}.blockRates.{name}` is not a finite number")));
                };
                block_rates.insert(name.clone(), r);
            }
        }
        Some(v) => return Err(bad(cmd, format!("`rates.{layer}.blockRates` must be an object, got {v}"))),
    }
    Ok(LayerRate { per_length, per_area, per_block, block_rates })
}

/// The `rates` parameter: a map of layer name (or `"default"`) to a [`LayerRate`].
fn parse_rates(cmd: &str, p: &Value) -> Result<Option<BTreeMap<String, LayerRate>>> {
    match p.get("rates") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(o)) => {
            let mut out = BTreeMap::new();
            for (layer, v) in o {
                out.insert(layer.clone(), parse_rate(cmd, layer, v)?);
            }
            Ok(Some(out))
        }
        Some(v) => Err(bad(cmd, format!("`rates` must be an object, got {v}"))),
    }
}

/// The rate that applies to `layer`: its own entry, else `"default"`, else none.
fn rate_for<'a>(rates: &'a BTreeMap<String, LayerRate>, layer: &str) -> Option<&'a LayerRate> {
    rates.get(layer).or_else(|| rates.get("default"))
}

/// One CSV field, quoted (and its quotes doubled) if it contains a comma, quote or newline.
fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

/// Write the layer breakdown (and costs, if any) to `path` as CSV.
fn write_csv(cmd: &str, path: &str, totals: &BTreeMap<String, LayerTotals>, rates: Option<&BTreeMap<String, LayerRate>>) -> Result<()> {
    let mut out = String::from("layer,lengthTotal,areaTotal,blockCounts,cost\n");
    for (layer, t) in totals {
        let blocks = t.blocks.iter().map(|(name, n)| format!("{name}:{n}")).collect::<Vec<_>>().join(";");
        let cost = rates.and_then(|r| rate_for(r, layer)).map(|r| format!("{:.6}", r.cost(t)));
        out.push_str(&format!(
            "{},{},{},{},{}\n",
            csv_field(layer),
            t.length,
            t.area,
            csv_field(&blocks),
            cost.as_deref().unwrap_or("")
        ));
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::atomic_write::write_atomic(std::path::Path::new(path), out.as_bytes()).map_err(|e| bad(cmd, format!("{path}: {e}")))
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = out;
        Err(bad(cmd, "exportCsv is not available on the web; there is no file system"))
    }
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

    let rates = parse_rates("boq", p)?;
    let csv_path = match p.get("exportCsv") {
        None | Some(Value::Null) => None,
        Some(Value::String(path)) => Some(path.clone()),
        Some(v) => return Err(bad("boq", format!("`exportCsv` must be a path, got {v}"))),
    };

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

    let mut result = json!({ "layers": layers, "entityCount": entity_count });

    if let Some(rates) = &rates {
        let mut cost_by_layer: BTreeMap<String, f64> = BTreeMap::new();
        let mut total_cost = 0.0;
        for (layer, t) in &totals {
            if let Some(r) = rate_for(rates, layer) {
                let cost = r.cost(t);
                cost_by_layer.insert(layer.clone(), cost);
                total_cost += cost;
            }
        }
        if let Some(o) = result.as_object_mut() {
            o.insert("costByLayer".into(), json!(cost_by_layer));
            o.insert("totalCost".into(), json!(total_cost));
        }
        if !cost_by_layer.is_empty() {
            lines.push(format!("Total cost: {total_cost:.2}"));
            for (layer, cost) in &cost_by_layer {
                lines.push(format!("  {layer}: cost {cost:.2}"));
            }
        }
    }

    if let Some(path) = &csv_path {
        write_csv("boq", path, &totals, rates.as_ref())?;
        if let Some(o) = result.as_object_mut() {
            o.insert("csvPath".into(), json!(path));
        }
        lines.push(format!("Exported CSV to {path}"));
    }

    if let Some(o) = result.as_object_mut() {
        o.insert("message".into(), json!(lines.join("\n")));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boq_reports_quantities_without_rates() {
        let mut s = Session::new();
        s.cmdline("line 0,0 100,0").unwrap();
        s.cmdline("").unwrap();
        s.cmdline("circle 50,25 5").unwrap();
        let r = s.execute("boq", &json!({})).unwrap();
        assert!(r.get("costByLayer").is_none(), "no rates given: no cost fields");
        assert!(r.get("totalCost").is_none());
        assert_eq!(r["entityCount"], 2);
    }

    #[test]
    fn boq_computes_cost_by_layer_and_total() {
        let mut s = Session::new();
        s.cmdline("line 0,0 100,0").unwrap();
        s.cmdline("").unwrap();
        s.cmdline("circle 50,25 10").unwrap();
        let layer = "0"; // default layer for both entities
        let rates = json!({ layer: { "ratePerLength": 2.0, "ratePerArea": 0.5 } });
        let r = s.execute("boq", &json!({ "rates": rates })).unwrap();

        let length = r["layers"][layer]["lengthTotal"].as_f64().unwrap();
        let area = r["layers"][layer]["areaTotal"].as_f64().unwrap();
        let expected = length * 2.0 + area * 0.5;
        let got = r["costByLayer"][layer].as_f64().unwrap();
        assert!((got - expected).abs() < 1e-9, "{got} vs {expected}");
        assert!((r["totalCost"].as_f64().unwrap() - expected).abs() < 1e-9);
    }

    #[test]
    fn boq_block_rate_uses_its_own_rate_over_the_layer_default() {
        let mut t = LayerTotals::default();
        t.blocks.insert("Door".to_string(), 3);
        t.blocks.insert("Window".to_string(), 2);
        let mut r = LayerRate { per_block: Some(10.0), ..Default::default() };
        r.block_rates.insert("Door".to_string(), 25.0);
        // Door uses its own rate (25 * 3), Window falls back to the layer's ratePerBlock (10 * 2).
        let expected = 25.0 * 3.0 + 10.0 * 2.0;
        assert!((r.cost(&t) - expected).abs() < 1e-9);
    }

    #[test]
    fn boq_rejects_malformed_rates() {
        let mut s = Session::new();
        let e = s.execute("boq", &json!({ "rates": { "0": "not an object" } })).unwrap_err().to_string();
        assert!(e.contains("rates"), "{e}");
    }

    #[test]
    fn boq_exports_csv_with_header_and_rows() {
        let mut s = Session::new();
        s.cmdline("circle 0,0 10").unwrap();
        let dir = std::env::temp_dir().join(format!("cadcraft-boq-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("boq.csv").to_string_lossy().into_owned();

        let r = s.execute("boq", &json!({ "exportCsv": path })).unwrap();
        assert_eq!(r["csvPath"], json!(path));

        let text = std::fs::read_to_string(&path).unwrap();
        let mut lines = text.lines();
        assert_eq!(lines.next(), Some("layer,lengthTotal,areaTotal,blockCounts,cost"));
        let row = lines.next().unwrap();
        assert!(row.starts_with("0,0,"), "{row}");
        std::fs::remove_dir_all(&dir).ok();
    }
}

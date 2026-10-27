//! BOQ (Bill of Quantities): a CADCraft-original command (AutoCAD has no direct equivalent —
//! users normally do this via Data Extraction or external tools). Scans a drawing's entities and
//! reports draft quantity totals for estimating: length (lines, polylines, arcs), area (closed
//! polylines, circles), and block reference counts, grouped by layer. This is a first, basic
//! step toward automatic quantity takeoff from a drawing. An optional `rates` map (or `ratesFile`
//! JSON file) turns those quantities into a per-layer cost and a grand total, and an optional
//! `exportCsv` path writes the same breakdown to a CSV file, so a basic estimate can be produced
//! without leaving the engine (JSON in, spreadsheet/report out).
//!
//! Real drawings often draw a room's boundary as several separate LINE/ARC entities that happen
//! to close up, rather than one LwPolyline. BOQ reuses `inquiry::loops`'s endpoint-chaining logic
//! (read-only, unchanged) to find those implied closed loops and adds their area to the total —
//! see `add_implied_loop_areas` for the exact double-counting rule.

use std::collections::BTreeMap;

use cadcraft_doc::{EntityKind, Handle, Space};
use cadcraft_geom::Polyline;
use serde_json::{Value, json};

use super::{CommandSpec, bad};
use crate::{Result, Session};

/// BOQ's JSON parameters.
pub(super) const PARAMS: &str = "{space?: current|model|all (default all)|<layout>, \
    rates?: {<layer>|default: {ratePerLength?, ratePerArea?, ratePerBlock?, blockRates?: {<block>: rate}}}, \
    ratesFile?: path (a JSON file shaped like `rates`; used only when `rates` itself is not given), \
    includeImpliedLoops?: boolean (default false; gross XY areas, no hole/overlap subtraction), \
    exportCsv?: path} → \
    {layers: {layer: {lengthTotal, areaTotal, blocks: {name: count}}}, costByLayer?, totalCost?, \
    impliedLoopCount, impliedLoops: [{layer, area, members: [handle]}], \
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
            Some(Value::Number(n)) => n.as_f64().filter(|r| r.is_finite() && *r >= 0.0).map(Some)
                .ok_or_else(|| bad(cmd, format!("`rates.{layer}.{key}` must be finite and nonnegative"))),
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
                let Some(r) = n.as_f64().filter(|r| r.is_finite() && *r >= 0.0) else {
                    return Err(bad(cmd, format!("`rates.{layer}.blockRates.{name}` must be finite and nonnegative")));
                };
                block_rates.insert(name.clone(), r);
            }
        }
        Some(v) => return Err(bad(cmd, format!("`rates.{layer}.blockRates` must be an object, got {v}"))),
    }
    Ok(LayerRate { per_length, per_area, per_block, block_rates })
}

/// A full rates table: a map of layer name (or `"default"`) to a [`LayerRate`].
fn parse_rate_table(cmd: &str, o: &serde_json::Map<String, Value>) -> Result<BTreeMap<String, LayerRate>> {
    let mut out = BTreeMap::new();
    for (layer, v) in o {
        out.insert(layer.clone(), parse_rate(cmd, layer, v)?);
    }
    Ok(out)
}

/// The `rates` parameter: a map of layer name (or `"default"`) to a [`LayerRate`].
fn parse_rates(cmd: &str, p: &Value) -> Result<Option<BTreeMap<String, LayerRate>>> {
    match p.get("rates") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(o)) => Ok(Some(parse_rate_table(cmd, o)?)),
        Some(v) => Err(bad(cmd, format!("`rates` must be an object, got {v}"))),
    }
}

/// `ratesFile`: a JSON file on disk shaped like `rates`, read only when `rates` itself was not
/// given inline. A missing file, unreadable path or malformed JSON is a normal `bad` error, never
/// a panic.
#[cfg(not(target_arch = "wasm32"))]
fn read_rates_file(cmd: &str, path: &str) -> Result<BTreeMap<String, LayerRate>> {
    use std::io::Read;
    const MAX_RATES_BYTES: u64 = 1_048_576;
    let file = std::fs::File::open(path).map_err(|e| bad(cmd, format!("`ratesFile` {path}: {e}")))?;
    let mut text = String::new();
    file.take(MAX_RATES_BYTES + 1).read_to_string(&mut text).map_err(|e| bad(cmd, format!("`ratesFile` {path}: {e}")))?;
    if text.len() as u64 > MAX_RATES_BYTES {
        return Err(bad(cmd, "`ratesFile` exceeds the 1 MiB limit"));
    }
    let value: Value = serde_json::from_str(&text).map_err(|e| bad(cmd, format!("`ratesFile` {path}: invalid JSON: {e}")))?;
    let Value::Object(o) = &value else { return Err(bad(cmd, format!("`ratesFile` {path} must contain a JSON object, got {value}"))) };
    parse_rate_table(cmd, o)
}
#[cfg(target_arch = "wasm32")]
fn read_rates_file(cmd: &str, _path: &str) -> Result<BTreeMap<String, LayerRate>> {
    Err(bad(cmd, "ratesFile is not available on the web; there is no file system"))
}

/// `rates` if given inline, else the table loaded from `ratesFile`, else `None`.
fn resolve_rates(cmd: &str, p: &Value) -> Result<Option<BTreeMap<String, LayerRate>>> {
    if let Some(rates) = parse_rates(cmd, p)? {
        return Ok(Some(rates));
    }
    match p.get("ratesFile") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(path)) => Ok(Some(read_rates_file(cmd, path)?)),
        Some(v) => Err(bad(cmd, format!("`ratesFile` must be a path, got {v}"))),
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

/// Add implied closed-loop areas (loose LINE/ARC chains that close up, detected by
/// `inquiry::loops`) to `totals`, and return the JSON entries describing each loop picked up this
/// way.
///
/// Double-counting rule: a LINE or ARC that takes part in a detected implied loop still
/// contributes its own length to `lengthTotal` exactly as it always did (walls have a real linear
/// length for takeoff purposes too, e.g. skirting board or coping) — this function does not touch
/// `length`. On top of that, each implied loop's `area` (computed once by `loops`, not
/// re-tessellated here) is added once to `areaTotal`, keyed by the layer of the loop's first
/// member entity (its "effective layer"). Each detector cycle is visited once, and LwPolyline
/// entities cannot be members. This is a gross-area estimate: coincident independent geometry,
/// overlaps and nested boundaries are not deduplicated or subtracted (hence the opt-in flag).
fn add_implied_loop_areas(d: &cadcraft_doc::Drawing, loops_report: &Value, totals: &mut BTreeMap<String, LayerTotals>) -> Result<Vec<Value>> {
    let mut implied = Vec::new();
    let Some(loops) = loops_report.get("loops").and_then(Value::as_array) else { return Ok(implied) };
    for lp in loops {
        let Some(members) = lp.get("members").and_then(Value::as_array) else { continue };
        let Some(first_hex) = members.first().and_then(Value::as_str) else { continue };
        let Some(handle) = Handle::parse_hex(first_hex) else { continue };
        let Some(layer) = d.entity(handle).map(|e| e.common.layer.clone()) else { continue };
        let area = lp.get("area").and_then(Value::as_f64).filter(|a| a.is_finite() && *a >= 0.0)
            .ok_or_else(|| bad("boq", "implied loop has invalid area"))?;
        totals.entry(layer.clone()).or_default().area += area;
        implied.push(json!({ "layer": layer, "area": area, "members": members.clone() }));
    }
    Ok(implied)
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

    let rates = resolve_rates("boq", p)?;
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

    let mut implied_loops = Vec::new();
    let include_implied = match p.get("includeImpliedLoops") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(v)) => *v,
        Some(_) => return Err(bad("boq", "`includeImpliedLoops` must be a boolean")),
    };
    if include_implied {
        for sp in &spaces {
            // No pagination loss; work remains bounded by LOOPS' default pair cap.
            let report = super::inquiry::loops::analyze(d, sp, &json!({"limit": usize::MAX}))?;
            if report["capped"] == true || report["truncated"] == true {
                return Err(bad("boq", format!("implied-loop analysis incomplete in {sp:?}: endpoint pair cap exceeded; retry with includeImpliedLoops=false")));
            }
            implied_loops.extend(add_implied_loop_areas(d, &report, &mut totals)?);
        }
    }
    for (layer, t) in &totals {
        if !t.length.is_finite() || t.length < 0.0 || !t.area.is_finite() || t.area < 0.0 {
            return Err(bad("boq", format!("non-finite or negative quantities on layer {layer}")));
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
    if !implied_loops.is_empty() {
        lines.push(format!("{} implied closed loop(s) from loose LINE/ARC chains contributed area", implied_loops.len()));
    }

    let mut result = json!({
        "layers": layers,
        "entityCount": entity_count,
        "impliedLoopCount": implied_loops.len(),
        "impliedLoops": implied_loops,
        "impliedLoopLimitations": "Opt-in gross XY-projected areas; no hole/overlap subtraction or duplicate-geometry deduplication; branches unresolved; mixed-layer loops use first member's layer; independent closed entities remain separate quantities.",
    });

    if let Some(rates) = &rates {
        let mut cost_by_layer: BTreeMap<String, f64> = BTreeMap::new();
        let mut total_cost = 0.0;
        for (layer, t) in &totals {
            if let Some(r) = rate_for(rates, layer) {
                let cost = r.cost(t);
                cost_by_layer.insert(layer.clone(), cost);
                total_cost += cost;
                if !cost.is_finite() || !total_cost.is_finite() {
                    return Err(bad("boq", "cost overflow; use smaller rates or drawing quantities"));
                }
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

    fn triangle(s: &mut Session, x: f64, size: f64) {
        s.execute("line", &json!({"points": [[x, 0], [x + size, 0], [x, size], [x, 0]]})).unwrap();
    }

    #[test]
    fn boq_implied_areas_follow_scope_and_are_opt_in() {
        let mut s = Session::new();
        triangle(&mut s, 0.0, 2.0);
        s.execute("layout.set", &json!({"name": "Layout1"})).unwrap();
        triangle(&mut s, 10.0, 4.0);
        for (scope, area, count) in [("model", 2.0, 1), ("Layout1", 8.0, 1), ("current", 8.0, 1), ("all", 10.0, 2)] {
            let r = s.execute("boq", &json!({"space": scope, "includeImpliedLoops": true})).unwrap();
            assert_eq!(r["impliedLoopCount"], count, "{r}");
            assert!((r["layers"]["0"]["areaTotal"].as_f64().unwrap() - area).abs() < 1e-8, "{r}");
            assert_eq!(s.space(), Space::Paper("Layout1".into()));
        }
        let r = s.execute("boq", &json!({})).unwrap();
        assert_eq!(r["impliedLoopCount"], 0);
        assert_eq!(r["layers"]["0"]["areaTotal"], 0.0);
        assert!(s.execute("boq", &json!({"includeImpliedLoops": "yes"})).is_err());
    }

    #[test]
    fn boq_implied_arc_area_and_closed_polyline_are_each_counted_once() {
        let mut s = Session::new();
        s.execute("arc", &json!({"center": [0, 0], "radius": 2, "start": 0, "end": 180})).unwrap();
        s.execute("line", &json!({"points": [[-2, 0], [2, 0]]})).unwrap();
        s.execute("pline", &json!({"points": [[10, 0], [12, 0], [10, 2]], "closed": true})).unwrap();
        let r = s.execute("boq", &json!({"includeImpliedLoops": true})).unwrap();
        assert_eq!(r["impliedLoopCount"], 1, "{r}");
        let area = r["layers"]["0"]["areaTotal"].as_f64().unwrap();
        assert!((area - (2.0 * cadcraft_geom::PI + 2.0)).abs() < 1e-8, "{r}");
    }

    #[test]
    fn boq_implied_areas_do_not_stop_at_default_page_limit() {
        let mut s = Session::new();
        for i in 0..201 { triangle(&mut s, f64::from(i) * 3.0, 1.0); }
        let r = s.execute("boq", &json!({"includeImpliedLoops": true})).unwrap();
        assert_eq!(r["impliedLoopCount"], 201);
        assert_eq!(r["layers"]["0"]["areaTotal"], 100.5);
    }

    #[test]
    fn boq_implied_cap_is_an_error_not_a_partial_estimate() {
        let mut s = Session::new();
        for i in 0..237 { triangle(&mut s, f64::from(i) * 3.0, 1.0); }
        let e = s.execute("boq", &json!({"includeImpliedLoops": true})).unwrap_err().to_string();
        assert!(e.contains("cap"), "{e}");
        assert!(s.execute("boq", &json!({})).is_ok());
    }

    #[test]
    fn boq_rejects_cost_overflow() {
        let mut s = Session::new();
        s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
        assert!(s.execute("boq", &json!({"rates": {"default": {"ratePerLength": 1e308}}})).is_err());
    }

    #[test]
    fn boq_ratesfile_rejects_invalid_tables_and_oversized_files() {
        let mut s = Session::new();
        let dir = std::env::temp_dir().join(format!("cadcraft-boq-invalid-tables-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rates.json");
        for text in ["[]", "{\"default\":{\"ratePerArea\":-1}}", "{\"default\":{\"ratePerArea\":1e999}}"] {
            std::fs::write(&path, text).unwrap();
            assert!(s.execute("boq", &json!({"ratesFile": path})).is_err());
        }
        std::fs::write(&path, vec![b' '; 1_048_577]).unwrap();
        assert!(s.execute("boq", &json!({"ratesFile": path})).unwrap_err().to_string().contains("1 MiB"));
        // Inline tables do not read the (invalid) file.
        assert!(s.execute("boq", &json!({"ratesFile": path, "rates": {}})).is_ok());
        std::fs::remove_dir_all(dir).unwrap();
    }

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
    fn boq_rejects_negative_rates() {
        let mut s = Session::new();
        for entry in [json!({"ratePerLength": -1}), json!({"ratePerArea": -1}), json!({"ratePerBlock": -1}), json!({"blockRates": {"Door": -1}})] {
            assert!(s.execute("boq", &json!({"rates": {"default": entry}})).is_err());
        }
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

    /// Four separate LINEs forming a closed 10x10 square (no LwPolyline) should be detected as an
    /// implied loop: its area counts once on layer "0", and each LINE still contributes its own
    /// length to `lengthTotal` as usual.
    #[test]
    fn boq_detects_implied_loop_from_loose_lines_and_adds_its_area_once() {
        let mut s = Session::new();
        s.cmdline("line 0,0 10,0").unwrap();
        s.cmdline("").unwrap();
        s.cmdline("line 10,0 10,10").unwrap();
        s.cmdline("").unwrap();
        s.cmdline("line 10,10 0,10").unwrap();
        s.cmdline("").unwrap();
        s.cmdline("line 0,10 0,0").unwrap();
        s.cmdline("").unwrap();

        let r = s.execute("boq", &json!({"includeImpliedLoops": true})).unwrap();
        assert_eq!(r["impliedLoopCount"], 1, "{r}");
        let implied = r["impliedLoops"][0].clone();
        assert_eq!(implied["layer"], "0");
        assert!((implied["area"].as_f64().unwrap() - 100.0).abs() < 1e-6);
        assert_eq!(implied["members"].as_array().unwrap().len(), 4);

        let layer = "0";
        let area = r["layers"][layer]["areaTotal"].as_f64().unwrap();
        assert!((area - 100.0).abs() < 1e-6, "loop area should land on layer 0 exactly once: {area}");

        let length = r["layers"][layer]["lengthTotal"].as_f64().unwrap();
        assert!((length - 40.0).abs() < 1e-6, "all four 10-unit sides still sum their own length: {length}");
        assert_eq!(r["entityCount"], 4);
    }

    #[test]
    fn boq_ratesfile_loads_rates_from_disk() {
        let mut s = Session::new();
        s.cmdline("line 0,0 100,0").unwrap();
        s.cmdline("").unwrap();

        let dir = std::env::temp_dir().join(format!("cadcraft-boq-ratesfile-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rates.json");
        std::fs::write(&path, r#"{"0": {"ratePerLength": 3.0}}"#).unwrap();

        let r = s.execute("boq", &json!({ "ratesFile": path.to_string_lossy() })).unwrap();
        let length = r["layers"]["0"]["lengthTotal"].as_f64().unwrap();
        let expected = length * 3.0;
        let got = r["costByLayer"]["0"].as_f64().unwrap();
        assert!((got - expected).abs() < 1e-9, "{got} vs {expected}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn boq_rates_inline_wins_over_ratesfile() {
        let mut s = Session::new();
        s.cmdline("line 0,0 100,0").unwrap();
        s.cmdline("").unwrap();

        let dir = std::env::temp_dir().join(format!("cadcraft-boq-ratesfile-precedence-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rates.json");
        std::fs::write(&path, r#"{"0": {"ratePerLength": 999.0}}"#).unwrap();

        let r = s.execute("boq", &json!({ "ratesFile": path.to_string_lossy(), "rates": { "0": { "ratePerLength": 1.0 } } })).unwrap();
        let length = r["layers"]["0"]["lengthTotal"].as_f64().unwrap();
        let got = r["costByLayer"]["0"].as_f64().unwrap();
        assert!((got - length).abs() < 1e-9, "inline `rates` should win over `ratesFile`: {got} vs {length}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn boq_ratesfile_missing_file_is_a_bad_params_error_not_a_panic() {
        let mut s = Session::new();
        let path = std::env::temp_dir().join("cadcraft-boq-ratesfile-does-not-exist.json");
        std::fs::remove_file(&path).ok();
        let e = s.execute("boq", &json!({ "ratesFile": path.to_string_lossy() })).unwrap_err().to_string();
        assert!(e.contains("ratesFile"), "{e}");
    }

    #[test]
    fn boq_ratesfile_malformed_json_is_a_bad_params_error_not_a_panic() {
        let mut s = Session::new();
        let dir = std::env::temp_dir().join(format!("cadcraft-boq-ratesfile-malformed-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rates.json");
        std::fs::write(&path, "{ not valid json").unwrap();
        let e = s.execute("boq", &json!({ "ratesFile": path.to_string_lossy() })).unwrap_err().to_string();
        assert!(e.contains("ratesFile"), "{e}");
        std::fs::remove_dir_all(&dir).ok();
    }
}

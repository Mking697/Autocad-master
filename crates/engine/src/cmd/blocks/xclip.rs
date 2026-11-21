//! XCLIP's reference-owned boundary, JSON operations and command-line prompts.
//!
//! Clean-room references: Autodesk's public XCLIP command reference, GUID
//! 104EDB9F-F025-4F67-B5C9-B3F174CFE2F3, and SPATIAL_FILTER DXF reference,
//! 34F179D8-2030-47E4-8D49-F87B6538A05A. This is in-session/JSON data, NOT a
//! substitute DXF encoding: the ACAD_FILTER/SPATIAL extension-dictionary graph,
//! its matrices, and display/plot clipping remain follow-ups (docs/xclip.md).
use cadcraft_doc::{EntityKind, Handle, Insert, InsertClip, LwPolyline};
use cadcraft_geom::{Mat3, PolyVertex, Vec2};
use serde_json::{Value, json};

use crate::cmd::curves::{handle_value, pick_at};
use crate::cmd::helpers::{lwpoly, rect_vertices};
use crate::cmd::{CommandSpec, bad, bool_or, point_param, point_value, str_param, targets};
use crate::{Accept, Input, Interactive, Prompt, Result, Session, Step};

const MAX_POINTS: usize = 1024;
const LIMIT: f64 = 1e12;

pub(super) fn spec() -> CommandSpec {
    CommandSpec::new("xclip", "Clip Reference", run)
        .menu(&["Modify", "Clip", "External Reference"])
        .params("{handle? (one INSERT, otherwise selection), option?: new|on|off|delete|polyline|invert|clipdepth|?, p1?, p2? | points?: [[x,y],…] | object?: polyline handle, invert?: bool, front?, back?, remove?: bool}. Boundary coordinates are drawing coordinates. In-session/JSON/undo only: canvas/plot clipping and DXF/DWG SPATIAL_FILTER persistence are not available yet.")
        .interactive(|_| Ok(Box::new(XclipM::default())))
}

fn reference(s: &Session, p: &Value) -> Result<(Handle, Insert, Mat3)> {
    // Do not allocate a user-sized handle vector just to reject a multi-selection.
    if p.get("handles").and_then(Value::as_array).is_some_and(|hs| hs.len() != 1) {
        return Err(bad("xclip", "select exactly one block or external reference"));
    }
    let hs = targets(s, p)?;
    if hs.len() != 1 {
        return Err(bad("xclip", "select exactly one block or external reference"));
    }
    let h = hs.first().copied().ok_or_else(|| bad("xclip", "no reference selected"))?;
    if s.doc()?.space_of(h).as_ref() != Some(&s.space()) {
        return Err(bad("xclip", "the reference must be in the current space"));
    }
    let Some(EntityKind::Insert(i)) = s.doc()?.entity(h).map(|e| &e.kind) else {
        return Err(bad("xclip", "the selected object is not a block or external reference"));
    };
    let b = s.doc()?.block(&i.block).ok_or_else(|| bad("xclip", "the block definition is missing"))?;
    Ok((h, i.clone(), i.transform(b.base.xy())))
}

fn valid_point(p: Vec2) -> bool {
    p.is_finite() && p.x.abs() <= LIMIT && p.y.abs() <= LIMIT
}

/// XCLIP accepts straight, non-self-intersecting polygons, unlike VPCLIP's curved outlines.
/// Bound the quadratic simplicity check before doing any geometry work or allocation.
fn polygon(mut pts: Vec<Vec2>) -> Result<LwPolyline> {
    if pts.len() > MAX_POINTS || pts.iter().any(|p| !valid_point(*p)) {
        return Err(bad("xclip", "boundary coordinates or vertex count exceed the supported range"));
    }
    if pts.first().zip(pts.last()).is_some_and(|(a, b)| a.near(*b, 1e-9)) {
        pts.pop();
    }
    let n = pts.len();
    if n < 3 {
        return Err(bad("xclip", "a boundary needs at least three distinct points"));
    }
    let edges: Vec<_> = pts.iter().copied().zip(pts.iter().copied().cycle().skip(1)).take(n).collect();
    for (i, &(a, b)) in edges.iter().enumerate() {
        if a.near(b, 1e-9) {
            return Err(bad("xclip", "a boundary edge has zero length"));
        }
        for (j, &(c, d)) in edges.iter().enumerate().skip(i + 1) {
            if j == i + 1 || (i == 0 && j == n - 1) {
                continue;
            }
            let ab = cadcraft_geom::Line::new(a, b);
            let cd = cadcraft_geom::Line::new(c, d);
            if cadcraft_geom::line_line(&ab, &cd).is_some() || ab.dist(c) < 1e-9 || ab.dist(d) < 1e-9 || cd.dist(a) < 1e-9 || cd.dist(b) < 1e-9 {
                return Err(bad("xclip", "the boundary must not intersect itself"));
            }
        }
    }
    let k = lwpoly(pts.into_iter().map(PolyVertex::new).collect(), true);
    if cadcraft_render::clip::boundary_polygon(&k).and_then(|p| cadcraft_render::clip::PolyClip::new(&p)).is_none() {
        return Err(bad("xclip", "the boundary has no area"));
    }
    let EntityKind::LwPolyline(p) = k else {
        return Err(bad("xclip", "invalid boundary"));
    };
    Ok(p)
}

fn boundary(s: &Session, p: &Value, xf: Mat3) -> Result<LwPolyline> {
    let pts = if let Some(v) = p.get("object") {
        let h = handle_value(v).ok_or_else(|| bad("xclip", "`object` must be a polyline handle"))?;
        if s.doc()?.space_of(h).as_ref() != Some(&s.space()) {
            return Err(bad("xclip", "the polyline must be in the current space"));
        }
        let Some(EntityKind::LwPolyline(pl)) = s.doc()?.entity(h).map(|e| &e.kind) else {
            return Err(bad("xclip", "select a straight-segment 2D polyline"));
        };
        if pl.vertices.len() > MAX_POINTS || pl.vertices.iter().any(|v| v.bulge != 0.0) {
            return Err(bad("xclip", "select a straight-segment polyline with at most 1024 points"));
        }
        pl.vertices.iter().map(|v| v.p).collect()
    } else if let Some(v) = p.get("points") {
        let a = v.as_array().filter(|a| a.len() <= MAX_POINTS).ok_or_else(|| bad("xclip", "`points` must contain at most 1024 points"))?;
        a.iter().map(|v| point_value(v).ok_or_else(|| bad("xclip", "invalid boundary point"))).collect::<Result<Vec<_>>>()?
    } else {
        let a = point_param(p, "p1").ok_or_else(|| bad("xclip", "pass `p1` and `p2`, `points`, or `object`"))?;
        let b = point_param(p, "p2").ok_or_else(|| bad("xclip", "`p2` is required"))?;
        rect_vertices(a, b).into_iter().map(|v| v.p).collect()
    };
    // Check world coordinates as well as transformed ones (singular/hostile INSERTs).
    let world = polygon(pts)?;
    let inv = xf.inverse().ok_or_else(|| bad("xclip", "the reference transform is singular"))?;
    polygon(world.vertices.iter().map(|v| inv.apply(v.p)).collect())
}

fn depth(p: &Value, key: &str) -> Result<Option<f64>> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_f64()
            .filter(|v| v.is_finite() && (0.0..=LIMIT).contains(v))
            .map(Some)
            .ok_or_else(|| bad("xclip", format!("`{key}` must be a finite, nonnegative distance"))),
    }
}

fn run(s: &mut Session, p: &Value) -> Result<Value> {
    let (h, mut ins, xf) = reference(s, p)?;
    let op = str_param(p, "option").unwrap_or("new").trim().to_ascii_lowercase();
    if bool_or(p, "delete", false) || matches!(op.as_str(), "delete" | "d") {
        ins.clip = None;
    } else if matches!(op.as_str(), "?" | "list") {
        return Ok(json!({"handle":h.hex(), "clip":ins.clip}));
    } else if matches!(op.as_str(), "polyline" | "p" | "generate polyline") {
        let c = ins.clip.as_ref().ok_or_else(|| bad("xclip", "the reference has no boundary"))?;
        let pts = c.boundary.vertices.iter().take(MAX_POINTS + 1).map(|v| xf.apply(v.p)).collect();
        let pl = polygon(pts)?;
        let polyline = s.add_entity(EntityKind::LwPolyline(pl))?;
        return Ok(json!({"handle":h.hex(), "polyline":polyline.hex()}));
    } else if matches!(op.as_str(), "on" | "off" | "invert" | "i" | "clipdepth" | "c") {
        let c = ins.clip.as_mut().ok_or_else(|| bad("xclip", "the reference has no boundary"))?;
        match op.as_str() {
            "on" => c.on = true,
            "off" => c.on = false,
            "invert" | "i" => c.inverted = p.get("invert").and_then(Value::as_bool).unwrap_or(!c.inverted),
            _ => {
                let (front, back) = if bool_or(p, "remove", false) { (None, None) } else { (depth(p, "front")?, depth(p, "back")?) };
                if front.is_none() && back.is_none() && !bool_or(p, "remove", false) {
                    return Err(bad("xclip", "pass `front`, `back`, or `remove`: true"));
                }
                c.front = front;
                c.back = back;
            }
        }
    } else if matches!(op.as_str(), "new" | "n" | "new boundary" | "rectangular" | "polygonal") {
        let boundary = boundary(s, p, xf)?;
        ins.clip = Some(InsertClip { boundary, on: true, inverted: bool_or(p, "invert", false), front: None, back: None });
    } else {
        return Err(bad("xclip", "unknown XCLIP option"));
    }
    let out = json!({"handle":h.hex(), "clip":ins.clip});
    s.doc_mut()?.modify_entity(h, |e| e.kind = EntityKind::Insert(ins))?;
    s.echo("XCLIP data updated (session/JSON only; canvas/plot clipping and DXF/DWG persistence are not available yet).");
    Ok(out)
}

#[derive(Default)]
enum Stage {
    #[default]
    Select,
    Options,
    New,
    Polyline,
    First,
    Opposite(Vec2),
    Polygon(Vec<Vec2>),
    Front,
    Back(f64),
}
#[derive(Default)]
struct XclipM {
    handle: Option<Handle>,
    inverted: bool,
    stage: Stage,
}
impl XclipM {
    fn apply(&self, s: &mut Session, mut p: Value) -> Result<Step> {
        let h = self.handle.ok_or_else(|| bad("xclip", "no reference selected"))?;
        if let Some(o) = p.as_object_mut() {
            o.insert("handle".into(), json!(h.hex()));
        }
        run(s, &p)?;
        Ok(Step::Done)
    }
    fn pick(s: &Session, i: &Input) -> Option<Handle> {
        match i {
            Input::Point(p) => pick_at(s, *p),
            Input::Pick(hs) if hs.len() == 1 => hs.first().copied(),
            _ => None,
        }
    }
}
impl Interactive for XclipM {
    fn name(&self) -> &'static str {
        "XCLIP"
    }
    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        if let Ok((h, _, _)) = reference(s, &json!({})) {
            self.handle = Some(h);
            self.stage = Stage::Options;
        }
        Ok(Step::Continue)
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match &self.stage {
            Stage::Select => Prompt::new("Select block or external reference", Accept::SELECT),
            Stage::Options => {
                Prompt::new("Enter clipping option", Accept::TEXT).kw(&["ON", "OFF", "Clipdepth", "Delete", "Polyline", "New"]).default("New")
            }
            Stage::New => {
                Prompt::new("Choose boundary type", Accept::TEXT).kw(&["Select", "Polygonal", "Rectangular", "Invert"]).default("Rectangular")
            }
            Stage::Polyline => Prompt::new("Select straight-segment polyline", Accept::SELECT),
            Stage::First => Prompt::new("Specify first corner", Accept::POINT),
            Stage::Opposite(a) => Prompt::new("Specify opposite corner", Accept::POINT).corner(*a),
            Stage::Polygon(pts) => Prompt::new("Specify polygon point; Enter to finish", Accept::POINT).kw(&["Undo"]).base_opt(pts.last().copied()),
            Stage::Front => Prompt::new("Specify front clip distance", Accept::NUMBER).kw(&["Remove"]),
            Stage::Back(_) => Prompt::new("Specify back clip distance", Accept::NUMBER),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        if matches!(i, Input::Cancel) {
            return Ok(Step::Cancel);
        }
        let key = match &i {
            Input::Text(k) | Input::Keyword(k) => k.trim().to_ascii_lowercase(),
            _ => String::new(),
        };
        match &mut self.stage {
            Stage::Select => {
                if matches!(i, Input::Enter) {
                    return Ok(Step::Done);
                }
                if let Some(h) = Self::pick(s, &i) {
                    reference(s, &json!({"handle":h.hex()}))?;
                    self.handle = Some(h);
                    self.stage = Stage::Options;
                }
            }
            Stage::Options => match key.as_str() {
                "on" | "off" | "delete" | "d" | "polyline" | "p" => return self.apply(s, json!({"option":key})),
                "clipdepth" | "c" => self.stage = Stage::Front,
                "new" | "n" | "" if !key.is_empty() || matches!(i, Input::Enter) => self.stage = Stage::New,
                _ => return Err(bad("xclip", "invalid clipping option")),
            },
            Stage::New => match key.as_str() {
                "select" | "s" => self.stage = Stage::Polyline,
                "polygonal" | "p" => self.stage = Stage::Polygon(Vec::new()),
                "rectangular" | "r" => self.stage = Stage::First,
                "" if matches!(i, Input::Enter) => self.stage = Stage::First,
                "invert" | "i" => {
                    let has_clip = self
                        .handle
                        .and_then(|h| s.doc().ok()?.entity(h))
                        .is_some_and(|e| matches!(&e.kind,EntityKind::Insert(i) if i.clip.is_some()));
                    if has_clip {
                        return self.apply(s, json!({"option":"invert"}));
                    }
                    self.inverted = !self.inverted;
                    s.echo(if self.inverted { "Outside boundary will be retained." } else { "Inside boundary will be retained." });
                }
                _ => return Err(bad("xclip", "invalid boundary option")),
            },
            Stage::Polyline => {
                if let Some(h) = Self::pick(s, &i) {
                    return self.apply(s, json!({"object":h.hex(),"invert":self.inverted}));
                }
            }
            Stage::First => {
                if let Input::Point(a) = i {
                    self.stage = Stage::Opposite(a);
                }
            }
            Stage::Opposite(a) => {
                if let Input::Point(b) = i {
                    let a = *a;
                    return self.apply(s, json!({"p1":[a.x,a.y],"p2":[b.x,b.y],"invert":self.inverted}));
                }
            }
            Stage::Polygon(pts) => match i {
                Input::Point(p) => {
                    if pts.len() >= MAX_POINTS || !valid_point(p) {
                        return Err(bad("xclip", "invalid point or too many vertices"));
                    }
                    pts.push(p);
                }
                Input::Enter => {
                    let points = pts.iter().map(|p| [p.x, p.y]).collect::<Vec<_>>();
                    return self.apply(s, json!({"points":points,"invert":self.inverted}));
                }
                _ if key == "undo" || key == "u" => {
                    pts.pop();
                }
                _ => {}
            },
            Stage::Front => {
                if key == "remove" || key == "r" {
                    return self.apply(s, json!({"option":"clipdepth","remove":true}));
                }
                if let Some(v) = crate::cmd::machines::number(&key) {
                    depth(&json!({"front":v}), "front")?;
                    self.stage = Stage::Back(v);
                }
            }
            Stage::Back(front) => {
                if let Some(v) = crate::cmd::machines::number(&key) {
                    let front = *front;
                    return self.apply(s, json!({"option":"clipdepth","front":front,"back":v}));
                }
            }
        }
        Ok(Step::Continue)
    }
    fn preview(&self, _s: &Session, p: Vec2) -> Vec<EntityKind> {
        match &self.stage {
            Stage::Opposite(a) => vec![lwpoly(rect_vertices(*a, p), true)],
            Stage::Polygon(pts) => {
                let vs = pts.iter().copied().chain(std::iter::once(p)).map(PolyVertex::new).collect();
                vec![lwpoly(vs, true)]
            }
            _ => Vec::new(),
        }
    }
}

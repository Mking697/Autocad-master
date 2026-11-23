//! MLEADER's leader types (straight, spline, none) and block content: applied to the multileader
//! `mleader_kind` builds, for the prompt machine and the JSON form.

use std::collections::BTreeMap;

use cadcraft_doc::{Entity, EntityKind, Handle, Insert, MLeaderStyle};
use cadcraft_geom::{Vec2, Vec3};
use serde_json::Value;

use super::{bad, str_param};
use crate::{EngineError, LeaderType, Result, Session};

#[path = "mleader_block_attrs.rs"]
mod block_attrs;

pub(super) fn attribute_values(p: &Value, key: &str, cmd: &str) -> Result<BTreeMap<String, String>> {
    block_attrs::values(p, key, cmd)
}

/// How the multileader differs from a straight leader with text.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Content {
    pub leader: LeaderType,
    /// Block content: the block name, and whether its extents centre (rather than its insertion
    /// point) attaches to the end of the landing.
    pub block: Option<(String, bool)>,
    pub block_scale: Option<f64>,
    pub attribs: BTreeMap<String, String>,
}

/// A block MLEADER can use as content: defined, and not an anonymous (`*`) block.
pub(super) fn block_name(s: &Session, name: &str) -> Result<String> {
    let name = name.trim();
    match s.doc()?.block(name) {
        Some(b) if !b.name.starts_with('*') => Ok(b.name.clone()),
        _ => Err(EngineError::Other(format!("Block \"{name}\" not found."))),
    }
}

pub(super) fn current_style(s: &Session) -> Result<cadcraft_doc::MLeaderStyle> {
    let d = s.doc()?;
    let cur = d.header.str("CMLEADERSTYLE", "Standard");
    Ok(d.mleader_styles.iter().find(|st| st.name.eq_ignore_ascii_case(&cur)).or(d.mleader_styles.first()).cloned().unwrap_or_default())
}

/// Validate new block defaults before MLEADERSTYLE mutates the document.
pub(super) fn style_block_fields(s: &Session, p: &Value) -> Result<(Option<Option<String>>, Option<f64>, Option<bool>)> {
    let block = p
        .get("block")
        .map(|v| match v {
            Value::Null => Ok(None),
            Value::String(n) => block_name(s, n).map(Some).map_err(|e| bad("mleaderstyle", e.to_string())),
            _ => Err(bad("mleaderstyle", "`block` must be a block name or null (MText)")),
        })
        .transpose()?;
    let scale = p
        .get("blockScale")
        .map(|v| {
            v.as_f64()
                .filter(|v| v.is_finite() && *v > 0.0 && *v <= 1.0e12)
                .ok_or_else(|| bad("mleaderstyle", "`blockScale` must be positive, finite and at most 1e12"))
        })
        .transpose()?;
    let center = p.get("blockCenter").map(|v| v.as_bool().ok_or_else(|| bad("mleaderstyle", "`blockCenter` must be boolean"))).transpose()?;
    Ok((block, scale, center))
}

/// Apply the leader type and block content to a multileader from `mleader_kind`.
pub(super) fn apply(s: &Session, k: EntityKind, c: &Content) -> Result<EntityKind> {
    let EntityKind::MLeader(mut m) = k else { return Ok(k) };
    m.spline = c.leader == LeaderType::Spline;
    let land = m.landing.xy();
    // The side the content sits on, as `mleader_kind` chose it for the text.
    let dir = if m.leaders.first().and_then(|l| l.first()).is_some_and(|a| a.x > land.x) { -1.0 } else { 1.0 };
    if c.leader == LeaderType::None {
        // Content only, at the picked location (text from its top left corner).
        m.leaders.clear();
        m.dogleg = 0.0;
        if let Some(t) = &mut m.text {
            t.insert = m.landing;
            t.attach = 1;
        }
    }
    if let Some((name, center)) = &c.block {
        let name = block_name(s, name)?;
        let st = current_style(s)?;
        let style_block = st.block.as_ref().is_some_and(|n| n.eq_ignore_ascii_case(&name));
        let scale = c.block_scale.unwrap_or(if style_block { st.block_scale } else { 1.0 });
        if !scale.is_finite() || scale <= 0.0 || scale > 1.0e12 {
            return Err(bad("mleader", "invalid block content scale"));
        }
        let mut values = if style_block { st.block_attribs } else { BTreeMap::new() };
        if !MLeaderStyle::valid_block_attribs(&values) || !MLeaderStyle::valid_block_attribs(&c.attribs) {
            return Err(bad("mleader", "invalid block attribute defaults"));
        }
        values.extend(c.attribs.clone());
        if !MLeaderStyle::valid_block_attribs(&values) {
            return Err(bad("mleader", "too many block attribute values"));
        }
        let anchor = land + Vec2::new(m.dogleg * dir, 0.0);
        let mut ins = Insert {
            block: name.clone(),
            insert: Vec3::new(anchor.x, anchor.y, m.landing.z),
            scale: Vec3::new(scale, scale, scale),
            rotation: 0.0,
            attribs: Vec::new(),
            cols: 1,
            rows: 1,
            col_spacing: 0.0,
            row_spacing: 0.0,
            clip: None,
        };
        if *center {
            // Move the block so the centre of its extents lands on the anchor.
            let d = s.doc()?;
            let probe = Entity { handle: Handle(0), common: Default::default(), kind: EntityKind::Insert(ins.clone()) };
            let b = cadcraft_doc::entity_bounds(d, &probe, 0);
            if !b.is_empty() {
                let c = b.center();
                if c.is_finite() {
                    ins.insert = Vec3::new(ins.insert.x + anchor.x - c.x, ins.insert.y + anchor.y - c.y, ins.insert.z);
                }
            }
        }
        block_attrs::instantiate(s, &mut ins, &values)?;
        m.text = None;
        m.block = Some(ins);
    }
    Ok(EntityKind::MLeader(m))
}

/// `leaderType` ("straight" | "spline" | "none"), `block` (a block name) and `blockAttach`
/// ("center" | "insertion") from the JSON form.
pub(super) fn json_content(s: &Session, p: &Value) -> Result<Content> {
    let leader = match str_param(p, "leaderType").map(str::to_ascii_lowercase).as_deref() {
        None | Some("straight") => LeaderType::Straight,
        Some("spline") => LeaderType::Spline,
        Some("none") => LeaderType::None,
        Some(_) => return Err(bad("mleader", "`leaderType` must be \"straight\", \"spline\" or \"none\"")),
    };
    let st = current_style(s)?;
    let chosen = match p.get("block") {
        None => st.block.as_deref(),
        Some(Value::Null) => None,
        Some(Value::String(n)) => Some(n.as_str()),
        Some(_) => return Err(bad("mleader", "`block` must be a block name or null (MText)")),
    };
    let block = match chosen {
        None => None,
        Some(n) => {
            let center = match str_param(p, "blockAttach").map(str::to_ascii_lowercase).as_deref() {
                None => st.block.as_ref().filter(|b| b.eq_ignore_ascii_case(n)).is_none_or(|_| st.block_center),
                Some("center") => true,
                Some("insertion") => false,
                Some(_) => return Err(bad("mleader", "`blockAttach` must be \"center\" or \"insertion\"")),
            };
            Some((block_name(s, n).map_err(|e| bad("mleader", e.to_string()))?, center))
        }
    };
    let block_scale = p
        .get("blockScale")
        .map(|v| {
            v.as_f64()
                .filter(|v| v.is_finite() && *v > 0.0 && *v <= 1.0e12)
                .ok_or_else(|| bad("mleader", "`blockScale` must be positive, finite and at most 1e12"))
        })
        .transpose()?;
    Ok(Content { leader, block, block_scale, attribs: attribute_values(p, "attribs", "mleader")? })
}

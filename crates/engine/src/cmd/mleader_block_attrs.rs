//! Bounded attribute values and ATTDEF instantiation for MLEADER block content.

use std::collections::BTreeMap;

use cadcraft_doc::{Attrib, EntityKind, Insert, MLeaderStyle};
use serde_json::Value;

use crate::cmd::bad;
use crate::{Result, Session};

/// Normalize attribute tags once, rejecting ambiguous spellings and oversized inputs
/// before copying their values into drawing state.
pub(super) fn values(p: &Value, key: &str, cmd: &str) -> Result<BTreeMap<String, String>> {
    let Some(v) = p.get(key) else { return Ok(BTreeMap::new()) };
    let values = v
        .as_object()
        .filter(|v| v.len() <= MLeaderStyle::MAX_BLOCK_ATTRIBS)
        .ok_or_else(|| bad(cmd, format!("`{key}` must be an object with at most {} attributes", MLeaderStyle::MAX_BLOCK_ATTRIBS)))?;
    let mut out = BTreeMap::new();
    let mut bytes = 0usize;
    for (tag, value) in values {
        let value = value.as_str().ok_or_else(|| bad(cmd, "attribute values must be strings"))?;
        bytes = bytes.saturating_add(tag.len()).saturating_add(value.len());
        if tag.trim().is_empty() || tag.len() > 255 || value.len() > 65_536 || bytes > MLeaderStyle::MAX_BLOCK_ATTRIB_BYTES {
            return Err(bad(cmd, "attribute tag or value is too large"));
        }
        if out.insert(tag.to_ascii_uppercase(), value.to_string()).is_some() {
            return Err(bad(cmd, "duplicate attribute tag"));
        }
    }
    Ok(out)
}

/// Place nonconstant ATTDEFs under the final insertion transform, like INSERT. Constant
/// attributes remain definition geometry. Invisible/locked/multiline definitions retain
/// their modes and geometry. Instance values no longer depend on the style afterward.
pub(super) fn instantiate(s: &Session, ins: &mut Insert, values: &BTreeMap<String, String>) -> Result<()> {
    let d = s.doc()?;
    let blk = d.block(&ins.block).ok_or_else(|| bad("mleader", "content block not found"))?;
    let xf = ins.transform(blk.base.xy());
    let mut bytes = 0usize;
    for e in blk.entities.iter() {
        let EntityKind::AttDef(ad) = &e.kind else { continue };
        if ad.constant {
            continue;
        }
        if ins.attribs.len() >= MLeaderStyle::MAX_BLOCK_ATTRIBS {
            return Err(bad("mleader", "too many block attribute definitions"));
        }
        let value = values.get(&ad.tag.to_ascii_uppercase()).unwrap_or(&ad.text.value);
        bytes = bytes.saturating_add(ad.tag.len()).saturating_add(value.len());
        if ad.tag.len() > 255 || value.len() > 65_536 || bytes > MLeaderStyle::MAX_BLOCK_ATTRIB_BYTES {
            return Err(bad("mleader", "block attribute definition is too large"));
        }
        let mut k = EntityKind::Text(ad.text.clone());
        k.transform(&xf);
        let EntityKind::Text(mut text) = k else { continue };
        text.value = value.clone();
        ins.attribs.push(Attrib {
            tag: ad.tag.clone(),
            text,
            invisible: ad.invisible,
            constant: false,
            prompt: String::new(),
            props: Default::default(),
            multiline: ad.multiline.as_ref().map(|ml| ml.transformed(&xf)),
            modes: ad.modes.for_reference(),
        });
    }
    Ok(())
}

//! CADCraft's per-style block attribute defaults. The public MLEADERSTYLE DXF
//! groups describe the block/scale/attachment but no attribute-default list. Keep
//! that additional policy in a versioned NOD XRECORD, not invented native groups.

use std::collections::BTreeMap;

use cadcraft_doc::MLeaderStyle;
use serde::{Deserialize, Serialize};

pub(crate) const KEY: &str = "CADCRAFT_MLEADER_BLOCK_DEFAULTS";
const MAX_BYTES: usize = 8 << 20;
const MAX_STYLES: usize = 1024;
type Defaults = BTreeMap<String, BTreeMap<String, String>>;

#[derive(Serialize, Deserialize)]
struct Payload {
    version: u8,
    styles: Defaults,
}

pub(crate) fn chunks(styles: &[MLeaderStyle]) -> Option<Vec<String>> {
    let mut defaults = Defaults::new();
    let mut bytes = 0usize;
    for st in styles.iter().filter(|s| !s.block_attribs.is_empty()) {
        if defaults.len() >= MAX_STYLES || st.name.len() > 255 || !MLeaderStyle::valid_block_attribs(&st.block_attribs) {
            return None;
        }
        bytes = st.block_attribs.iter().fold(bytes.saturating_add(st.name.len()), |n, (k, v)| n.saturating_add(k.len()).saturating_add(v.len()));
        if bytes > MAX_BYTES {
            return None;
        }
        defaults.insert(st.name.clone(), st.block_attribs.clone());
    }
    if defaults.is_empty() {
        return None;
    }
    let json = serde_json::to_string(&Payload { version: 1, styles: defaults }).ok()?;
    (json.len() <= MAX_BYTES).then(|| crate::dxf_ext::json_chunks(&json))
}

fn parse(text: &str) -> Option<Defaults> {
    if text.len() > MAX_BYTES {
        return None;
    }
    let p: Payload = serde_json::from_str(text).ok()?;
    if p.version != 1 || p.styles.len() > MAX_STYLES {
        return None;
    }
    let mut out = Defaults::new();
    for (name, values) in p.styles {
        if name.trim().is_empty() || name.len() > 255 || !MLeaderStyle::valid_block_attribs(&values) {
            return None;
        }
        let mut normalized = BTreeMap::new();
        for (tag, value) in values {
            if normalized.insert(tag.to_ascii_uppercase(), value).is_some() {
                return None;
            }
        }
        out.insert(name, normalized);
    }
    Some(out)
}

pub(crate) fn apply(styles: &mut [MLeaderStyle], text: &str) {
    let Some(defaults) = parse(text) else { return };
    for st in styles {
        if let Some(values) = defaults.get(&st.name) {
            st.block_attribs = values.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostile_default_metadata_is_rejected() {
        for text in ["null", "{}", "{\"version\":2,\"styles\":{}}", "{\"version\":1,\"styles\":{\"S\":{\"X\":[]}}}"] {
            assert!(parse(text).is_none());
        }
        let values: BTreeMap<String, String> = (0..=MLeaderStyle::MAX_BLOCK_ATTRIBS).map(|i| (format!("T{i}"), "v".into())).collect();
        let payload = Payload { version: 1, styles: BTreeMap::from([("S".into(), values)]) };
        assert!(parse(&serde_json::to_string(&payload).unwrap()).is_none());
        assert!(parse(&" ".repeat(MAX_BYTES + 1)).is_none());
    }

    #[test]
    fn defaults_preserve_literal_dxf_escapes_and_unicode() {
        let st = MLeaderStyle {
            name: "Labels".into(),
            block_attribs: BTreeMap::from([("NUM".into(), "Ω \\U+0041 \\P".repeat(100))]),
            ..Default::default()
        };
        let text = chunks(std::slice::from_ref(&st)).unwrap().concat();
        assert_eq!(parse(&text).unwrap().get("Labels"), Some(&st.block_attribs));
    }
}

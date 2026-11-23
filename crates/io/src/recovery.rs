//! Explicit, conservative recovery of a damaged ASCII DXF entity-section tail.
//! Never used by OPEN: no resynchronization, invented values or binary-format guessing.
use cadcraft_doc::Drawing;

use crate::{IoError, Result};

/// Recovery bounds scanning and temporary allocations before attempting a parse.
pub const MAX_RECOVERY_BYTES: usize = 64 * 1024 * 1024;

/// Read normally first; on an ASCII tag-stream failure, retain only complete records before
/// the damaged record in ENTITIES. Other damage, including binary DXF/DWG corruption, is
/// reported rather than guessed at. Existing DWG failsafe notes remain visible unchanged.
pub fn recover(bytes: &[u8], name: &str) -> Result<Drawing> {
    if bytes.len() > MAX_RECOVERY_BYTES {
        return Err(IoError::Format("RECOVER: input exceeds the 64 MiB recovery budget".into()));
    }
    let original = match crate::read(bytes, name) {
        Ok(d) => return Ok(d),
        Err(e) => e,
    };
    if bytes.starts_with(b"AutoCAD Binary DXF") || cadcraft_dwg::is_dwg(bytes) {
        return Err(original);
    }
    if !matches!(cadcraft_dxf::parse(bytes), Err(cadcraft_dxf::DxfError::Eof | cadcraft_dxf::DxfError::BadCode { .. })) {
        return Err(original);
    }
    let Some(cut) = complete_entity_prefix(bytes) else { return Err(original) };
    let Some(prefix) = bytes.get(..cut) else { return Err(original) };
    let mut d = crate::dxf_read::read(prefix)?;
    if d.model.is_empty() && d.layouts.iter().all(|l| l.entities.is_empty()) {
        return Err(original);
    }
    d.read_notes.push(format!(
        "RECOVER: discarded {} bytes starting at the damaged ENTITIES record; only the complete prefix was recovered. Inspect the drawing before saving; the source file was not changed.",
        bytes.len().saturating_sub(cut)
    ));
    Ok(d)
}

fn complete_entity_prefix(bytes: &[u8]) -> Option<usize> {
    let mut lines = bytes.split_inclusive(|b| *b == b'\n');
    let mut offset = 0usize;
    let mut record = 0usize;
    let mut section = "";
    let mut section_name = false;
    loop {
        let code_line = lines.next()?;
        let start = offset;
        offset = offset.checked_add(code_line.len())?;
        let code_text = std::str::from_utf8(code_line).ok().map(str::trim);
        if code_text == Some("") {
            continue;
        }
        let Some(code) = code_text.and_then(|t| t.parse::<i32>().ok()) else {
            return (section == "ENTITIES").then_some(record);
        };
        let Some(value_line) = lines.next() else {
            return (section == "ENTITIES").then_some(if code == 0 { start } else { record });
        };
        offset = offset.checked_add(value_line.len())?;
        // Only structural names are ASCII; drawing strings retain their original code-page bytes.
        let value = std::str::from_utf8(value_line).ok().map(str::trim).unwrap_or("");
        if code == 0 {
            record = start;
            section_name = value == "SECTION";
            if value == "ENDSEC" || value == "EOF" {
                section = "";
            }
        } else if section_name && code == 2 {
            section = if value == "ENTITIES" { "ENTITIES" } else { "other" };
            section_name = false;
        }
    }
}

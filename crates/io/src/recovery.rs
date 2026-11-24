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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recover_rejects_input_over_the_recovery_budget_without_panicking() {
        let huge = vec![b'0'; MAX_RECOVERY_BYTES + 1];
        let err = recover(&huge, "huge.dxf").unwrap_err();
        assert!(matches!(err, IoError::Format(msg) if msg.contains("64 MiB")));
    }

    #[test]
    fn recover_refuses_to_guess_at_corrupt_binary_dwg() {
        // Looks like a DWG (the `AC10xx` magic `is_dwg` checks for) but the rest is garbage:
        // RECOVER must not attempt binary resynchronization, only report the original error.
        let mut corrupt = b"AC1032".to_vec();
        corrupt.extend(std::iter::repeat_n(0xA5u8, 512));
        let err = recover(&corrupt, "corrupt.dwg").unwrap_err();
        assert!(!format!("{err}").is_empty());
    }

    #[test]
    fn recover_refuses_binary_dxf_signature() {
        let mut corrupt = b"AutoCAD Binary DXF".to_vec();
        corrupt.extend(std::iter::repeat_n(0u8, 64));
        assert!(recover(&corrupt, "corrupt.dxf").is_err());
    }

    #[test]
    fn recover_on_empty_bytes_does_not_panic() {
        assert!(recover(b"", "empty.dxf").is_err());
    }

    #[test]
    fn complete_entity_prefix_excludes_an_incomplete_entity_record() {
        // Truncated mid-group (a lone trailing group code, no value) with no complete entity
        // before it: the cut point must land before the incomplete LINE record entirely, so a
        // caller building a salvaged drawing from this prefix never invents its geometry.
        let truncated = b"0\nSECTION\n2\nENTITIES\n0\nLINE\n5\n10\n10\n1\n20";
        let cut = complete_entity_prefix(truncated).expect("ENTITIES section was opened");
        assert!(truncated[..cut].ends_with(b"ENTITIES\n"), "cut must land before the incomplete LINE record");
    }

    #[test]
    fn complete_entity_prefix_never_panics_on_malformed_tag_streams() {
        let inputs: &[&[u8]] = &[
            b"",
            b"\n",
            b"0\n",
            b"0\nSECTION\n2\nENTITIES\n0",
            b"garbage-not-a-code\nvalue\n",
            b"0\nSECTION\n2\nENTITIES\n999999999999999999999999\nvalue\n",
            // A code line with no following value line.
            b"0\nSECTION\n2\nENTITIES\n0\nLINE\n5",
        ];
        for input in inputs {
            // Must return without panicking; the exact cut point isn't asserted here, only safety.
            let _ = complete_entity_prefix(input);
        }
    }

    #[test]
    fn complete_entity_prefix_cuts_before_the_damaged_record() {
        let bytes = b"0\nSECTION\n2\nENTITIES\n0\nLINE\n5\n10\n10\n1\n20\n2\n11\n3\n21\n4\n0\nCIRCLE\n10\n9\n20";
        let cut = complete_entity_prefix(bytes).expect("a complete LINE record precedes the damage");
        let prefix = &bytes[..cut];
        assert!(prefix.ends_with(b"21\n4\n"), "prefix should end right after the complete LINE record");
        assert!(!String::from_utf8_lossy(prefix).contains("CIRCLE"), "the damaged record must not be included");
    }
}

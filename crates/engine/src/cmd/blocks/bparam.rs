//! BPARAMETER and BACTIONTOOL: author one Linear parameter and its paired Stretch action into the
//! block being edited (BEDIT). This build authors exactly those two types (`docs/gaps.md`'s
//! Top-10 #4 / F3's smallest vertical slice): JSON-command-only, no draggable block-editor
//! graphics yet. BSAVE/BSAVEAS regenerate the block's dynamic-block data from what is stored here
//! (`cadcraft_doc::dynblock::author_linear_stretch`), since nothing else keeps it between saves.

use cadcraft_doc::dynblock::{LinearParam, StretchAction};
use serde_json::{Value, json};

use super::bedit::editing;
use crate::cmd::helpers::v3;
use crate::cmd::{CommandSpec, bad, bool_or, point_req, points_param, str_param, targets};
use crate::{Result, Session};

pub(super) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("bparameter", "Parameter", run_bparameter)
            .menu(&["Tools", "Block Editor", "Parameter", "Linear"])
            .alias(&["param"])
            .params(
                "{type: \"linear\" (the only type this build authors), base: [x,y], end: [x,y], label? (default \"Distance1\"), description?, fromMidpoint?: bool (default false)} → {node}",
            )
            .enabled(editing),
        CommandSpec::new("bactiontool", "Action Tool", run_bactiontool)
            .menu(&["Tools", "Block Editor", "Action Tool", "Stretch"])
            .alias(&["actiontool"])
            .params(
                "{type: \"stretch\" (the only type this build authors), handles? | handle? (default the current selection): the objects it moves, frame: [[x,y],[x,y],…] (2 opposite corners, or a polygon), name? (default \"Stretch1\")} → {node}",
            )
            .enabled(has_parameter),
    ]
}

/// BACTIONTOOL needs a parameter added first in this session (BPARAMETER): this slice pairs one
/// Stretch action with the one Linear parameter it authors.
fn has_parameter(s: &Session) -> std::result::Result<(), String> {
    editing(s)?;
    match s.state() {
        Ok(st) if st.block_edit.as_ref().is_some_and(|b| b.pending_param.is_some()) => Ok(()),
        Ok(_) => Err("Add a Linear parameter first (BPARAMETER).".into()),
        Err(e) => Err(e.to_string()),
    }
}

fn run_bparameter(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "bparameter";
    let kind = str_param(p, "type").unwrap_or("linear");
    if !kind.eq_ignore_ascii_case("linear") {
        return Err(bad(cmd, "`type` must be \"linear\" (the only type this build can author)"));
    }
    let base = point_req(cmd, p, "base")?;
    let end = point_req(cmd, p, "end")?;
    if base.near(end, 1e-9) {
        return Err(bad(cmd, "`base` and `end` must not be the same point"));
    }
    let param = LinearParam {
        base: v3(base),
        end: v3(end),
        label: str_param(p, "label").unwrap_or("Distance1").trim().to_string(),
        description: str_param(p, "description").unwrap_or("").to_string(),
        from_midpoint: bool_or(p, "fromMidpoint", false),
    };
    let st = s.state_mut()?;
    let be = st.block_edit.as_mut().ok_or_else(|| bad(cmd, "The Block Editor is not open."))?;
    be.pending_param = Some(param);
    // A redefined parameter invalidates a previously authored action's connections (graph id 1
    // stays the same here, but the pairing is re-made explicitly by a fresh BACTIONTOOL call).
    be.pending_action = None;
    s.echo("Linear parameter added; BACTIONTOOL pairs a Stretch action with it, BSAVE keeps both.");
    Ok(json!({ "node": 1 }))
}

fn run_bactiontool(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "bactiontool";
    let kind = str_param(p, "type").unwrap_or("stretch");
    if !kind.eq_ignore_ascii_case("stretch") {
        return Err(bad(cmd, "`type` must be \"stretch\" (the only type this build can author)"));
    }
    let selection = targets(s, p)?;
    if selection.is_empty() {
        return Err(bad(cmd, "`handles` (or a selection) is required"));
    }
    let frame = points_param(p, "frame").ok_or_else(|| bad(cmd, "`frame` ([[x, y], …], at least 2 points) is required"))?;
    if frame.len() < 2 {
        return Err(bad(cmd, "`frame` needs at least 2 points"));
    }
    let name = str_param(p, "name").unwrap_or("Stretch1").trim().to_string();
    let action = StretchAction { selection, frame, name };
    let st = s.state_mut()?;
    let be = st.block_edit.as_mut().ok_or_else(|| bad(cmd, "The Block Editor is not open."))?;
    be.pending_action = Some(action);
    s.echo("Stretch action added; BSAVE writes both into the block's dynamic-block data.");
    Ok(json!({ "node": 2 }))
}

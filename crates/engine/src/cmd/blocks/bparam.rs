//! BPARAMETER and BACTIONTOOL: author one parameter and its paired action into the block being
//! edited (BEDIT). This build authors three vertical slices (`docs/gaps.md`'s Top-10 #4 / F3): a
//! Linear parameter with a Stretch action, a Point parameter with a Move action, and a Visibility
//! parameter (no paired action — a reference's Visibility State property drives it directly) —
//! JSON-command-only, no draggable block-editor graphics yet. BSAVE/BSAVEAS regenerate the
//! block's dynamic-block data from what is stored here (`cadcraft_doc::dynblock::author_*`),
//! since nothing else keeps it between saves.

use cadcraft_doc::dynblock::{LinearParam, MoveAction, PointParam, StretchAction, VisibilityParam, VisibilityStateDef};
use serde_json::{Value, json};

use super::bedit::editing;
use crate::blockedit::{PendingAction, PendingParam};
use crate::cmd::curves::handle_value;
use crate::cmd::helpers::v3;
use crate::cmd::{CommandSpec, bad, bool_or, point_req, points_param, str_param, targets};
use crate::{Result, Session};

pub(super) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("bparameter", "Parameter", run_bparameter)
            .menu(&["Tools", "Block Editor", "Parameter", "Linear"])
            .alias(&["param"])
            .params(
                "{type: \"linear\" | \"point\" | \"visibility\" (default \"linear\"), base: [x,y], end: [x,y], label? (default \"Distance1\"), description?, fromMidpoint?: bool (default false)} for linear; {type: \"point\", at: [x,y], label? (default \"Position1\"), description?} for point; {type: \"visibility\", at: [x,y], label? (default \"Visibility1\"), description?, controlled?: [handle,…] (default the current selection), states: [{name, visible: [handle,…]}, …] (at least 2)} for visibility → {node}",
            )
            .enabled(editing),
        CommandSpec::new("bactiontool", "Action Tool", run_bactiontool)
            .menu(&["Tools", "Block Editor", "Action Tool", "Stretch"])
            .alias(&["actiontool"])
            .params(
                "{type: \"stretch\" | \"move\" (default matches the pending parameter), handles? | handle? (default the current selection): the objects it moves, frame?: [[x,y],[x,y],…] (2 opposite corners, or a polygon; required for \"stretch\", unused for \"move\"), name? (default \"Stretch1\"/\"Move1\")} → {node}",
            )
            .enabled(has_parameter),
    ]
}

/// BACTIONTOOL needs a parameter added first in this session (BPARAMETER): this slice pairs one
/// action with the one parameter it authors.
fn has_parameter(s: &Session) -> std::result::Result<(), String> {
    editing(s)?;
    match s.state() {
        Ok(st) if st.block_edit.as_ref().is_some_and(|b| b.pending_param.is_some()) => Ok(()),
        Ok(_) => Err("Add a parameter first (BPARAMETER).".into()),
        Err(e) => Err(e.to_string()),
    }
}

fn run_bparameter(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "bparameter";
    let kind = str_param(p, "type").unwrap_or("linear");
    let pending = if kind.eq_ignore_ascii_case("linear") {
        let base = point_req(cmd, p, "base")?;
        let end = point_req(cmd, p, "end")?;
        if base.near(end, 1e-9) {
            return Err(bad(cmd, "`base` and `end` must not be the same point"));
        }
        PendingParam::Linear(LinearParam {
            base: v3(base),
            end: v3(end),
            label: str_param(p, "label").unwrap_or("Distance1").trim().to_string(),
            description: str_param(p, "description").unwrap_or("").to_string(),
            from_midpoint: bool_or(p, "fromMidpoint", false),
        })
    } else if kind.eq_ignore_ascii_case("point") {
        let at = point_req(cmd, p, "at")?;
        PendingParam::Point(PointParam {
            at: v3(at),
            label: str_param(p, "label").unwrap_or("Position1").trim().to_string(),
            description: str_param(p, "description").unwrap_or("").to_string(),
        })
    } else if kind.eq_ignore_ascii_case("visibility") {
        let at = point_req(cmd, p, "at")?;
        let controlled = targets(s, p)?;
        let states_val =
            p.get("states").and_then(Value::as_array).ok_or_else(|| bad(cmd, "`states` (at least 2, each {name, visible}) is required"))?;
        if states_val.len() < 2 {
            return Err(bad(cmd, "`states` needs at least 2 entries"));
        }
        let mut states = Vec::with_capacity(states_val.len());
        for st in states_val {
            let name = st.get("name").and_then(Value::as_str).ok_or_else(|| bad(cmd, "each state needs a `name`"))?.trim().to_string();
            if name.is_empty() {
                return Err(bad(cmd, "a state's `name` must not be empty"));
            }
            let visible = st
                .get("visible")
                .and_then(Value::as_array)
                .map(|a| a.iter().map(handle_value).collect::<Option<Vec<_>>>())
                .unwrap_or(Some(Vec::new()))
                .ok_or_else(|| bad(cmd, "a state's `visible` must be an array of object handles"))?;
            states.push(VisibilityStateDef { name, visible });
        }
        PendingParam::Visibility(VisibilityParam {
            at: v3(at),
            label: str_param(p, "label").unwrap_or("Visibility1").trim().to_string(),
            description: str_param(p, "description").unwrap_or("").to_string(),
            controlled,
            states,
        })
    } else {
        return Err(bad(cmd, "`type` must be \"linear\", \"point\" or \"visibility\" (the only types this build can author)"));
    };
    let st = s.state_mut()?;
    let be = st.block_edit.as_mut().ok_or_else(|| bad(cmd, "The Block Editor is not open."))?;
    be.pending_param = Some(pending);
    // A redefined parameter invalidates a previously authored action's connections (graph id 1
    // stays the same here, but the pairing is re-made explicitly by a fresh BACTIONTOOL call).
    be.pending_action = None;
    s.echo("Parameter added; BACTIONTOOL pairs an action with it, BSAVE keeps both.");
    Ok(json!({ "node": 1 }))
}

fn run_bactiontool(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "bactiontool";
    // Default the action type to match the pending parameter, as AutoCAD's Action Tool palette
    // only offers compatible actions for the parameter being edited.
    let default_kind = match s.state().ok().and_then(|st| st.block_edit.as_ref()).and_then(|b| b.pending_param.as_ref()) {
        Some(PendingParam::Point(_)) => "move",
        _ => "stretch",
    };
    let kind = str_param(p, "type").unwrap_or(default_kind);
    let selection = targets(s, p)?;
    if selection.is_empty() {
        return Err(bad(cmd, "`handles` (or a selection) is required"));
    }
    let pending = if kind.eq_ignore_ascii_case("stretch") {
        let frame = points_param(p, "frame").ok_or_else(|| bad(cmd, "`frame` ([[x, y], …], at least 2 points) is required"))?;
        if frame.len() < 2 {
            return Err(bad(cmd, "`frame` needs at least 2 points"));
        }
        let name = str_param(p, "name").unwrap_or("Stretch1").trim().to_string();
        PendingAction::Stretch(StretchAction { selection, frame, name })
    } else if kind.eq_ignore_ascii_case("move") {
        let name = str_param(p, "name").unwrap_or("Move1").trim().to_string();
        PendingAction::Move(MoveAction { selection, name })
    } else {
        return Err(bad(cmd, "`type` must be \"stretch\" or \"move\" (the only types this build can author)"));
    };
    let st = s.state_mut()?;
    let be = st.block_edit.as_mut().ok_or_else(|| bad(cmd, "The Block Editor is not open."))?;
    be.pending_action = Some(pending);
    s.echo("Action added; BSAVE writes both into the block's dynamic-block data.");
    Ok(json!({ "node": 2 }))
}

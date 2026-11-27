//! Authoring one Linear parameter and its paired Stretch action into a block's dynamic-block
//! data: the smallest vertical slice of authoring (`docs/gaps.md`'s Top-10 #4 / F3). It builds
//! exactly what this module's reader ([`super::dynamic_block`]) expects, so a block authored here
//! reads back — and drags — like one made in another program: an `ACAD_ENHANCEDBLOCK` xdict
//! entry on the block's `BLOCK_RECORD`, pointing at an `ACAD_EVALUATION_GRAPH` that lists a
//! `BLOCKLINEARPARAMETER` and, when given, a `BLOCKSTRETCHACTION` connected to its end point.
//! Group codes were checked against the public DXF Reference's
//! BLOCKLINEARPARAMETER/BLOCKSTRETCHACTION entries for the numbers only (no text copied); the
//! layout otherwise matches [`super`]'s own reader, which is all CADCraft itself needs to agree
//! with.
//!
//! Handles are placeholders freshly minted with [`Drawing::new_handle`]: the DXF/DWG writer
//! remaps every kept object's handle and reference on save (`crates/io/src/dxf_raw.rs`'s
//! `finish`), so authoring code never predicts the file's eventual handles.
//!
//! This slice supports exactly one parameter and one action per block: authoring again replaces
//! them (graph ids 1 and 2 are fixed), and a block that already carries dynamic-block data (from
//! another program, or a previous call) is refused rather than merged with or corrupted.
//!
//! The action moves its whole selection (no per-vertex restriction): `vertices` (`731/94`) stays
//! empty, because listing an object there with no indices would move *nothing* of it — the
//! reader's `move_entity` only takes the whole-object path when an object isn't in that list at
//! all. Selection alone (`71`/`330`) decides what moves.

use cadcraft_geom::{Vec2, Vec3};

use crate::foreign::ForeignObject;
use crate::{Drawing, Handle, RawTag};

/// The parameter's graph id (this slice authors exactly one).
const PARAM_NODE: i32 = 1;
/// The action's graph id.
const ACTION_NODE: i32 = 2;

fn tag(code: i32, value: impl Into<String>) -> RawTag {
    RawTag { code, value: value.into() }
}

/// A point written as `code`, `code + 10`, `code + 20`.
fn point_tags(code: i32, p: Vec3) -> Vec<RawTag> {
    vec![tag(code, p.x.to_string()), tag(code + 10, p.y.to_string()), tag(code + 20, p.z.to_string())]
}

/// A Linear parameter to author: a distance between `base` and `end`
/// (`AcDbBlock2PtParameter` + `AcDbBlockLinearParameter`).
#[derive(Clone, Debug, PartialEq)]
pub struct LinearParam {
    pub base: Vec3,
    pub end: Vec3,
    /// The property name a reference's Properties palette shows ("Distance1").
    pub label: String,
    pub description: String,
    /// The value moves both points apart from their midpoint (`true`) or just the end point.
    pub from_midpoint: bool,
}

/// A Stretch action paired with the Linear parameter: dragging its end point moves `selection`'s
/// objects, whole, by the same amount. `frame` is kept for the block editor and for carrying
/// other parameters along; it does not gate `selection` itself (see the module comment).
#[derive(Clone, Debug, PartialEq)]
pub struct StretchAction {
    /// The entities it moves.
    pub selection: Vec<Handle>,
    /// The stretch frame: two opposite corners, or a polygon of 3 or more points.
    pub frame: Vec<Vec2>,
    /// Shown in the block editor ("Stretch1").
    pub name: String,
}

/// A Point parameter to author: a single draggable position
/// (`AcDbBlock1PtParameter` + `AcDbBlockPointParameter`).
#[derive(Clone, Debug, PartialEq)]
pub struct PointParam {
    pub at: Vec3,
    /// The property name a reference's Properties palette shows ("Position1").
    pub label: String,
    pub description: String,
}

/// A Move action paired with the Point parameter: dragging the point moves `selection`'s objects,
/// whole, by the same amount (no stretch frame — a Move action always moves its whole selection).
#[derive(Clone, Debug, PartialEq)]
pub struct MoveAction {
    /// The entities it moves.
    pub selection: Vec<Handle>,
    /// Shown in the block editor ("Move1").
    pub name: String,
}

/// A named set of visible objects, part of a [`VisibilityParam`]: the state's name and which of
/// the parameter's `controlled` objects are shown in it (the rest are hidden). This slice leaves
/// a state's "active" list (other parameters/actions shown only in this state) empty — every
/// other parameter stays visible in every state, matching a block with no further authoring.
#[derive(Clone, Debug, PartialEq)]
pub struct VisibilityStateDef {
    pub name: String,
    pub visible: Vec<Handle>,
}

/// A Visibility parameter to author: named states that show or hide `controlled`'s objects — no
/// paired action (a reference's Visibility State property drives it directly, through
/// [`super::set_property`]/[`super::drag_grip`]'s existing Visibility handling, which already
/// reads this shape from other programs' files).
#[derive(Clone, Debug, PartialEq)]
pub struct VisibilityParam {
    pub at: Vec3,
    /// The property name a reference's Properties palette shows ("Visibility1").
    pub label: String,
    pub description: String,
    /// Every object the parameter controls (shown in some states, hidden in others).
    pub controlled: Vec<Handle>,
    pub states: Vec<VisibilityStateDef>,
}

/// Why [`author_linear_stretch`] would refuse to author into block `name`, if it would.
fn refusal(d: &Drawing, name: &str) -> Option<String> {
    let key = ("BLOCK_RECORD".to_string(), name.trim().to_ascii_uppercase());
    let has_data = super::dynamic_block(d, name).is_some() || d.foreign.records.get(&key).is_some_and(|x| x.xdict.is_some());
    has_data.then(|| {
        format!("\"{name}\" already has dynamic-block data; this build can only author a fresh parameter (and, when it pairs with one, action).")
    })
}

fn parameter_object(handle: Handle, owner: Handle, p: &LinearParam) -> ForeignObject {
    let mut tags = vec![tag(5, handle.hex()), tag(330, owner.hex())];
    tags.push(tag(100, "AcDbEvalExpr"));
    tags.push(tag(90, PARAM_NODE.to_string()));
    tags.push(tag(100, "AcDbBlockElement"));
    tags.push(tag(300, "Linear1"));
    tags.push(tag(100, "AcDbBlockParameter"));
    tags.push(tag(280, "1")); // shown in the Properties palette
    tags.push(tag(100, "AcDbBlock2PtParameter"));
    tags.extend(point_tags(1010, p.base));
    tags.extend(point_tags(1011, p.end));
    tags.push(tag(177, if p.from_midpoint { "1" } else { "0" }));
    tags.push(tag(170, "2"));
    tags.push(tag(91, "10")); // base grip id (shown only when `from_midpoint`)
    tags.push(tag(91, "11")); // end grip id
    tags.push(tag(100, "AcDbBlockLinearParameter"));
    tags.push(tag(305, p.label.clone()));
    tags.push(tag(306, p.description.clone()));
    tags.push(tag(307, String::new())); // the value set's own unused description
    tags.push(tag(96, "0")); // no minimum/maximum/increment/list
    tags.push(tag(141, "0"));
    tags.push(tag(142, "0"));
    tags.push(tag(143, "0"));
    tags.push(tag(175, "0"));
    ForeignObject { handle, kind: "BLOCKLINEARPARAMETER".into(), tags }
}

fn stretch_object(handle: Handle, owner: Handle, a: &StretchAction) -> ForeignObject {
    let mut tags = vec![tag(5, handle.hex()), tag(330, owner.hex())];
    tags.push(tag(100, "AcDbEvalExpr"));
    tags.push(tag(90, ACTION_NODE.to_string()));
    tags.push(tag(100, "AcDbBlockElement"));
    tags.push(tag(300, a.name.clone()));
    tags.push(tag(100, "AcDbBlockAction"));
    tags.push(tag(70, "0"));
    tags.push(tag(71, a.selection.len().to_string()));
    for h in &a.selection {
        tags.push(tag(330, h.hex()));
    }
    let at = a.frame.iter().fold(Vec2::ZERO, |acc, p| acc + *p) * (1.0 / (a.frame.len().max(1) as f64));
    tags.extend(point_tags(1010, at.to3(0.0)));
    tags.push(tag(100, "AcDbBlockStretchAction"));
    tags.push(tag(92, PARAM_NODE.to_string()));
    tags.push(tag(301, "EndXDelta"));
    tags.push(tag(93, PARAM_NODE.to_string()));
    tags.push(tag(302, "EndYDelta"));
    tags.push(tag(72, a.frame.len().to_string()));
    for p in &a.frame {
        tags.push(tag(1011, p.x.to_string()));
        tags.push(tag(1021, p.y.to_string()));
    }
    tags.push(tag(73, "0")); // no per-vertex restriction: selection moves whole (see module doc)
    tags.push(tag(75, "0"));
    tags.push(tag(140, "1")); // multiplier
    tags.push(tag(141, "0")); // angle offset
    ForeignObject { handle, kind: "BLOCKSTRETCHACTION".into(), tags }
}

fn point_object(handle: Handle, owner: Handle, p: &PointParam) -> ForeignObject {
    let mut tags = vec![tag(5, handle.hex()), tag(330, owner.hex())];
    tags.push(tag(100, "AcDbEvalExpr"));
    tags.push(tag(90, PARAM_NODE.to_string()));
    tags.push(tag(100, "AcDbBlockElement"));
    tags.push(tag(300, "Position1"));
    tags.push(tag(100, "AcDbBlockParameter"));
    tags.push(tag(280, "1")); // shown in the Properties palette
    tags.push(tag(100, "AcDbBlock1PtParameter"));
    tags.extend(point_tags(1010, p.at));
    tags.push(tag(93, "20")); // position grip id
    tags.push(tag(100, "AcDbBlockPointParameter"));
    tags.push(tag(303, p.label.clone()));
    tags.push(tag(304, p.description.clone()));
    ForeignObject { handle, kind: "BLOCKPOINTPARAMETER".into(), tags }
}

/// `at` is the action's own glyph location in the Block Editor (not read back into
/// [`MoveAction`] — the paired Point parameter already carries the position).
fn move_object(handle: Handle, owner: Handle, at: Vec3, a: &MoveAction) -> ForeignObject {
    let mut tags = vec![tag(5, handle.hex()), tag(330, owner.hex())];
    tags.push(tag(100, "AcDbEvalExpr"));
    tags.push(tag(90, ACTION_NODE.to_string()));
    tags.push(tag(100, "AcDbBlockElement"));
    tags.push(tag(300, a.name.clone()));
    tags.push(tag(100, "AcDbBlockAction"));
    tags.push(tag(70, "0"));
    tags.push(tag(71, a.selection.len().to_string()));
    for h in &a.selection {
        tags.push(tag(330, h.hex()));
    }
    tags.extend(point_tags(1010, at));
    tags.push(tag(100, "AcDbBlockMoveAction"));
    tags.push(tag(92, PARAM_NODE.to_string()));
    tags.push(tag(301, "PosXDelta"));
    tags.push(tag(93, PARAM_NODE.to_string()));
    tags.push(tag(302, "PosYDelta"));
    tags.push(tag(140, "1")); // multiplier
    tags.push(tag(141, "0")); // angle offset
    ForeignObject { handle, kind: "BLOCKMOVEACTION".into(), tags }
}

/// A grip id is required for every state, even a block with no further authoring (AutoCAD shows
/// the triangle at this fixed position; which objects it lists comes from [`super::grips`]'s own
/// `shown`/`choices` logic, not this id).
fn visibility_object(handle: Handle, owner: Handle, p: &VisibilityParam) -> ForeignObject {
    let mut tags = vec![tag(5, handle.hex()), tag(330, owner.hex())];
    tags.push(tag(100, "AcDbEvalExpr"));
    tags.push(tag(90, PARAM_NODE.to_string()));
    tags.push(tag(100, "AcDbBlockElement"));
    tags.push(tag(300, "Visibility1"));
    tags.push(tag(100, "AcDbBlockParameter"));
    tags.push(tag(280, "1")); // shown in the Properties palette
    tags.push(tag(100, "AcDbBlock1PtParameter"));
    tags.extend(point_tags(1010, p.at));
    tags.push(tag(93, "30")); // the parameter's own list grip id
    tags.push(tag(100, "AcDbBlockVisibilityParameter"));
    tags.push(tag(301, p.label.clone()));
    tags.push(tag(302, p.description.clone()));
    tags.push(tag(93, p.controlled.len().to_string()));
    for h in &p.controlled {
        tags.push(tag(331, h.hex()));
    }
    tags.push(tag(92, p.states.len().to_string()));
    for s in &p.states {
        tags.push(tag(303, s.name.clone()));
        tags.push(tag(94, s.visible.len().to_string()));
        for h in &s.visible {
            tags.push(tag(332, h.hex()));
        }
        tags.push(tag(95, "0")); // no parameters/actions listed active only in this state
    }
    ForeignObject { handle, kind: "BLOCKVISIBILITYPARAMETER".into(), tags }
}

fn graph_object(handle: Handle, owner: Handle, members: &[Handle]) -> ForeignObject {
    let mut tags = vec![tag(5, handle.hex()), tag(330, owner.hex()), tag(100, "AcDbEvalGraph")];
    tags.extend(members.iter().map(|m| tag(360, m.hex())));
    ForeignObject { handle, kind: "ACAD_EVALUATION_GRAPH".into(), tags }
}

fn dictionary_object(handle: Handle, owner: Handle, graph: Handle) -> ForeignObject {
    let tags = vec![
        tag(5, handle.hex()),
        tag(330, owner.hex()),
        tag(100, "AcDbDictionary"),
        tag(280, "1"),
        tag(281, "1"),
        tag(3, "ACAD_ENHANCEDBLOCK"),
        tag(360, graph.hex()),
    ];
    ForeignObject { handle, kind: "DICTIONARY".into(), tags }
}

/// The BLOCK_RECORD handle authoring resolves for block `name` (minting a `Foreign::targets`
/// placeholder entry it if doesn't have one yet), and its lookup key — shared setup for every
/// `author_*` function (mirrors `dynblock::apply::record_handle`'s own use of `targets`).
fn record_and_key(d: &mut Drawing, name: &str) -> (Handle, (String, String)) {
    let key = ("BLOCK_RECORD".to_string(), name.trim().to_ascii_uppercase());
    let record_handle = d.foreign.targets.iter().find(|(_, k)| **k == key).map(|(h, _)| *h).unwrap_or_else(|| {
        let h = d.new_handle();
        std::sync::Arc::make_mut(&mut d.foreign).targets.insert(h, key.clone());
        h
    });
    (record_handle, key)
}

/// Writes the graph and xdict objects tying `members` together, plus whatever `objects` already
/// holds (the parameter and, when given, the action, already owned by `graph`), and points block
/// `key`'s xdict at them.
fn finish(d: &mut Drawing, key: (String, String), record_handle: Handle, graph: Handle, mut objects: Vec<ForeignObject>, members: Vec<Handle>) {
    let xdict = d.new_handle();
    objects.push(graph_object(graph, xdict, &members));
    objects.push(dictionary_object(xdict, record_handle, graph));
    let f = std::sync::Arc::make_mut(&mut d.foreign);
    f.objects.extend(objects);
    f.records.entry(key).or_default().xdict = Some(xdict);
}

/// Author `param` (graph id 1) and, when given, `action` (graph id 2, following the parameter's
/// end point) into block `name`'s dynamic-block data. An error, changing nothing, when the block
/// already has dynamic-block data, or `action` is given with an empty selection or fewer than two
/// frame points.
pub fn author_linear_stretch(d: &mut Drawing, name: &str, param: &LinearParam, action: Option<&StretchAction>) -> Result<(), String> {
    if let Some(why) = refusal(d, name) {
        return Err(why);
    }
    if let Some(a) = action {
        if a.selection.is_empty() {
            return Err("The Stretch action needs at least one selected object.".into());
        }
        if a.frame.len() < 2 {
            return Err("The Stretch action needs a frame of at least two points.".into());
        }
    }
    let (record_handle, key) = record_and_key(d, name);
    let graph = d.new_handle();
    let param_h = d.new_handle();
    let mut members = vec![param_h];
    let mut objects = vec![parameter_object(param_h, graph, param)];
    if let Some(a) = action {
        let action_h = d.new_handle();
        objects.push(stretch_object(action_h, graph, a));
        members.push(action_h);
    }
    finish(d, key, record_handle, graph, objects, members);
    Ok(())
}

/// Author `param` (graph id 1) and, when given, `action` (graph id 2) into block `name`'s
/// dynamic-block data, pairing a Point parameter with a Move action the same way
/// [`author_linear_stretch`] pairs a Linear parameter with a Stretch action: an error, changing
/// nothing, when the block already has dynamic-block data, or `action` has an empty selection.
pub fn author_point_move(d: &mut Drawing, name: &str, param: &PointParam, action: Option<&MoveAction>) -> Result<(), String> {
    if let Some(why) = refusal(d, name) {
        return Err(why);
    }
    if let Some(a) = action
        && a.selection.is_empty()
    {
        return Err("The Move action needs at least one selected object.".into());
    }
    let (record_handle, key) = record_and_key(d, name);
    let graph = d.new_handle();
    let param_h = d.new_handle();
    let mut members = vec![param_h];
    let mut objects = vec![point_object(param_h, graph, param)];
    if let Some(a) = action {
        let action_h = d.new_handle();
        objects.push(move_object(action_h, graph, param.at, a));
        members.push(action_h);
    }
    finish(d, key, record_handle, graph, objects, members);
    Ok(())
}

/// Author `param` (graph id 1, no paired action) into block `name`'s dynamic-block data: named
/// states that show or hide its controlled objects. An error, changing nothing, when the block
/// already has dynamic-block data, it controls no objects, or it has fewer than two states (one
/// state can't be switched to anything else, so it would author dead data).
pub fn author_visibility(d: &mut Drawing, name: &str, param: &VisibilityParam) -> Result<(), String> {
    if let Some(why) = refusal(d, name) {
        return Err(why);
    }
    if param.controlled.is_empty() {
        return Err("The Visibility parameter needs at least one controlled object.".into());
    }
    if param.states.len() < 2 {
        return Err("The Visibility parameter needs at least two states.".into());
    }
    let (record_handle, key) = record_and_key(d, name);
    let graph = d.new_handle();
    let param_h = d.new_handle();
    let objects = vec![visibility_object(param_h, graph, param)];
    finish(d, key, record_handle, graph, objects, vec![param_h]);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Block, Common, EntityKind, EntityStore, Insert, Line, Space};

    /// A drawing with block "B": two lines, (0,0)-(2,0) and (2,0)-(2,1), and a reference to it.
    fn fixture() -> (Drawing, Handle, Handle, Handle) {
        let mut d = Drawing::new_imperial();
        let h1 = d.new_handle();
        let h2 = d.new_handle();
        let mut entities = EntityStore::new();
        entities.push(crate::Entity::new(h1, EntityKind::Line(Line { a: Vec3::new(0.0, 0.0, 0.0), b: Vec3::new(2.0, 0.0, 0.0) })));
        entities.push(crate::Entity::new(h2, EntityKind::Line(Line { a: Vec3::new(2.0, 0.0, 0.0), b: Vec3::new(2.0, 1.0, 0.0) })));
        let b = Block { name: "B".into(), entities, ..Block::new("B") };
        d.blocks.insert("B".into(), std::sync::Arc::new(b));
        let ins = Insert {
            block: "B".into(),
            insert: Vec3::ZERO,
            scale: Vec3::new(1.0, 1.0, 1.0),
            rotation: 0.0,
            attribs: Vec::new(),
            cols: 1,
            rows: 1,
            col_spacing: 0.0,
            row_spacing: 0.0,
            clip: None,
        };
        let r = d.add(&Space::Model, Common::default(), EntityKind::Insert(ins)).unwrap();
        (d, h1, h2, r)
    }

    #[test]
    fn authored_data_reads_back_as_a_linear_parameter_and_stretch_action() {
        let (mut d, h1, h2, _r) = fixture();
        let param = LinearParam {
            base: Vec3::new(0.0, 0.0, 0.0),
            end: Vec3::new(2.0, 0.0, 0.0),
            label: "Distance1".into(),
            description: "Width".into(),
            from_midpoint: false,
        };
        let action = StretchAction { selection: vec![h2], frame: vec![Vec2::new(1.0, -1.0), Vec2::new(3.0, 2.0)], name: "Stretch1".into() };
        author_linear_stretch(&mut d, "B", &param, Some(&action)).unwrap();

        let m = super::super::dynamic_block(&d, "B").expect("B is now a dynamic block");
        assert_eq!(m.parameters.len(), 1);
        assert_eq!(m.actions.len(), 1);
        let p = &m.parameters[0];
        assert_eq!((p.node, p.element.as_str(), p.show_properties), (1, "Linear1", true));
        let super::super::ParameterKind::Linear { base, end, label, description, from_midpoint, .. } = &p.kind else { panic!("{p:?}") };
        assert_eq!((base.x, base.y, end.x, end.y), (0.0, 0.0, 2.0, 0.0));
        assert_eq!((label.as_str(), description.as_str(), *from_midpoint), ("Distance1", "Width", false));

        let a = &m.actions[0];
        assert_eq!((a.node, a.name.as_str()), (2, "Stretch1"));
        assert_eq!(a.selection, [h2]);
        assert_eq!(a.connections, [(1, "EndXDelta".to_string()), (1, "EndYDelta".to_string())]);
        let super::super::ActionKind::Stretch { frame, vertices, multiplier, .. } = &a.kind else { panic!("{a:?}") };
        assert_eq!(frame.len(), 2);
        assert!(vertices.is_empty(), "no per-vertex restriction: the selection moves whole");
        assert_eq!(*multiplier, 1.0);
        let _ = h1;
    }

    #[test]
    fn dragging_the_end_grip_moves_the_selected_line_and_leaves_the_other_alone() {
        use super::super::GripPart;
        let (mut d, h1, h2, r) = fixture();
        let param = LinearParam {
            base: Vec3::new(0.0, 0.0, 0.0),
            end: Vec3::new(2.0, 0.0, 0.0),
            label: "Distance1".into(),
            description: String::new(),
            from_midpoint: false,
        };
        let action = StretchAction { selection: vec![h2], frame: vec![Vec2::new(1.0, -1.0), Vec2::new(3.0, 2.0)], name: "Stretch1".into() };
        author_linear_stretch(&mut d, "B", &param, Some(&action)).unwrap();

        let g = super::super::grips(&d, r);
        let end = g.iter().find(|g| g.node == 1 && g.part == GripPart::End).expect("the end grip is shown");
        assert_eq!((end.at.x, end.at.y), (2.0, 0.0));

        super::super::drag_grip(&mut d, r, 1, GripPart::End, Vec2::new(5.0, 0.0)).unwrap();
        let name = match &d.entity(r).unwrap().kind {
            EntityKind::Insert(i) => i.block.clone(),
            _ => panic!(),
        };
        let b = d.block(&name).unwrap();
        let line_of = |h: Handle| {
            b.entities.iter().find_map(|e| if let EntityKind::Line(l) = &e.kind { Some(l.clone()) } else { None }).filter(|_| h == h1 || h == h2)
        };
        // The selected line (h2's copy) shifted by the drag (+3 in x); the other is unchanged.
        let lines: Vec<Line> = b.entities.iter().filter_map(|e| if let EntityKind::Line(l) = &e.kind { Some(l.clone()) } else { None }).collect();
        assert_eq!(lines[0], Line { a: Vec3::new(0.0, 0.0, 0.0), b: Vec3::new(2.0, 0.0, 0.0) }, "the un-selected line is unchanged");
        assert_eq!(lines[1], Line { a: Vec3::new(5.0, 0.0, 0.0), b: Vec3::new(5.0, 1.0, 0.0) }, "the selected line moved by the drag");
        let _ = line_of;
    }

    #[test]
    fn authored_point_move_data_reads_back_and_a_dragged_grip_moves_the_selected_line() {
        use super::super::GripPart;
        let (mut d, h1, h2, r) = fixture();
        let param = PointParam { at: Vec3::new(2.0, 0.0, 0.0), label: "Position1".into(), description: "Corner".into() };
        let action = MoveAction { selection: vec![h2], name: "Move1".into() };
        author_point_move(&mut d, "B", &param, Some(&action)).unwrap();

        let m = super::super::dynamic_block(&d, "B").expect("B is now a dynamic block");
        assert_eq!(m.parameters.len(), 1);
        assert_eq!(m.actions.len(), 1);
        let p = &m.parameters[0];
        assert_eq!((p.node, p.element.as_str(), p.show_properties), (1, "Position1", true));
        let super::super::ParameterKind::Point { at, label, description } = &p.kind else { panic!("{p:?}") };
        assert_eq!((at.x, at.y, label.as_str(), description.as_str()), (2.0, 0.0, "Position1", "Corner"));

        let a = &m.actions[0];
        assert_eq!((a.node, a.name.as_str()), (2, "Move1"));
        assert_eq!(a.selection, [h2]);
        let super::super::ActionKind::Move { multiplier, angle_offset } = &a.kind else { panic!("{a:?}") };
        assert_eq!((*multiplier, *angle_offset), (1.0, 0.0));

        let g = super::super::grips(&d, r);
        let at_grip = g.iter().find(|g| g.node == 1 && g.part == GripPart::At).expect("the position grip is shown");
        assert_eq!((at_grip.at.x, at_grip.at.y), (2.0, 0.0));

        super::super::drag_grip(&mut d, r, 1, GripPart::At, Vec2::new(5.0, 3.0)).unwrap();
        let name = match &d.entity(r).unwrap().kind {
            EntityKind::Insert(i) => i.block.clone(),
            _ => panic!(),
        };
        let b = d.block(&name).unwrap();
        let lines: Vec<Line> = b.entities.iter().filter_map(|e| if let EntityKind::Line(l) = &e.kind { Some(l.clone()) } else { None }).collect();
        // Dragging (2,0) to (5,3) is a +3,+3 move, applied to the selected line (h2's copy) only.
        assert_eq!(lines[0], Line { a: Vec3::new(0.0, 0.0, 0.0), b: Vec3::new(2.0, 0.0, 0.0) }, "the un-selected line is unchanged");
        assert_eq!(lines[1], Line { a: Vec3::new(5.0, 3.0, 0.0), b: Vec3::new(5.0, 4.0, 0.0) }, "the selected line moved by the drag");
        let _ = h1;
    }

    #[test]
    fn a_block_that_already_has_dynamic_block_data_refuses_a_second_author_call() {
        let (mut d, _h1, h2, _r) = fixture();
        let param = LinearParam {
            base: Vec3::ZERO,
            end: Vec3::new(2.0, 0.0, 0.0),
            label: "Distance1".into(),
            description: String::new(),
            from_midpoint: false,
        };
        author_linear_stretch(&mut d, "B", &param, None).unwrap();
        let point = PointParam { at: Vec3::new(2.0, 0.0, 0.0), label: "Position1".into(), description: String::new() };
        let action = MoveAction { selection: vec![h2], name: "Move1".into() };
        let err = author_point_move(&mut d, "B", &point, Some(&action)).unwrap_err();
        assert!(err.contains("already has dynamic-block data"), "{err}");
    }

    #[test]
    fn authored_visibility_data_reads_back_and_set_property_hides_the_controlled_line() {
        let (mut d, h1, h2, r) = fixture();
        let param = VisibilityParam {
            at: Vec3::new(2.0, 0.0, 0.0),
            label: "Visibility1".into(),
            description: String::new(),
            controlled: vec![h2],
            states: vec![VisibilityStateDef { name: "On".into(), visible: vec![h2] }, VisibilityStateDef { name: "Off".into(), visible: Vec::new() }],
        };
        author_visibility(&mut d, "B", &param).unwrap();

        let m = super::super::dynamic_block(&d, "B").expect("B is now a dynamic block");
        assert_eq!(m.parameters.len(), 1);
        assert_eq!(m.actions.len(), 0, "a Visibility parameter authors with no paired action");
        let p = &m.parameters[0];
        assert_eq!((p.node, p.element.as_str(), p.show_properties), (1, "Visibility1", true));
        let super::super::ParameterKind::Visibility { label, controlled, states, .. } = &p.kind else { panic!("{p:?}") };
        assert_eq!(label, "Visibility1");
        assert_eq!(controlled, &[h2]);
        assert_eq!(states.len(), 2);
        assert_eq!((states[0].name.as_str(), states[0].visible.as_slice()), ("On", [h2].as_slice()));
        assert_eq!((states[1].name.as_str(), states[1].visible.as_slice()), ("Off", [].as_slice()));

        let (_, props) = super::super::custom_properties(&d, r).expect("custom properties");
        let prop = props.iter().find(|p| p.name == "Visibility1").expect("Visibility1 property");
        assert_eq!(prop.value, "On");
        assert_eq!(prop.choices, ["On", "Off"]);

        super::super::set_property(&mut d, r, "Visibility1", "Off").unwrap();
        let name = match &d.entity(r).unwrap().kind {
            EntityKind::Insert(i) => i.block.clone(),
            _ => panic!(),
        };
        let b = d.block(&name).unwrap();
        let controlled_copy =
            b.entities.iter().find(|e| matches!(&e.kind, EntityKind::Line(l) if l.a.x == 2.0 && l.b.y == 1.0)).expect("the controlled line's copy");
        assert!(!controlled_copy.common.visible, "the controlled line is hidden in the Off state");
        let other = b.entities.iter().find(|e| matches!(&e.kind, EntityKind::Line(l) if l.a.x == 0.0)).expect("the other line's copy");
        assert!(other.common.visible, "the uncontrolled line stays visible");
        let _ = h1;
    }
}

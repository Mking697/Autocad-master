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

/// Why [`author_linear_stretch`] would refuse to author into block `name`, if it would.
fn refusal(d: &Drawing, name: &str) -> Option<String> {
    let key = ("BLOCK_RECORD".to_string(), name.trim().to_ascii_uppercase());
    let has_data = super::dynamic_block(d, name).is_some() || d.foreign.records.get(&key).is_some_and(|x| x.xdict.is_some());
    has_data.then(|| format!("\"{name}\" already has dynamic-block data; this build can only author a fresh Linear parameter and Stretch action."))
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
    let key = ("BLOCK_RECORD".to_string(), name.trim().to_ascii_uppercase());
    // The owner the written file resolves for us: the generic writer remaps a kept object's
    // owner (330) through `Foreign::targets`, so the dictionary's owner reads back as the real
    // BLOCK_RECORD even though we never see its eventual handle (mirrors
    // `dynblock::apply::record_handle`'s own use of `targets` for the same reason).
    let record_handle = d.foreign.targets.iter().find(|(_, k)| **k == key).map(|(h, _)| *h).unwrap_or_else(|| {
        let h = d.new_handle();
        std::sync::Arc::make_mut(&mut d.foreign).targets.insert(h, key.clone());
        h
    });
    let xdict = d.new_handle();
    let graph = d.new_handle();
    let param_h = d.new_handle();
    let mut members = vec![param_h];
    let mut objects = vec![parameter_object(param_h, graph, param)];
    if let Some(a) = action {
        let action_h = d.new_handle();
        objects.push(stretch_object(action_h, graph, a));
        members.push(action_h);
    }
    objects.push(graph_object(graph, xdict, &members));
    objects.push(dictionary_object(xdict, record_handle, graph));
    let f = std::sync::Arc::make_mut(&mut d.foreign);
    f.objects.extend(objects);
    f.records.entry(key).or_default().xdict = Some(xdict);
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
}

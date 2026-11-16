//! PICKSTYLE: with group selection on (bit 1, the default), picking or windowing an object of a
//! selectable group selects the whole group. Ctrl+Shift+A (the `pickstyle` command without a
//! value) turns group selection on and off.

use std::collections::{HashMap, HashSet};

use cadcraft_doc::{EntityKind, Handle};
use serde_json::{Value, json};

use super::super::{CommandSpec, bad};

use super::super::always;
use crate::{Accept, Input, Interactive, Prompt, Result, Session, Step};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("pickstyle", "Group Selection", run)
            .key("Cmd+Shift+A")
            .params("{value?: 0 none | 1 group selection | 2 associative hatch | 3 both} (no value: group selection on/off) → {value, message}")
            .enabled(always)
            .noundo()
            .transparent()
            .interactive(|s| Ok(Box::new(PickstyleM { current: s.settings.pickstyle }))),
    ]
}

/// The commands whose object prompts pick single members, not whole groups.
const MEMBER_PICKS: &[&str] = &["groupedit", "-group"];

/// `hs` plus the other members of the selectable groups they belong to (in the space being
/// edited, visible) when PICKSTYLE bit 1 (group selection) is on, then plus the boundary
/// objects of any associative HATCH in the result when PICKSTYLE bit 2 (associative hatch) is
/// on. Order: `hs` first.
pub(crate) fn expand(s: &Session, hs: Vec<Handle>) -> Vec<Handle> {
    if hs.is_empty() {
        return hs;
    }
    let pickstyle = s.settings.pickstyle;
    let member_pick = s.running.as_ref().is_some_and(|r| MEMBER_PICKS.contains(&r.id.as_str()));
    let out = if pickstyle & 1 != 0 && !member_pick { expand_groups(s, hs) } else { hs };
    if pickstyle & 2 != 0 { expand_assoc_hatch(s, out) } else { out }
}

/// `hs` plus the other members of the selectable groups they belong to (in the space being
/// edited, visible). Order: `hs` first.
fn expand_groups(s: &Session, hs: Vec<Handle>) -> Vec<Handle> {
    let Ok(d) = s.doc() else { return hs };
    if !d.groups.iter().any(|g| g.selectable) {
        return hs;
    }
    let space = s.space();
    let Some(store) = d.space(&space) else { return hs };
    // Member → its selectable groups, built once (window selections may hold many objects).
    let mut of: HashMap<Handle, Vec<usize>> = HashMap::new();
    for (i, g) in d.groups.iter().enumerate().filter(|(_, g)| g.selectable) {
        for m in &g.members {
            of.entry(*m).or_default().push(i);
        }
    }
    let mut seen: HashSet<Handle> = hs.iter().copied().collect();
    let mut done: HashSet<usize> = HashSet::new();
    let mut out = hs.clone();
    for h in &hs {
        if !store.get(*h).is_some_and(|e| expansion_eligible(d, e)) {
            continue;
        }
        for gi in of.get(h).into_iter().flatten() {
            if !done.insert(*gi) {
                continue;
            }
            let Some(g) = d.groups.get(*gi) else { continue };
            for m in &g.members {
                if store.get(*m).is_some_and(|e| expansion_eligible(d, e)) && seen.insert(*m) {
                    out.push(*m);
                }
            }
        }
    }
    out
}

/// Objects added by expansion must be visible and on an unlocked layer. Current-space
/// membership is checked through the space's store by each caller. Keep original picks intact.
fn expansion_eligible(d: &cadcraft_doc::Drawing, e: &cadcraft_doc::Entity) -> bool {
    d.is_visible(e) && d.layer(&e.common.layer).is_none_or(|l| !l.locked)
}

/// `hs` plus the boundary objects of associative HATCHes in the input, not recursively through
/// newly appended sources. Picking a boundary alone does not pull in its hatch. Order: `hs`
/// first. This deliberately does not claim bidirectional reference-application parity.
fn expand_assoc_hatch(s: &Session, hs: Vec<Handle>) -> Vec<Handle> {
    let Ok(d) = s.doc() else { return hs };
    let space = s.space();
    let Some(store) = d.space(&space) else { return hs };
    let mut seen: HashSet<Handle> = hs.iter().copied().collect();
    let mut out = hs.clone();
    for h in &hs {
        let Some(e) = store.get(*h).filter(|e| expansion_eligible(d, e)) else { continue };
        let EntityKind::Hatch(hatch) = &e.kind else { continue };
        if !hatch.associative {
            continue;
        }
        for src in hatch.loops.iter().flat_map(|l| &l.sources) {
            if store.get(*src).is_some_and(|se| expansion_eligible(d, se)) && seen.insert(*src) {
                out.push(*src);
            }
        }
    }
    out
}

fn run(s: &mut Session, p: &Value) -> Result<Value> {
    let v = match p.get("value") {
        None | Some(Value::Null) => s.settings.pickstyle ^ 1,
        Some(v) => v
            .as_u64()
            .or_else(|| v.as_str().and_then(|t| t.trim().parse().ok()))
            .filter(|v| *v <= 3)
            .and_then(|v| u8::try_from(v).ok())
            .ok_or_else(|| bad("pickstyle", "`value` must be 0, 1, 2 or 3"))?,
    };
    s.settings.pickstyle = v;
    let msg = format!("<Group selection {}>", if v & 1 != 0 { "on" } else { "off" });
    Ok(json!({ "value": v, "message": msg }))
}

/// Typed PICKSTYLE: "Enter new value for PICKSTYLE <1>:".
struct PickstyleM {
    current: u8,
}

impl Interactive for PickstyleM {
    fn name(&self) -> &'static str {
        "PICKSTYLE"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        Prompt::new("Enter new value for PICKSTYLE", Accept::NUMBER).default(self.current.to_string())
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        match i {
            Input::Enter => Ok(Step::Done),
            Input::Text(t) => match run(s, &json!({ "value": t })) {
                Ok(_) => Ok(Step::Done),
                Err(_) => {
                    s.echo("Requires an integer between 0 and 3.");
                    Ok(Step::Continue)
                }
            },
            _ => Ok(Step::Continue),
        }
    }
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    use cadcraft_doc::{Common, Hatch, HatchLoop, Line, Space};
    use cadcraft_geom::{Vec2, Vec3};

    fn line(s: &mut Session, space: &Space, common: Common) -> Handle {
        s.doc_mut().unwrap().add(space, common, EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(1.0, 0.0, 0.0) })).unwrap()
    }

    fn hatch(s: &mut Session, sources: Vec<Handle>) -> Handle {
        let h = Hatch {
            pattern: "SOLID".into(),
            solid: true,
            loops: vec![HatchLoop { vertices: vec![], outer: true, sources }],
            scale: 1.0,
            angle: 0.0,
            associative: true,
            style: 0,
            elevation: 0.0,
            gradient: None,
            origin: Vec2::ZERO,
            background: None,
            pattern_lines: vec![],
        };
        s.doc_mut().unwrap().add(&Space::Model, Common::default(), EntityKind::Hatch(h)).unwrap()
    }

    #[test]
    fn pickstyle_bits_direction_and_nonassociative_hatches() {
        let mut s = Session::new();
        let boundary = line(&mut s, &Space::Model, Common::default());
        let h = hatch(&mut s, vec![boundary]);
        for bit in [0, 1] {
            s.settings.pickstyle = bit;
            assert_eq!(expand(&s, vec![h]), vec![h]);
        }
        for bit in [2, 3] {
            s.settings.pickstyle = bit;
            assert_eq!(expand(&s, vec![h]), vec![h, boundary]);
            assert_eq!(expand(&s, vec![boundary]), vec![boundary], "boundary alone does not select its hatch");
        }
        s.doc_mut()
            .unwrap()
            .modify_entity(h, |e| {
                if let EntityKind::Hatch(h) = &mut e.kind {
                    h.associative = false;
                }
            })
            .unwrap();
        assert_eq!(expand(&s, vec![h]), vec![h]);
    }

    #[test]
    fn pickstyle_idle_click_uses_hatch_bit() {
        let mut s = Session::new();
        let boundary = line(&mut s, &Space::Model, Common::default());
        let h = hatch(&mut s, vec![boundary]);
        s.doc_mut()
            .unwrap()
            .modify_entity(h, |e| {
                if let EntityKind::Hatch(h) = &mut e.kind {
                    h.loops[0].vertices = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]
                        .into_iter()
                        .map(|(x, y)| cadcraft_geom::PolyVertex::new(Vec2::new(x, y)))
                        .collect();
                }
            })
            .unwrap();
        for bit in [0, 2] {
            s.execute("pickstyle", &json!({ "value": bit })).unwrap();
            s.set_selection(vec![]);
            s.idle_click(Vec2::new(5.0, 5.0), false).unwrap();
            assert_eq!(s.selection(), if bit == 2 { vec![h, boundary] } else { vec![h] });
        }
    }

    #[test]
    fn pickstyle_group_bit_and_selectable_flag_remain_independent() {
        let mut s = Session::new();
        let boundary = line(&mut s, &Space::Model, Common::default());
        let h = hatch(&mut s, vec![boundary]);
        let member = line(&mut s, &Space::Model, Common::default());
        s.doc_mut().unwrap().groups.push(cadcraft_doc::Group {
            name: "A".into(),
            description: String::new(),
            selectable: true,
            members: vec![member, h],
        });
        s.settings.pickstyle = 1;
        assert_eq!(expand(&s, vec![member]), vec![member, h]);
        s.settings.pickstyle = 2;
        assert_eq!(expand(&s, vec![member]), vec![member]);
        s.settings.pickstyle = 3;
        assert_eq!(expand(&s, vec![member]), vec![member, h, boundary]);
        s.doc_mut().unwrap().groups[0].selectable = false;
        assert_eq!(expand(&s, vec![member]), vec![member]);
        assert_eq!(expand(&s, vec![h]), vec![h, boundary]);
        assert_eq!(run(&mut s, &json!({})).unwrap()["value"], 2, "toggle preserves hatch bit");
    }

    #[test]
    fn pickstyle_hatch_expansion_excludes_locked_hidden_missing_and_other_space() {
        let mut s = Session::new();
        let visible = line(&mut s, &Space::Model, Common::default());
        let locked = line(&mut s, &Space::Model, Common { layer: "Locked".into(), ..Common::default() });
        s.doc_mut().unwrap().layer_mut("Locked").unwrap().locked = true;
        let hidden = line(&mut s, &Space::Model, Common { visible: false, ..Common::default() });
        let paper = line(&mut s, &Space::Paper("Layout1".into()), Common::default());
        let h = hatch(&mut s, vec![visible, visible, locked, hidden, paper, Handle(u64::MAX)]);
        s.settings.pickstyle = 2;
        assert_eq!(expand(&s, vec![h]), vec![h, visible]);
        s.doc_mut().unwrap().groups.push(cadcraft_doc::Group {
            name: "constraints".into(),
            description: String::new(),
            selectable: true,
            members: vec![h, visible, locked, hidden, paper],
        });
        s.settings.pickstyle = 1;
        assert_eq!(expand(&s, vec![h]), vec![h, visible]);
    }

    #[test]
    fn pickstyle_ignores_hatch_seeds_outside_current_space_or_hidden() {
        let mut s = Session::new();
        let boundary = line(&mut s, &Space::Model, Common::default());
        let h = hatch(&mut s, vec![boundary]);
        let kind = s.doc().unwrap().entity(h).unwrap().kind.clone();
        let paper = s.doc_mut().unwrap().add(&Space::Paper("Layout1".into()), Common::default(), kind).unwrap();
        s.settings.pickstyle = 2;
        assert_eq!(expand(&s, vec![paper]), vec![paper]);
        s.doc_mut().unwrap().modify_entity(h, |e| e.common.visible = false).unwrap();
        assert_eq!(expand(&s, vec![h]), vec![h]);
    }

    #[test]
    fn pickstyle_does_not_follow_cyclic_hatch_sources_recursively() {
        let mut s = Session::new();
        let a = hatch(&mut s, vec![]);
        let b = hatch(&mut s, vec![a]);
        s.doc_mut()
            .unwrap()
            .modify_entity(a, |e| {
                if let EntityKind::Hatch(h) = &mut e.kind {
                    h.loops[0].sources = vec![a, b, b];
                }
            })
            .unwrap();
        s.settings.pickstyle = 2;
        assert_eq!(expand(&s, vec![a]), vec![a, b]);
    }
}

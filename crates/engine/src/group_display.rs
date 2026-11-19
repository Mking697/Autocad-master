//! GROUPDISPLAYMODE (#417 part 3): how a *whole group* selection shows grips, instead of the
//! normal per-member grips ([`crate::grips`]). AutoCAD's values: 0 grips on every member of the
//! group (no special display at all — the same as any other multi-object selection), 1 a single
//! grip at the group's centroid, 2 (the default) the group's bounding box *and* that centroid
//! grip.
//!
//! This only ever replaces the grips of a selection that is *exactly* the full, existing
//! membership of one selectable group — a partial selection, or a selection spanning more than
//! one group or extra loose entities, keeps the ordinary per-object grips.

use std::collections::HashSet;

use cadcraft_doc::{Drawing, Group, Handle, entity_bounds};
use cadcraft_geom::{Bounds2, Vec2};

use crate::Session;

/// What to show instead of per-member grips for a whole-group selection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroupDisplay {
    /// The group's combined bounding box (GROUPDISPLAYMODE 2 only).
    pub bounds: Option<Bounds2>,
    /// The single grip at the group's centroid (GROUPDISPLAYMODE 1 and 2).
    pub grip: Option<Vec2>,
}

/// The selectable group (if any) whose currently-existing membership is exactly `selected`.
fn whole_group<'a>(d: &'a Drawing, selected: &[Handle]) -> Option<&'a Group> {
    if selected.is_empty() {
        return None;
    }
    let set: HashSet<Handle> = selected.iter().copied().collect();
    d.groups.iter().find(|g| {
        if !g.selectable {
            return false;
        }
        let members: HashSet<Handle> = g.members.iter().copied().filter(|h| d.entity(*h).is_some()).collect();
        !members.is_empty() && members == set
    })
}

/// The combined bounding box of `g`'s existing members.
fn group_bounds(d: &Drawing, g: &Group) -> Bounds2 {
    g.members.iter().filter_map(|h| d.entity(*h)).fold(Bounds2::EMPTY, |b, e| b.union(&entity_bounds(d, e, 0)))
}

impl Session {
    /// Group display data for `selected` per GROUPDISPLAYMODE, or `None` when the selection
    /// isn't exactly one whole, selectable group, the group's bounds are empty, or the mode (0)
    /// calls for ordinary per-member grips.
    pub fn group_display(&self, selected: &[Handle]) -> Option<GroupDisplay> {
        if self.settings.groupdisplaymode == 0 {
            return None;
        }
        let d = self.doc().ok()?;
        let g = whole_group(d, selected)?;
        let bounds = group_bounds(d, g);
        if bounds.is_empty() {
            return None;
        }
        Some(GroupDisplay { bounds: (self.settings.groupdisplaymode >= 2).then_some(bounds), grip: Some(bounds.center()) })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::cmd::groups;

    fn session() -> Session {
        let mut s = Session::new();
        s.execute("line", &json!({ "points": [[0, 0], [10, 0]] })).unwrap_or_default();
        s.execute("line", &json!({ "points": [[0, 0], [0, 10]] })).unwrap_or_default();
        s.execute("circle", &json!({ "center": [20, 20], "radius": 2 })).unwrap_or_default();
        s.set_selection(Vec::new());
        s
    }

    fn handles(s: &Session) -> Vec<Handle> {
        s.doc().unwrap().model.iter().map(|e| e.handle).collect()
    }

    #[test]
    fn mode_zero_never_shows_group_display() {
        let mut s = session();
        let hs = handles(&s);
        groups::create(&mut s, Some("G1"), "", &hs[..2], true).unwrap();
        s.settings.groupdisplaymode = 0;
        assert_eq!(s.group_display(&hs[..2]), None);
    }

    #[test]
    fn mode_one_gives_a_centroid_grip_but_no_bounds() {
        let mut s = session();
        let hs = handles(&s);
        groups::create(&mut s, Some("G1"), "", &hs[..2], true).unwrap();
        s.settings.groupdisplaymode = 1;
        let gd = s.group_display(&hs[..2]).expect("a whole-group selection shows group display");
        assert_eq!(gd.bounds, None);
        let grip = gd.grip.expect("a centroid grip");
        assert!((grip.x - 5.0).abs() < 1e-9 && (grip.y - 5.0).abs() < 1e-9, "{grip:?}");
    }

    #[test]
    fn mode_two_gives_both_bounds_and_centroid_grip() {
        let mut s = session();
        let hs = handles(&s);
        groups::create(&mut s, Some("G1"), "", &hs[..2], true).unwrap();
        s.settings.groupdisplaymode = 2;
        let gd = s.group_display(&hs[..2]).expect("a whole-group selection shows group display");
        let b = gd.bounds.expect("bounds in mode 2");
        assert_eq!((b.min.x, b.min.y, b.max.x, b.max.y), (0.0, 0.0, 10.0, 10.0));
        assert!(gd.grip.is_some());
    }

    #[test]
    fn a_partial_group_selection_keeps_ordinary_grips() {
        let mut s = session();
        let hs = handles(&s);
        groups::create(&mut s, Some("G1"), "", &hs, true).unwrap(); // group of all 3
        s.settings.groupdisplaymode = 2;
        // Only 2 of the 3 members selected: not the whole group.
        assert_eq!(s.group_display(&hs[..2]), None);
    }

    #[test]
    fn a_selection_spanning_a_group_plus_extra_entities_keeps_ordinary_grips() {
        let mut s = session();
        let hs = handles(&s);
        groups::create(&mut s, Some("G1"), "", &hs[..2], true).unwrap();
        s.settings.groupdisplaymode = 2;
        assert_eq!(s.group_display(&hs), None, "the circle isn't in the group");
    }

    #[test]
    fn a_non_selectable_group_keeps_ordinary_grips() {
        let mut s = session();
        let hs = handles(&s);
        groups::create(&mut s, Some("G1"), "", &hs[..2], false).unwrap();
        s.settings.groupdisplaymode = 2;
        assert_eq!(s.group_display(&hs[..2]), None);
    }

    #[test]
    fn an_erased_member_is_dropped_from_the_match() {
        let mut s = session();
        let hs = handles(&s);
        groups::create(&mut s, Some("G1"), "", &hs[..2], true).unwrap();
        s.set_selection(vec![hs[0]]);
        s.execute("erase", &json!({ "handles": [hs[1].hex()] })).unwrap();
        // after_command drops the erased member from the group, leaving one: selecting just that
        // one object is now the whole (shrunk) group.
        s.settings.groupdisplaymode = 2;
        assert!(s.group_display(&[hs[0]]).is_some());
    }

    #[test]
    fn no_group_at_all_keeps_ordinary_grips() {
        let s = session();
        let hs = handles(&s);
        assert_eq!(s.group_display(&hs[..1]), None);
    }
}

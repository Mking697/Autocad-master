//! Boundary merge for UNION/SUBTRACT/INTERSECT when two regions' boundaries touch (share
//! vertices or whole edges) without crossing into each other's interior.
//!
//! The regions are already tessellated to straight edges. Edges that coincide between the two
//! regions (or merely share a vertex) are the only "meeting" this module handles: each unique
//! edge is sampled just off both sides to see which side (if any) the operator's result covers,
//! kept — oriented with the result on its left — when the two sides differ, and the kept edges
//! are traced into closed loops the same way a planar graph's faces are traced (always the
//! sharpest clockwise turn). A proper crossing (the boundaries cut through each other's
//! interior) or a partial collinear overlap is not handled here; the caller checks for those
//! first and reports "not available yet" instead of calling this module.

use std::collections::{HashMap, HashSet};

use cadcraft_geom::{Bounds2, Line, TAU, Vec2, line_line, norm_angle};

use super::{Op, inside};

/// Most unique edges considered (hostile input: very fine tessellation).
const MAX_EDGES: usize = 200_000;
/// Most loops traced out of one merge (hostile input: pathological touching configurations).
const MAX_TRACE_LOOPS: usize = 10_000;

/// Whether `a` and `b`'s edges include a proper crossing (an intersection strictly inside both
/// segments, more than `tol` from every endpoint) or a collinear overlap that doesn't match
/// endpoint-for-endpoint. Either needs curve splitting this module doesn't do.
pub fn has_unsupported_crossing(a: &[Vec<Vec2>], b: &[Vec<Vec2>], tol: f64) -> bool {
    let edges = |ls: &[Vec<Vec2>]| -> Vec<Line> {
        ls.iter().flat_map(|l| (0..l.len()).filter_map(move |i| Some(Line::new(*l.get(i)?, *l.get((i + 1) % l.len())?)))).collect()
    };
    let (ea, eb) = (edges(a), edges(b));
    for x in &ea {
        if x.len() < tol {
            continue;
        }
        for y in &eb {
            if y.len() < tol {
                continue;
            }
            if let Some(p) = line_line(x, y)
                && p.dist(x.a) > tol
                && p.dist(x.b) > tol
                && p.dist(y.a) > tol
                && p.dist(y.b) > tol
            {
                return true;
            }
            if collinear_partial_overlap(x, y, tol) {
                return true;
            }
        }
    }
    false
}

/// Whether `x` and `y` are collinear and overlap over more than a point without matching
/// endpoint-for-endpoint (that case is a shared edge, which the merge handles directly).
fn collinear_partial_overlap(x: &Line, y: &Line, tol: f64) -> bool {
    let dir = (x.b - x.a).normalized();
    if dir == Vec2::ZERO {
        return false;
    }
    let perp = |p: Vec2| (p - x.a).cross(dir).abs();
    if perp(y.a) > tol || perp(y.b) > tol {
        return false; // not collinear with x
    }
    let same_pair = (x.a.near(y.a, tol) && x.b.near(y.b, tol)) || (x.a.near(y.b, tol) && x.b.near(y.a, tol));
    if same_pair {
        return false; // exact shared edge
    }
    let t = |p: Vec2| (p - x.a).dot(dir);
    let (tx0, tx1) = (0.0_f64, x.len());
    let (mut ty0, mut ty1) = (t(y.a), t(y.b));
    if ty0 > ty1 {
        std::mem::swap(&mut ty0, &mut ty1);
    }
    (tx1.min(ty1) - tx0.max(ty0)) > tol
}

fn qkey(p: Vec2, origin: Vec2, q: f64) -> (i64, i64) {
    (((p.x - origin.x) / q).round() as i64, ((p.y - origin.y) / q).round() as i64)
}

struct Verts {
    pts: Vec<Vec2>,
    index: HashMap<(i64, i64), usize>,
    origin: Vec2,
    q: f64,
}

impl Verts {
    fn id(&mut self, p: Vec2) -> usize {
        let k = qkey(p, self.origin, self.q);
        *self.index.entry(k).or_insert_with(|| {
            self.pts.push(p);
            self.pts.len() - 1
        })
    }
}

/// One undirected edge from the tessellated input, with which region(s) contributed it.
struct EdgeInfo {
    u: usize,
    v: usize,
    from_a: bool,
    from_b: bool,
}

/// `a` op `b`'s boundary loops, for loops that only touch (no proper crossing); `None` when
/// tracing can't close a loop (hostile/degenerate input — the caller falls back to reporting
/// the combine as not available).
pub fn merge(ta: &[Vec<Vec2>], tb: &[Vec<Vec2>], op: Op, tol: f64) -> Option<Vec<Vec<Vec2>>> {
    let all_pts = ta.iter().chain(tb).flatten().copied().filter(|p| p.is_finite());
    let bb = Bounds2::from_points(all_pts);
    if bb.is_empty() {
        return None;
    }
    let count = ta.iter().chain(tb).try_fold(0usize, |n, l| n.checked_add(l.len()))?;
    if count.saturating_mul(count) > super::MAX_PAIRS || !tol.is_finite() || tol <= 0.0 {
        return None;
    }
    let q = tol.max(1e-12);
    let mut verts = Verts { pts: Vec::new(), index: HashMap::new(), origin: bb.min, q };
    let mut edge_index: HashMap<(usize, usize), usize> = HashMap::new();
    let mut edges: Vec<EdgeInfo> = Vec::new();
    let add = |loops: &[Vec<Vec2>],
               from_a: bool,
               verts: &mut Verts,
               edges: &mut Vec<EdgeInfo>,
               edge_index: &mut HashMap<(usize, usize), usize>|
     -> Option<()> {
        for l in loops {
            let n = l.len();
            for i in 0..n {
                let (p, p2) = (*l.get(i)?, *l.get((i + 1) % n)?);
                if !p.is_finite() || !p2.is_finite() || p.near(p2, tol) {
                    continue;
                }
                let (u, v) = (verts.id(p), verts.id(p2));
                if u == v {
                    continue;
                }
                let key = (u.min(v), u.max(v));
                match edge_index.get(&key) {
                    Some(&i) => {
                        if let Some(e) = edges.get_mut(i) {
                            if from_a {
                                e.from_a = true;
                            } else {
                                e.from_b = true;
                            }
                        }
                    }
                    None => {
                        if edges.len() >= MAX_EDGES {
                            return None;
                        }
                        edge_index.insert(key, edges.len());
                        edges.push(EdgeInfo { u, v, from_a, from_b: !from_a });
                    }
                }
            }
        }
        Some(())
    };
    add(ta, true, &mut verts, &mut edges, &mut edge_index)?;
    add(tb, false, &mut verts, &mut edges, &mut edge_index)?;

    let result = |in_a: bool, in_b: bool| match op {
        Op::Union => in_a || in_b,
        Op::Subtract => in_a && !in_b,
        Op::Intersect => in_a && in_b,
    };

    // Directed kept edges: `from -> list of to`, with interior of the op's result on the left.
    let mut out_edges: HashMap<usize, Vec<usize>> = HashMap::new();
    for e in &edges {
        let (p, pv) = (verts.pts.get(e.u).copied()?, verts.pts.get(e.v).copied()?);
        let dir = (pv - p).normalized();
        if dir == Vec2::ZERO {
            continue;
        }
        let left = dir.perp();
        let len = p.dist(pv);
        let eps = (tol * 0.25).min(len * 0.1);
        let mid = p.mid(pv);
        let (sl, sr) = (mid + left * eps, mid - left * eps);
        let rl = result(inside(ta, sl), inside(tb, sl));
        let rr = result(inside(ta, sr), inside(tb, sr));
        if rl == rr {
            continue; // interior wall or outside both ways: not a result boundary
        }
        let (from, to) = if rl { (e.u, e.v) } else { (e.v, e.u) };
        out_edges.entry(from).or_default().push(to);
    }

    let mut visited: HashSet<(usize, usize)> = HashSet::new();
    let mut loops: Vec<Vec<Vec2>> = Vec::new();
    let mut starts: Vec<(usize, usize)> = out_edges.iter().flat_map(|(&u, vs)| vs.iter().map(move |&v| (u, v))).collect();
    starts.sort_unstable();
    // Total turn-following steps across every trace, bounded by a small multiple of the kept
    // edges: a well-formed cycle uses each edge once, so this only trips on hostile input.
    let mut budget = starts.len().saturating_mul(4).max(16);
    for (u0, v0) in starts {
        if visited.contains(&(u0, v0)) {
            continue;
        }
        if loops.len() >= MAX_TRACE_LOOPS {
            return None;
        }
        let mut ids = vec![u0];
        let mut used = vec![(u0, v0)];
        let (mut u, mut v) = (u0, v0);
        let mut closed = false;
        loop {
            ids.push(v);
            if budget == 0 {
                break;
            }
            budget -= 1;
            let (Some(pv), Some(pu)) = (verts.pts.get(v).copied(), verts.pts.get(u).copied()) else { break };
            let back = (pu - pv).angle();
            let mut best: Option<(f64, usize)> = None;
            for &w in out_edges.get(&v).into_iter().flatten() {
                let Some(pw) = verts.pts.get(w).copied() else { continue };
                let a = (pw - pv).angle();
                let mut cw = norm_angle(back - a);
                if cw < 1e-12 {
                    cw = TAU;
                }
                if best.is_none_or(|(b, _)| cw < b) {
                    best = Some((cw, w));
                }
            }
            let Some((_, w)) = best else { break };
            u = v;
            v = w;
            used.push((u, v));
            if u == u0 && v == v0 {
                ids.pop();
                closed = true;
                break;
            }
        }
        if !closed || ids.len() < 3 {
            return None;
        }
        visited.extend(used);
        let pts = ids.iter().map(|&i| verts.pts.get(i).copied()).collect::<Option<Vec<_>>>()?;
        loops.push(pts);
    }
    Some(loops)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x0: f64, y0: f64, s: f64) -> Vec<Vec2> {
        vec![Vec2::new(x0, y0), Vec2::new(x0 + s, y0), Vec2::new(x0 + s, y0 + s), Vec2::new(x0, y0 + s)]
    }

    #[test]
    fn union_of_two_adjacent_squares_merges() {
        let ta = vec![square(0.0, 0.0, 1.0)];
        let tb = vec![square(1.0, 0.0, 1.0)];
        let out = merge(&ta, &tb, Op::Union, 1e-9).unwrap();
        println!("UNION loops: {out:?}");
        assert_eq!(out.len(), 1);
    }
}

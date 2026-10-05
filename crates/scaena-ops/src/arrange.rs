//! Several nodes at once (PLAN 2.42, ADR-0013): moved together, aligned, spread, and put in
//! front of or behind one another, each one patch.
//!
//! - **Together.** The first node moves as a drag moves it; the rest move as far as it did,
//!   each snapped its own way: by cells on a grid, to whole canvas units where a `rect` places
//!   it or a frame holds it. One in a stack stays where it is.
//! - **Aligned.** Each node's box takes the edge, or the middle, that the boxes of all of them
//!   share at its extreme: the leftmost left, the rightmost right, the middle of them all; each
//!   snapped as a move snaps, so on a grid they line up on its tracks.
//! - **Spread.** The first and the last stay; the ones between move so that the gaps between
//!   them all are equal, as near as the tracks allow.
//! - **Ordered.** A node is painted after its container's other children by `z`, then by where
//!   the deck lists it (SPEC §3.2). Forward puts it in front of the next of them that it
//!   overlaps, backward behind the one before; front and back past all of them. Each is the
//!   one `z` that does it where there is one, else the fewest that do.
//! - **Listed** (PLAN 2.50). A node goes just before or after one of its container's other
//!   children as a layers panel lists them (`scaena_core::layers`): over it or under it, by
//!   the `z` that does it as an order does; in a stack, before it or after it in the order the
//!   stack lays them out, each child whose `at.index` that changes renumbered.

use crate::{Context, OpsError};
use scaena_core::patch::SemanticOp;
use scaena_core::{Deck, Snapshot};
use scaena_engine::geometry::{By, NodeBox, Snap, Target, Targets};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

type Rect = [f32; 4];

/// The edge, or the middle, that nodes aligned share.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    Left,
    Center,
    Right,
    Top,
    Middle,
    Bottom,
}

/// The way nodes spread run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Spread {
    Across,
    Down,
}

/// Where a node goes among what its container paints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Order {
    /// In front of the next it overlaps.
    Forward,
    /// Behind the one before it that it overlaps.
    Backward,
    /// In front of all of them.
    Front,
    /// Behind all of them.
    Back,
}

/// Each way of arranging is read from its name, as the CLI takes it.
macro_rules! named {
    ($($ty:ty),*) => {$(
        impl std::str::FromStr for $ty {
            type Err = String;

            fn from_str(s: &str) -> Result<$ty, String> {
                serde_json::from_value(Value::String(s.to_string())).map_err(|_| {
                    let names = schemars::schema_for!($ty).as_value().get("enum").cloned().unwrap_or_default();
                    format!("`{s}` is not one of {names}")
                })
            }
        }
    )*};
}
named!(Align, Spread, Order);

/// A node arranged: where it may go, and whether a `rect` places it now.
pub struct Member {
    pub targets: Targets,
    pub rect: bool,
}

/// Where a node arranged lands.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Landed {
    pub node: String,
    /// Its box there, `[x, y, width, height]` in canvas units.
    pub cell: Rect,
}

/// Several nodes arranged: where each lands, and the patch that puts them all there.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Arranged {
    pub landed: Vec<Landed>,
    /// `place` ops, or `choose` ops for `z`, each written where what it sets lives; empty when
    /// each is there already.
    pub patch: Vec<Value>,
}

/// How `m` snaps when it moves with others: by cells where cells or a slot place it, as it
/// was dropped where a `rect` does (or, `free`, every one), in a frame by its own `rect`, and
/// not at all in a stack.
fn way(m: &Member, free: bool) -> Option<Snap> {
    match m.targets.by {
        By::Stack { .. } => None,
        By::Frame { .. } => Some(Snap::Free),
        By::Grid if free || m.rect => Some(Snap::Free),
        By::Cells { .. } if free => None,
        By::Grid | By::Cells { .. } => Some(Snap::Move),
    }
}

/// Where nodes arranged land, and what moves them there.
#[derive(Default)]
struct Landing {
    landed: Vec<Landed>,
    targets: Vec<Target>,
}

impl Landing {
    /// `m` snapped `how` from `to`: where it lands, and, unless it lands where it stands,
    /// what puts it there. A node that does not move keeps its placement as it is, a slot's
    /// name and all.
    fn land(&mut self, m: &Member, how: Option<Snap>, to: Rect) -> Rect {
        let target = how.and_then(|how| m.targets.snap(how, to));
        let cell = target.as_ref().map_or(m.targets.cell, |t| t.cell);
        self.landed.push(Landed { node: m.targets.node.clone(), cell });
        let stays = cell.iter().zip(m.targets.cell).all(|(a, b)| (a - b).abs() < 0.5);
        self.targets.extend(target.filter(|_| !stays));
        cell
    }

    fn arranged(self, state: &str, fork: bool) -> Result<Arranged, OpsError> {
        let ops: Vec<SemanticOp> = self.targets.iter().flat_map(|t| t.ops(Some(state), fork)).collect();
        let patch = ops.iter().map(serde_json::to_value).collect::<Result<_, _>>().context("a patch")?;
        Ok(Arranged { landed: self.landed, patch })
    }
}

/// `members` moved together (PLAN 2.42): the first `by` canvas units, snapped as a drag of it
/// alone snaps (`free`, off the grid), and the rest as far as it went, each its own way; where
/// the grid's edge stops one of them sooner, all of them stop there. `None` where nothing
/// moves the first.
pub fn together(
    members: &[Member],
    by: [f32; 2],
    free: bool,
    state: &str,
    fork: bool,
) -> Result<Option<Arranged>, OpsError> {
    let Some(first) = members.first() else { return Ok(None) };
    let Some(how) = way(first, free) else { return Ok(None) };
    let [x, y, w, h] = first.targets.cell;
    let Some(lead) = first.targets.snap(how, [x + by[0], y + by[1], w, h]) else { return Ok(None) };
    // As far as the first went, or, where the grid stops one of them first, as far as that
    // one can go: they move as one.
    let mut d = [lead.cell[0] - x, lead.cell[1] - y];
    for m in &members[1..] {
        let [x, y, w, h] = m.targets.cell;
        let Some(t) = way(m, free).and_then(|how| m.targets.snap(how, [x + d[0], y + d[1], w, h])) else { continue };
        for (i, went) in [t.cell[0] - x, t.cell[1] - y].into_iter().enumerate() {
            if went.abs() < d[i].abs() {
                d[i] = if went.signum() == d[i].signum() { went } else { 0.0 };
            }
        }
    }
    let mut landing = Landing::default();
    for m in members {
        let [x, y, w, h] = m.targets.cell;
        landing.land(m, way(m, free), [x + d[0], y + d[1], w, h]);
    }
    landing.arranged(state, fork).map(Some)
}

/// `members` aligned on `edge` (PLAN 2.42): each box takes the edge all of them reach
/// farthest, or the middle of them all.
pub fn aligning(members: &[Member], edge: Align, state: &str, fork: bool) -> Result<Arranged, OpsError> {
    if members.len() < 2 {
        return Err(OpsError::new("aligning takes two nodes or more"));
    }
    let cells: Vec<Rect> = members.iter().map(|m| m.targets.cell).collect();
    let low = |i: usize| cells.iter().map(|c| c[i]).fold(f32::INFINITY, f32::min);
    let high = |i: usize| cells.iter().map(|c| c[i] + c[i + 2]).fold(f32::NEG_INFINITY, f32::max);
    let (left, right, top, bottom) = (low(0), high(0), low(1), high(1));
    let mut landing = Landing::default();
    for (m, &[x, y, w, h]) in members.iter().zip(&cells) {
        let to = match edge {
            Align::Left => [left, y, w, h],
            Align::Center => [(left + right - w) / 2.0, y, w, h],
            Align::Right => [right - w, y, w, h],
            Align::Top => [x, top, w, h],
            Align::Middle => [x, (top + bottom - h) / 2.0, w, h],
            Align::Bottom => [x, bottom - h, w, h],
        };
        landing.land(m, way(m, false), to);
    }
    landing.arranged(state, fork)
}

/// `members` spread `along` (PLAN 2.42): the first and the last stay, and the ones between
/// move so the gaps between them all are equal.
pub fn spreading(members: &[Member], along: Spread, state: &str, fork: bool) -> Result<Arranged, OpsError> {
    if members.len() < 3 {
        return Err(OpsError::new("spreading takes three nodes or more"));
    }
    let a = match along {
        Spread::Across => 0,
        Spread::Down => 1,
    };
    let mut order: Vec<&Member> = members.iter().collect();
    scaena_core::sort::by(&mut order, |p, q| p.targets.cell[a].total_cmp(&q.targets.cell[a]));
    let (first, last) = (order[0].targets.cell, order[order.len() - 1].targets.cell);
    let sizes: f32 = order.iter().map(|m| m.targets.cell[a + 2]).sum();
    let gap = (last[a] + last[a + 2] - first[a] - sizes) / (order.len() - 1) as f32;
    let mut at = first[a] + first[a + 2] + gap;
    let mut landing = Landing::default();
    for m in &order {
        let mut to = m.targets.cell;
        if !std::ptr::eq(*m, order[0]) && !std::ptr::eq(*m, order[order.len() - 1]) {
            to[a] = at;
            at += to[a + 2] + gap;
        }
        landing.land(m, way(m, false), to);
    }
    landing.arranged(state, fork)
}

/// One of a container's children, as it paints them: its `z`, where the deck lists it, and
/// its box at rest.
#[derive(Debug, Clone, PartialEq)]
pub struct Sibling {
    pub node: String,
    pub z: i64,
    pub index: usize,
    pub rect: Rect,
}

fn overlap(a: &Rect, b: &Rect) -> bool {
    a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3]
}

/// The `z` each of `siblings` takes for `nodes`, children of one container, to go `order`
/// (PLAN 2.42): only those that change, in the order they changed.
pub fn ordering(siblings: &[Sibling], nodes: &[String], order: Order) -> Result<Vec<(String, i64)>, OpsError> {
    let mut now: Vec<Sibling> = siblings.to_vec();
    for node in nodes {
        if !now.iter().any(|s| &s.node == node) {
            return Err(OpsError::new(format!("`{node}` is not one of the children it is ordered among")));
        }
    }
    let key = |s: &Sibling| (s.z, s.index);
    let painted = |now: &[Sibling]| {
        let mut l: Vec<usize> = (0..now.len()).collect();
        scaena_core::sort::by_key(&mut l, |&i| key(&now[i]));
        l
    };
    // The nodes in the order that keeps what they keep among themselves: the topmost first
    // when they go up, the lowest first when they go down.
    let mut mine: Vec<&String> = nodes.iter().collect();
    let place = |now: &[Sibling], n: &str| painted(now).iter().position(|&i| now[i].node == n).unwrap_or(0);
    scaena_core::sort::by_key(&mut mine, |n| place(&now, n));
    if matches!(order, Order::Forward | Order::Back) {
        mine.reverse();
    }
    let mut changed: Vec<(String, i64)> = Vec::new();
    for node in mine {
        let l = painted(&now);
        let Some(at) = l.iter().position(|&i| &now[i].node == node) else { continue };
        let n = l[at];
        let rect = now[n].rect;
        // Where it goes: between `below` and `above`, positions in `l` without it.
        let others: Vec<usize> = l.iter().copied().filter(|&i| i != n).collect();
        let k = match order {
            Order::Front => others.len(),
            Order::Back => 0,
            Order::Forward => match l[at + 1..].iter().position(|&i| overlap(&now[i].rect, &rect)) {
                Some(j) => at + j + 1,
                None => continue,
            },
            Order::Backward => match l[..at].iter().rposition(|&i| overlap(&now[i].rect, &rect)) {
                Some(j) => j,
                None => continue,
            },
        };
        if k == at && matches!(order, Order::Front | Order::Back) {
            continue;
        }
        put(&mut now, n, k, &mut changed);
    }
    Ok(changed)
}

/// The `z` each of `siblings` takes for `node` to be listed just before `to`, painted just
/// over it, or, `after`, just under it (PLAN 2.50): only those that change, in the order they
/// changed.
pub fn restacking(siblings: &[Sibling], node: &str, to: &str, after: bool) -> Result<Vec<(String, i64)>, OpsError> {
    let mut now: Vec<Sibling> = siblings.to_vec();
    let find = |id: &str| now.iter().position(|s| s.node == id);
    let n =
        find(node).ok_or_else(|| OpsError::new(format!("`{node}` is not one of the children it is ordered among")))?;
    if node == to {
        return Err(OpsError::new(format!("`{node}` goes before or after another child of what holds it")));
    }
    if find(to).is_none() {
        return Err(OpsError::new(format!("`{to}` is not another child of what holds `{node}`")));
    }
    let mut l: Vec<usize> = (0..now.len()).collect();
    scaena_core::sort::by_key(&mut l, |&i| (now[i].z, now[i].index));
    let at = l.iter().position(|&i| i == n).unwrap_or(0);
    let others: Vec<usize> = l.iter().copied().filter(|&i| i != n).collect();
    let there = others.iter().position(|&i| now[i].node == to).unwrap_or(0);
    let k = if after { there } else { there + 1 };
    let mut changed = Vec::new();
    if k != at {
        put(&mut now, n, k, &mut changed);
    }
    Ok(changed)
}

/// `now[n]` painted `k`th among the others, by its `z`: one `z` where one does it, the
/// nearest its own; else, from where it goes up, each the least that keeps it after the one
/// before. Each `z` that changes goes into `changed`, once, last where it last changed.
fn put(now: &mut [Sibling], n: usize, k: usize, changed: &mut Vec<(String, i64)>) {
    let key = |s: &Sibling| (s.z, s.index);
    let mut l: Vec<usize> = (0..now.len()).collect();
    scaena_core::sort::by_key(&mut l, |&i| key(&now[i]));
    let others: Vec<usize> = l.iter().copied().filter(|&i| i != n).collect();
    let below = k.checked_sub(1).map(|i| key(&now[others[i]]));
    let above = others.get(k).map(|&i| key(&now[i]));
    let idx = now[n].index;
    let fits = |v: i64| below.is_none_or(|b| b < (v, idx)) && above.is_none_or(|a| (v, idx) < a);
    let near = [below.map(|b| b.0), below.map(|b| b.0 + 1), above.map(|a| a.0), above.map(|a| a.0 - 1)];
    let mut tries: Vec<i64> = near.into_iter().flatten().collect();
    scaena_core::sort::by_key(&mut tries, |v| (v - now[n].z).abs());
    if let Some(v) = tries.into_iter().find(|&v| fits(v)) {
        now[n].z = v;
        let id = now[n].node.clone();
        changed.retain(|(c, _)| *c != id);
        changed.push((id, v));
        return;
    }
    let mut want: Vec<usize> = others;
    want.insert(k, n);
    let mut last: Option<(i64, usize)> = None;
    for i in want {
        let (z, index) = key(&now[i]);
        let z = match last {
            Some((lz, li)) if (z, index) <= (lz, li) => {
                if index > li {
                    lz
                } else {
                    lz + 1
                }
            }
            _ => z,
        };
        if z != now[i].z {
            now[i].z = z;
            let id = now[i].node.clone();
            changed.retain(|(c, _)| *c != id);
            changed.push((id, z));
        }
        last = Some((z, index));
    }
}

/// The patch that gives each node its `z` (`choose`, written where `z` lives), made in
/// `state`, or, to `fork` it, kept there.
pub fn zs(changed: &[(String, i64)], state: &str, fork: bool) -> Vec<Value> {
    let op = |(node, z): &(String, i64)| json!({ "op": "choose", "node": node, "prop": "z", "value": z, "state": state, "fork": fork });
    changed.iter().map(op).collect()
}

/// How nodes are arranged (PLAN 2.42).
#[derive(Debug, Clone, PartialEq)]
pub enum How {
    /// Moved together, the first `by` canvas units as a drag snaps it (`free`, off the grid),
    /// the rest as far as it went.
    Together {
        by: [f32; 2],
        free: bool,
    },
    Align(Align),
    Spread(Spread),
    Order(Order),
    /// Listed just before `to`, or just after it, among what holds it (PLAN 2.50).
    Next {
        to: String,
        after: bool,
    },
}

/// How nodes are arranged, as a client asks: one of `align`, `spread`, `order`, `before`,
/// `after`, and `by`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Asked {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<Align>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spread: Option<Spread>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<Order>,
    /// Listed just before this child of the same container, as a layers panel lists them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    /// Listed just after this child of the same container.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<[f32; 2]>,
    /// With `by`: off the grid.
    #[serde(default)]
    pub free: bool,
}

impl Asked {
    /// The one way asked, or why there is not one.
    pub fn how(&self) -> Result<How, OpsError> {
        let ways = [
            self.align.is_some(),
            self.spread.is_some(),
            self.order.is_some(),
            self.before.is_some(),
            self.after.is_some(),
            self.by.is_some(),
        ];
        if ways.iter().filter(|w| **w).count() != 1 {
            return Err(OpsError::new(
                "nodes are arranged one way: give one of `align`, `spread`, `order`, `before`, `after`, and `by`",
            ));
        }
        if self.free && self.by.is_none() {
            return Err(OpsError::new("`free` moves nodes off the grid: move them with `by`"));
        }
        Ok(match (self.align, self.spread, self.order, &self.before, &self.after, self.by) {
            (Some(edge), ..) => How::Align(edge),
            (_, Some(along), ..) => How::Spread(along),
            (_, _, Some(order), ..) => How::Order(order),
            (_, _, _, Some(to), ..) => How::Next { to: to.clone(), after: false },
            (_, _, _, _, Some(to), _) => How::Next { to: to.clone(), after: true },
            (.., Some(by)) => How::Together { by, free: self.free },
            _ => return Err(OpsError::new("nodes are arranged one way")),
        })
    }
}

/// The nodes arranged, as `snap` shows them: where each may go (`found`, the engine's for
/// each), and whether a `rect` places it.
pub fn members(snap: &Snapshot, found: Vec<Targets>) -> Vec<Member> {
    let placed = |node: &str| snap.nodes.get(node).and_then(|p| p.get("at")).and_then(|at| at.get("rect")).is_some();
    found.into_iter().map(|targets| Member { rect: placed(&targets.node), targets }).collect()
}

/// What holds `node` in `snap`, laid out as `boxes`: none at the root.
fn holder<'b>(boxes: &'b [NodeBox], snap: &Snapshot, node: &str) -> Result<&'b Option<String>, OpsError> {
    let found = boxes.iter().find(|b| b.node == node);
    let missing = || OpsError::new(format!("`{node}` is not on screen in `{}`", snap.state_id));
    Ok(&found.ok_or_else(missing)?.parent)
}

/// The children of what holds `node` in `snap`, laid out as `boxes`, as it paints them: each
/// with its `z`, where `deck` lists it, and its box.
pub fn siblings(deck: &Deck, snap: &Snapshot, boxes: &[NodeBox], node: &str) -> Result<Vec<Sibling>, OpsError> {
    let parent = holder(boxes, snap, node)?;
    let z = |id: &str| snap.nodes.get(id).and_then(|p| p.get("z")).and_then(Value::as_i64).unwrap_or(0);
    let mine = boxes.iter().filter(|b| &b.parent == parent);
    let sibling = |b: &NodeBox| Sibling {
        node: b.node.clone(),
        z: z(&b.node),
        index: deck.nodes.get_index_of(&b.node).unwrap_or(usize::MAX),
        rect: b.rect,
    };
    Ok(mine.map(sibling).collect())
}

/// `nodes` arranged `how` in `snap`, laid out as `boxes` (PLAN 2.42): `found` is where each
/// may go, the engine's, in the order named. They are children of one container; the patch
/// is made in the state, or, to `fork` it, kept there. `None` where nothing moves them.
pub fn arrange(
    deck: &Deck,
    snap: &Snapshot,
    boxes: &[NodeBox],
    nodes: &[String],
    found: Vec<Targets>,
    how: How,
    fork: bool,
) -> Result<Option<Arranged>, OpsError> {
    let Some(first) = nodes.first() else { return Err(OpsError::new("name the nodes to arrange")) };
    let parent = holder(boxes, snap, first)?;
    for node in &nodes[1..] {
        if holder(boxes, snap, node)? != parent {
            let held = |p: &Option<String>| p.as_ref().map_or("the canvas".to_string(), |p| format!("`{p}`"));
            return Err(OpsError::new(format!(
                "`{first}` is held by {} and `{node}` by {}: arrange what one container holds",
                held(parent),
                held(holder(boxes, snap, node)?)
            )));
        }
    }
    let state = &snap.state_id;
    let members = members(snap, found);
    if let (false, Some(By::Stack { parent, .. })) =
        (matches!(how, How::Order(_) | How::Next { .. }), members.first().map(|m| &m.targets.by))
    {
        return Err(OpsError::new(format!(
            "the stack `{parent}` places what it holds in its order: drag one along it, or order them in front or behind"
        )));
    }
    match how {
        How::Together { by, free } => together(&members, by, free, state, fork),
        How::Align(edge) => aligning(&members, edge, state, fork).map(Some),
        How::Spread(along) => spreading(&members, along, state, fork).map(Some),
        How::Order(order) => {
            let changed = ordering(&siblings(deck, snap, boxes, first)?, nodes, order)?;
            Ok(Some(Arranged { landed: Vec::new(), patch: zs(&changed, state, fork) }))
        }
        How::Next { to, after } => {
            if nodes.len() > 1 {
                return Err(OpsError::new("one node goes before or after another at a time"));
            }
            if &to == first {
                return Err(OpsError::new(format!("`{first}` goes before or after another child of what holds it")));
            }
            if holder(boxes, snap, &to)? != parent {
                return Err(OpsError::new(format!("`{to}` is not held by what holds `{first}`")));
            }
            let member = &members[0];
            if !matches!(member.targets.by, By::Stack { .. }) {
                let changed = restacking(&siblings(deck, snap, boxes, first)?, first, &to, after)?;
                return Ok(Some(Arranged { landed: Vec::new(), patch: zs(&changed, state, fork) }));
            }
            // In a stack, the order it lays its children out in, as the list has them.
            let others = member.targets.flow.iter().filter(|(id, ..)| id != first);
            let there = others.clone().position(|(id, ..)| *id == to);
            let there =
                there.ok_or_else(|| OpsError::new(format!("`{to}` is not another child of what holds `{first}`")))?;
            let target = member.targets.ordered(there + usize::from(after));
            let mut landing = Landing::default();
            if let Some(target) = target.filter(|t| !t.spots.is_empty()) {
                landing.landed.push(Landed { node: first.clone(), cell: target.cell });
                landing.targets.push(target);
            }
            landing.arranged(state, fork).map(Some)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(node: &str, z: i64, index: usize, rect: Rect) -> Sibling {
        Sibling { node: node.into(), z, index, rect }
    }

    fn painted(siblings: &[Sibling], changed: &[(String, i64)]) -> Vec<String> {
        let mut now = siblings.to_vec();
        for (n, z) in changed {
            now.iter_mut().find(|s| &s.node == n).unwrap().z = *z;
        }
        now.sort_by_key(|s| (s.z, s.index));
        now.into_iter().map(|s| s.node).collect()
    }

    const ALL: Rect = [0.0, 0.0, 100.0, 100.0];

    #[test]
    fn forward_goes_in_front_of_the_next_it_overlaps_with_one_z() {
        let siblings = [s("a", 0, 0, ALL), s("b", 0, 1, [500.0, 0.0, 10.0, 10.0]), s("c", 0, 2, ALL)];
        let changed = ordering(&siblings, &["a".into()], Order::Forward).unwrap();
        assert_eq!(changed.len(), 1, "{changed:?}");
        assert_eq!(painted(&siblings, &changed), ["b", "c", "a"], "past b, which it does not overlap, to c");
        let back = ordering(&siblings, &["c".into()], Order::Backward).unwrap();
        assert_eq!(painted(&siblings, &back), ["c", "a", "b"]);
    }

    #[test]
    fn front_and_back_pass_all_and_keep_what_several_keep_among_themselves() {
        let siblings = [s("a", 0, 0, ALL), s("b", 0, 1, ALL), s("c", 1, 2, ALL), s("d", 2, 3, ALL)];
        let front = ordering(&siblings, &["a".into(), "b".into()], Order::Front).unwrap();
        assert_eq!(painted(&siblings, &front), ["c", "d", "a", "b"]);
        let back = ordering(&siblings, &["d".into(), "c".into()], Order::Back).unwrap();
        assert_eq!(painted(&siblings, &back), ["c", "d", "a", "b"]);
        assert!(ordering(&siblings, &["d".into()], Order::Front).unwrap().is_empty(), "in front already");
    }

    #[test]
    fn listed_before_one_is_painted_just_over_it_and_after_one_just_under_it() {
        // Painted a, b, c (bottom first): a layers panel lists c, b, a.
        let siblings = [s("a", 0, 0, ALL), s("b", 0, 1, ALL), s("c", 0, 2, ALL)];
        let top = restacking(&siblings, "a", "c", false).unwrap();
        assert_eq!(painted(&siblings, &top), ["b", "c", "a"], "before the topmost: over it, {top:?}");
        assert_eq!(top.len(), 1, "one z does it");
        let under = restacking(&siblings, "c", "b", true).unwrap();
        assert_eq!(painted(&siblings, &under), ["a", "c", "b"], "after b: just under it, {under:?}");
        assert!(restacking(&siblings, "b", "a", false).unwrap().is_empty(), "just over a already");
        assert!(restacking(&siblings, "a", "a", false).is_err());
        assert!(restacking(&siblings, "a", "d", false).is_err());
    }

    #[test]
    fn where_no_one_z_does_it_the_fewest_do() {
        // b and c share a z and a is listed after both: a z of 5 puts a after c but also after
        // d, which it must stay behind; renumbered, it goes between them.
        let siblings = [s("b", 5, 0, ALL), s("c", 5, 1, ALL), s("d", 5, 2, ALL), s("a", 0, 3, ALL)];
        let changed = ordering(&siblings, &["a".into()], Order::Forward).unwrap();
        assert_eq!(painted(&siblings, &changed), ["b", "a", "c", "d"], "{changed:?}");
    }
}

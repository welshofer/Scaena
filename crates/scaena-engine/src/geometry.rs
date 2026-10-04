//! What stands where in a state at rest (ADR-0013): each visible node's box, and the nodes
//! under a point, read from the [`Scene`] its frames at rest draw. A client that edits by
//! pointing (the web editor, the Mac app) asks here: it lays nothing out, and it never reads
//! pixels back.
//!
//! A node's box is the place layout gave it: its grid cell or slot, the box a container
//! gave it, or a group's box around its members (SPEC §3.4). A point hits a node inside that
//! box, or within [`SLOP`] of a box too thin to point at, as a rule's line is.

use crate::sample::Scene;
use scaena_core::displaylist::Rect;

/// How near a point must come to a box under twice its width or height, canvas units: a
/// rule a hairline thick is pointed at, not hit by luck. At 1080 a canvas unit is a pixel.
pub const SLOP: f32 = 6.0;

/// A visible node's place in a state at rest.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeBox {
    pub node: String,
    /// `[x, y, width, height]`, canvas units.
    pub rect: Rect,
    /// The container or group it sits in, if any.
    pub parent: Option<String>,
    /// Whether it draws anything: a container with no panel, and a group, only hold others.
    pub draws: bool,
}

/// A node that draws at a point, with the containers and groups it sits in, innermost first.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub node: String,
    pub rect: Rect,
    pub containers: Vec<String>,
}

impl Scene {
    /// Every visible node's box: those that draw, in paint order, then the containers and
    /// groups that only hold others, by id.
    pub fn boxes(&self) -> Vec<NodeBox> {
        let place = |node: &str, draws: bool| {
            let at = &self.tree[node];
            NodeBox { node: node.to_string(), rect: at.rect, parent: at.parent.clone(), draws }
        };
        let drawn: Vec<&str> =
            self.nodes.iter().filter(|n| self.tree.contains_key(&n.id)).map(|n| n.id.as_str()).collect();
        let mut holders: Vec<&str> = self.tree.keys().map(String::as_str).filter(|id| !drawn.contains(id)).collect();
        holders.sort_unstable();
        drawn.into_iter().map(|id| place(id, true)).chain(holders.into_iter().map(|id| place(id, false))).collect()
    }

    /// The nodes that draw at `point` (canvas units), topmost first: the last painted first,
    /// each inside its box, or within [`SLOP`] of one too thin to point at. A node faded out
    /// entirely is not there to point at.
    pub fn hit(&self, point: [f32; 2]) -> Vec<Hit> {
        let mut out = Vec::new();
        for node in self.nodes.iter().rev() {
            let Some(place) = self.tree.get(&node.id) else { continue };
            if node.opacity <= 0.0 || !reaches(place.rect, point) {
                continue;
            }
            out.push(Hit { node: node.id.clone(), rect: place.rect, containers: self.containers(&node.id) });
        }
        out
    }

    /// The containers and groups `id` sits in, innermost first.
    fn containers(&self, id: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut at = self.tree.get(id).and_then(|p| p.parent.as_deref());
        while let Some(parent) = at {
            out.push(parent.to_string());
            at = self.tree.get(parent).and_then(|p| p.parent.as_deref());
        }
        out
    }
}

/// Whether `point` falls in `rect`, its edges included, or within [`SLOP`] of it along a
/// side shorter than twice that.
fn reaches(rect: Rect, point: [f32; 2]) -> bool {
    let [x, y, w, h] = rect;
    let near = |at: f32, from: f32, size: f32| {
        let slop = if size < 2.0 * SLOP { SLOP } else { 0.0 };
        at >= from - slop && at <= from + size + slop
    };
    near(point[0], x, w) && near(point[1], y, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_point_reaches_a_box_or_a_hairline_near_it() {
        let card = [100.0, 100.0, 400.0, 200.0];
        assert!(reaches(card, [100.0, 100.0]) && reaches(card, [500.0, 300.0]) && reaches(card, [300.0, 200.0]));
        assert!(!reaches(card, [99.0, 200.0]) && !reaches(card, [300.0, 301.0]));
        // A rule a unit thick: pointed at from 6 units above or below, not from 7.
        let rule = [100.0, 500.0, 800.0, 1.0];
        assert!(reaches(rule, [400.0, 494.0]) && reaches(rule, [400.0, 507.0]));
        assert!(!reaches(rule, [400.0, 493.0]) && !reaches(rule, [400.0, 508.0]));
        // Along its length it ends where it ends.
        assert!(!reaches(rule, [99.0, 500.0]) && !reaches(rule, [901.0, 500.0]));
    }
}

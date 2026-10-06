//! What stands where in a state at rest, and where a node may go (ADR-0013): each visible
//! node's box, the nodes under a point, and the places a dragged node snaps to, read from
//! the [`Scene`] its frames at rest draw. A client that edits by pointing (the web editor,
//! the Mac app) asks here: it lays nothing out, and it never reads pixels back.
//!
//! A node's box is the place layout gave it: its grid cell or slot, the box a container
//! gave it, or a group's box around its members (SPEC §3.4). Where its `transform`, or what
//! holds it, turns, scales, leans, or moves it (SPEC §3.3), the box is drawn through that
//! map, and a point is read back through it. A point hits a node inside its box, or within
//! [`SLOP`] of a box too thin to point at, as a rule's line is.
//!
//! What holds a node says where it may go ([`Targets`]): the theme's grid holds a root, or a
//! group's member, by cells, a slot, or a `rect`; a grid container by its cells or areas; a
//! stack by order; a frame by a `rect` from its padding edge. A box dropped there snaps to a
//! [`Target`]: the box a guide shows, and the `place` ops that put the node there.
//!
//! In a text, a point hits a character: where a caret put there stands ([`Scene::carets`],
//! [`crate::carets`]).

use crate::EngineError;
use crate::carets::Carets;
use crate::containers::{self, Tracks};
use crate::layout::Grid;
use crate::sample::{Content, Scene};
use crate::theme::Theme;
use scaena_core::Snapshot;
use scaena_core::displaylist::{DisplayList, Op, Rect};
use scaena_core::document::{Deck, NodeType};
use scaena_core::model::values::{Range, Rect as Placed};
use scaena_core::patch::{SemanticOp, Spot};
use serde_json::{Value, json};

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
    /// Where its `transform` and those of what holds it draw it (SPEC §3.3): the map of
    /// canvas points `[a, b, c, d, e, f]` (`x' = a·x + c·y + e`) from its box as laid out to
    /// where it is drawn. `None` where nothing moves it.
    pub transform: Option<[f32; 6]>,
}

/// A node that draws at a point, with the containers and groups it sits in, innermost first.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub node: String,
    pub rect: Rect,
    /// As [`NodeBox::transform`].
    pub transform: Option<[f32; 6]>,
    pub containers: Vec<String>,
    /// For a text, where a caret put at the point stands: an offset in its text as written,
    /// at the edge of a character nearest the point ([`Carets::at`]).
    pub offset: Option<usize>,
}

impl Scene {
    /// Every visible node's box: those that draw, in paint order, then the containers and
    /// groups that only hold others, by id.
    pub fn boxes(&self) -> Vec<NodeBox> {
        let place = |node: &str, draws: bool| {
            let at = &self.tree[node];
            let transform = self.drawn(node);
            NodeBox { node: node.to_string(), rect: at.rect, parent: at.parent.clone(), draws, transform }
        };
        let drawn: Vec<&str> =
            self.nodes.iter().filter(|n| self.tree.contains_key(&n.id)).map(|n| n.id.as_str()).collect();
        let mut holders: Vec<&str> = self.tree.keys().map(String::as_str).filter(|id| !drawn.contains(id)).collect();
        holders.sort_unstable();
        drawn.into_iter().map(|id| place(id, true)).chain(holders.into_iter().map(|id| place(id, false))).collect()
    }

    /// The nodes that draw at `point` (canvas units), topmost first: the last painted first,
    /// each inside its box, or within [`SLOP`] of one too thin to point at, read through its
    /// transform. A node faded out entirely, or flattened to nothing, is not there to point at.
    pub fn hit(&self, point: [f32; 2]) -> Vec<Hit> {
        let mut out = Vec::new();
        for node in self.nodes.iter().rev() {
            let Some(place) = self.tree.get(&node.id) else { continue };
            let Some(at) = self.laid_out(&node.id, point) else { continue };
            if node.opacity <= 0.0 || !reaches(place.rect, at) {
                continue;
            }
            let offset = match &node.content {
                Content::Text(placed) => Some(placed.text.carets(placed.origin).at(at).0),
                _ => None,
            };
            let (containers, transform) = (self.containers(&node.id), self.drawn(&node.id));
            out.push(Hit { node: node.id.clone(), rect: place.rect, transform, containers, offset });
        }
        out
    }

    /// The map [`NodeBox::transform`] gives `node`: `None` where nothing moves it.
    pub fn drawn(&self, node: &str) -> Option<[f32; 6]> {
        let map = self.posed(node, true);
        (map != [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]).then(|| map.map(|v| v as f32))
    }

    /// Where `point` (canvas units), drawn through `node`'s transform and those of what
    /// holds it, stands as `node` is laid out: what its box, its carets, and its image are
    /// measured in. `None` where a transform flattens it to nothing.
    pub fn laid_out(&self, node: &str, point: [f32; 2]) -> Option<[f32; 2]> {
        let map = self.posed(node, true);
        if map == [1.0, 0.0, 0.0, 1.0, 0.0, 0.0] {
            return Some(point);
        }
        let back = scaena_core::pose::invert(&map)?;
        let [x, y] = scaena_core::pose::apply(&back, point.map(f64::from));
        Some([x as f32, y as f32])
    }

    /// The point of image `node` drawn under `point`, in fractions of the part its crop keeps:
    /// what its `focal` names (PLAN 2.45). `None` where it is no image this state draws, or the
    /// point is off the image.
    pub fn image_point(&self, node: &str, point: [f32; 2]) -> Option<[f32; 2]> {
        let at = self.laid_out(node, point)?;
        self.nodes.iter().find(|n| n.id == node).and_then(|n| match &n.content {
            Content::Image(image) => image.point(at),
            _ => None,
        })
    }

    /// Where a caret stands in `node`'s text, if it is a text this state draws: each
    /// character as written, on its line, between the edges of its glyphs, as it is laid
    /// out. Where it is drawn turned, scaled, or moved, its box's [`NodeBox::transform`] draws
    /// them there, and [`Scene::laid_out`] reads a point back.
    pub fn carets(&self, node: &str) -> Option<Carets> {
        self.nodes.iter().find(|n| n.id == node).and_then(|n| match &n.content {
            Content::Text(placed) => Some(placed.text.carets(placed.origin)),
            _ => None,
        })
    }

    /// The state at rest, its shaders `time` seconds into the global timeline, with `nodes`
    /// and everything they hold drawn `by` canvas units from where they stand, over the rest:
    /// what a drag shows while it moves, held above the page. Only their layers move;
    /// nothing is laid out again (ADR-0013). A member of a group composited as one layer
    /// moves inside it, in its place.
    pub fn moved(&self, time: f64, nodes: &[&str], by: [f32; 2]) -> DisplayList {
        let mut dl = self.draw_at(time);
        let mut held = nodes.to_vec();
        let mut i = 0;
        while let Some(id) = held.get(i).copied() {
            held.extend(self.tree.get(id).into_iter().flat_map(|p| p.children.iter().map(String::as_str)));
            i += 1;
        }
        carry(&mut dl.ops, &held, by, [1.0, 0.0, 0.0, 1.0]);
        let carried = |op: &Op| matches!(op, Op::Layer { node: Some(id), .. } if held.contains(&id.as_str()));
        let (over, rest): (Vec<Op>, Vec<Op>) = std::mem::take(&mut dl.ops).into_iter().partition(carried);
        dl.ops = rest;
        dl.ops.extend(over);
        dl
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

/// Move the layers of `held` in `ops` by `by`, canvas units, where `linear` is what the
/// layers around them scale and turn by: a layer moved carries what it holds, so a node in a
/// group's layer moves once.
fn carry(ops: &mut [Op], held: &[&str], by: [f32; 2], linear: [f32; 4]) {
    for op in ops {
        let Op::Layer { node, transform, ops, .. } = op else { continue };
        let [a, b, c, d] = linear;
        if node.as_deref().is_some_and(|id| held.contains(&id)) {
            // `by` in the units the layer is placed in.
            let det = a * d - b * c;
            if det.abs() > f32::EPSILON {
                transform[4] += (d * by[0] - c * by[1]) / det;
                transform[5] += (a * by[1] - b * by[0]) / det;
            }
            continue;
        }
        let [e, f, g, h] = [transform[0], transform[1], transform[2], transform[3]];
        carry(ops, held, by, [a * e + c * f, b * e + d * f, a * g + c * h, b * g + d * h]);
    }
}

/// What holds a node, and so how it is placed and where it may go (ADR-0013).
#[derive(Debug, Clone, PartialEq)]
pub enum By {
    /// The theme's grid holds a root, or a group's member: by cells, a slot of the state's
    /// layout template, or a `rect` on the canvas, an override (W301).
    Grid,
    /// A stack, by its children's order (`index`), along `x` when `across`.
    Stack { parent: String, across: bool },
    /// A grid container, by its cells or a named `area`.
    Cells { parent: String },
    /// A frame, by a `rect` from its padding edge.
    Frame { parent: String },
}

/// Where a node may go in a state at rest, in one format (ADR-0013): the guides a drag
/// shows, and what [`Targets::snap`] snaps a dropped box to.
#[derive(Debug, Clone, PartialEq)]
pub struct Targets {
    pub node: String,
    pub by: By,
    /// The box its placement names now, before its inset, offset, alignment, and size: its
    /// cells, its slot, or its `rect`; in a stack, its box. What a drag moves.
    pub cell: Rect,
    /// The tracks a placement by cells takes, each `[start, end]`: the theme grid's in this
    /// format, or its grid container's. Empty in a stack or a frame.
    pub columns: Vec<[f32; 2]>,
    pub rows: Vec<[f32; 2]>,
    /// The boxes a placement by name takes: the template's slots in this format, then
    /// `canvas` and `grid`; or a grid container's areas.
    pub slots: Vec<(String, Rect)>,
    /// A stack's children in their order, each with its box and its `at.index`.
    pub flow: Vec<(String, Rect, u32)>,
    /// What a `rect` is measured in: the canvas, a frame's padding box; or the container.
    pub within: Rect,
}

/// How a box dropped by a drag snaps (ADR-0013).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Snap {
    /// Moved: as many cells as the node spans now, from the track nearest its corner.
    Move,
    /// Resized: each edge to the nearest track's.
    Resize,
    /// Into the slot, or the area, it covers most.
    Slot,
    /// Where it was dropped, in whole canvas units: a `rect`, an override on the theme's
    /// grid (W301), a frame's own way of placing.
    Free,
    /// Among a stack's children, where its middle falls.
    Order,
}

/// Where a dropped box lands: the box a guide shows there, and the placements that put the
/// node there, its own and, in a stack, each sibling's whose place in the order changes.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub cell: Rect,
    pub spots: Vec<(String, Spot)>,
}

impl Target {
    /// The patch that puts the node there, made in `state`: a `place` per spot, each
    /// written where that placement lives (SPEC §7.3), or, to `fork` them, kept to `state`.
    pub fn ops(&self, state: Option<&str>, fork: bool) -> Vec<SemanticOp> {
        let place = |(node, spot): &(String, Spot)| SemanticOp::Place {
            node: node.clone(),
            at: spot.clone(),
            state: state.map(String::from),
            fork,
        };
        self.spots.iter().map(place).collect()
    }
}

impl Targets {
    /// Where `cell`, the box this node's placement names as a drag left it, lands when it
    /// snaps `how`. `None` where nothing places the node that way: `Order` outside a stack,
    /// `Move`, `Resize`, and `Slot` in a stack or a frame, `Free` in a stack or a grid
    /// container.
    pub fn snap(&self, how: Snap, cell: Rect) -> Option<Target> {
        let one = |spot: Spot, cell: Rect| Some(Target { cell, spots: vec![(self.node.clone(), spot)] });
        match (how, &self.by) {
            (Snap::Move | Snap::Resize, By::Grid | By::Cells { .. }) => {
                let (cols, rows) = (&self.columns, &self.rows);
                if cols.is_empty() || rows.is_empty() {
                    return None;
                }
                let pick = |tracks: &[[f32; 2]], at: f32, size: f32, now: f32, spans: f32| match how {
                    Snap::Move => {
                        let (a, b) = covered(tracks, now, spans);
                        shift(tracks, b - a + 1, at)
                    }
                    _ => covered(tracks, at, size),
                };
                let c = pick(cols, cell[0], cell[2], self.cell[0], self.cell[2]);
                let r = pick(rows, cell[1], cell[3], self.cell[1], self.cell[3]);
                let rect = [cols[c.0][0], rows[r.0][0], cols[c.1][1] - cols[c.0][0], rows[r.1][1] - rows[r.0][0]];
                one(Spot { col: Some(range(c)), row: Some(range(r)), ..Spot::default() }, rect)
            }
            (Snap::Slot, By::Grid | By::Cells { .. }) => {
                let (name, rect) = most(&self.slots, cell)?;
                let spot = match self.by {
                    By::Grid => Spot { slot: Some(name.clone()), ..Spot::default() },
                    _ => Spot { area: Some(name.clone()), ..Spot::default() },
                };
                one(spot, *rect)
            }
            (Snap::Free, By::Grid | By::Frame { .. }) => {
                let [x, y] = [(cell[0] - self.within[0]).round(), (cell[1] - self.within[1]).round()];
                let [w, h] = [cell[2].round().max(1.0), cell[3].round().max(1.0)];
                let spot = Spot { rect: Some(Placed([x, y, w, h].map(f64::from))), ..Spot::default() };
                one(spot, [self.within[0] + x, self.within[1] + y, w, h])
            }
            (Snap::Order, By::Stack { across, .. }) => {
                let middle = |r: &Rect| if *across { r[0] + r[2] / 2.0 } else { r[1] + r[3] / 2.0 };
                let others = self.flow.iter().filter(|(id, ..)| *id != self.node);
                self.ordered(others.filter(|(_, r, _)| middle(r) < middle(&cell)).count())
            }
            _ => None,
        }
    }

    /// The cells `cell` stands in on the tracks this node takes (the theme grid's, or a grid
    /// container's): on each axis the tracks it overlaps by half the shorter of the two or
    /// more, else the one nearest its middle (PLAN 2.50). Where a node moved into a grid lands,
    /// a box smaller than a track in the track it is in. `None` without tracks.
    pub fn standing(&self, cell: Rect) -> Option<Target> {
        let (cols, rows) = (&self.columns, &self.rows);
        if !matches!(self.by, By::Grid | By::Cells { .. }) || cols.is_empty() || rows.is_empty() {
            return None;
        }
        let (c, r) = (stands(cols, cell[0], cell[2]), stands(rows, cell[1], cell[3]));
        let rect = [cols[c.0][0], rows[r.0][0], cols[c.1][1] - cols[c.0][0], rows[r.1][1] - rows[r.0][0]];
        let spot = Spot { col: Some(range(c)), row: Some(range(r)), ..Spot::default() };
        Some(Target { cell: rect, spots: vec![(self.node.clone(), spot)] })
    }

    /// The node `k`th among the other children of its stack, in the order the stack lays
    /// them out (PLAN 2.50): each child whose `at.index` that changes, renumbered from 0, and
    /// the guide, a line across the stack where the node goes. `None` outside a stack.
    pub fn ordered(&self, k: usize) -> Option<Target> {
        let By::Stack { across, .. } = &self.by else { return None };
        let ends = |r: &Rect| if *across { (r[0], r[0] + r[2]) } else { (r[1], r[1] + r[3]) };
        let others: Vec<&(String, Rect, u32)> = self.flow.iter().filter(|(id, ..)| *id != self.node).collect();
        let k = k.min(others.len());
        let mut order: Vec<&str> = others.iter().map(|(id, ..)| id.as_str()).collect();
        order.insert(k, &self.node);
        let now: Vec<&str> = self.flow.iter().map(|(id, ..)| id.as_str()).collect();
        let spots = if order == now {
            Vec::new()
        } else {
            let index = |id: &str| self.flow.iter().find(|(f, ..)| f == id).map_or(0, |(.., i)| *i);
            let moved = order.iter().enumerate().filter(|&(n, id)| index(id) != n as u32);
            moved.map(|(n, id)| (id.to_string(), Spot { index: Some(n as u32), ..Spot::default() })).collect()
        };
        // The guide: a line across the stack where the node goes.
        let at = match (k.checked_sub(1).map(|i| &others[i].1), others.get(k).map(|o| &o.1)) {
            (Some(before), Some(after)) => (ends(before).1 + ends(after).0) / 2.0,
            (Some(before), None) => ends(before).1,
            (None, Some(after)) => ends(after).0,
            (None, None) => ends(&self.cell).0,
        };
        let w = self.within;
        let line = if *across { [at, w[1], 0.0, w[3]] } else { [w[0], at, w[2], 0.0] };
        Some(Target { cell: line, spots })
    }
}

/// Where `node` may go in the state `snap` resolves (with its overrides), laid out as
/// `scene` (ADR-0013): in what holds it; or, `into` another container (the canvas for
/// `None`), there, its cell the box it stands in now (PLAN 2.50).
pub(crate) fn targets(
    deck: &Deck,
    theme: &Theme,
    snap: &Snapshot,
    scene: &Scene,
    node: &str,
    into: Option<Option<&str>>,
) -> Result<Targets, EngineError> {
    let missing = |id: &str| EngineError::Layout(format!("`{id}` is not on screen in state `{}`", scene.state));
    let place = scene.tree.get(node).ok_or_else(|| missing(node))?;
    let shown = snap.nodes.get(node).ok_or_else(|| missing(node))?;
    // Into another container, the node's own placement places it no more.
    let at = if into.is_some() { None } else { shown.get("at") };
    if let Some(Some(holder)) = into {
        scene.tree.get(holder).ok_or_else(|| missing(holder))?;
        if !deck.nodes.get(holder).is_some_and(|h| h.node_type.is_container()) {
            return Err(EngineError::Layout(format!("`{holder}` holds nothing: it is not a container")));
        }
    }
    let canvas = [0.0, 0.0, scene.canvas[0], scene.canvas[1]];
    let mut t = Targets {
        node: node.to_string(),
        by: By::Grid,
        cell: place.rect,
        columns: Vec::new(),
        rows: Vec::new(),
        slots: Vec::new(),
        flow: Vec::new(),
        within: canvas,
    };
    let held_by = into.unwrap_or(place.parent.as_deref());
    let parent = held_by.filter(|p| deck.nodes[*p].node_type != NodeType::Group);
    let Some(parent) = parent else {
        let grid = Grid::from_theme(theme, scene.canvas)?;
        let template = snap.layout.as_deref();
        t.cell = if into.is_some() { place.rect } else { grid.cell(theme, template, at)? };
        (t.columns, t.rows) = (grid.columns(), grid.rows());
        t.slots = slots(theme, &grid, template)?;
        return Ok(t);
    };
    let holder = &scene.tree[parent];
    let props = &snap.nodes[parent];
    // A container's children stand from its box before its own inset.
    let inset = props.get("at").and_then(|a| a.get("inset")).and_then(Value::as_f64).unwrap_or(0.0) as f32;
    let r = holder.rect;
    t.within = [r[0] - inset, r[1] - inset, r[2] + 2.0 * inset, r[3] + 2.0 * inset];
    match deck.nodes[parent].node_type {
        NodeType::Stack => {
            let across = props.get("axis").and_then(Value::as_str) == Some("x");
            t.by = By::Stack { parent: parent.to_string(), across };
            let index = |id: &str| {
                let at = snap.nodes[id].get("at");
                at.and_then(|a| a.get("index")).and_then(Value::as_u64).unwrap_or(0) as u32
            };
            t.flow = holder.children.iter().map(|kid| (kid.clone(), scene.tree[kid].rect, index(kid))).collect();
        }
        NodeType::Grid => {
            t.by = By::Cells { parent: parent.to_string() };
            let Tracks { columns, rows } = scene.tracks.get(parent).cloned().unwrap_or_default();
            let areas = containers::areas(props).map_err(|e| EngineError::Layout(format!("node `{parent}`: {e}")))?;
            for (name, [c0, c1, r0, r1]) in areas {
                let span = |tracks: &[[f32; 2]], a: u16, b: u16| {
                    tracks.get(a as usize - 1).copied().zip(tracks.get(b as usize - 1).copied())
                };
                if let (Some((x0, x1)), Some((y0, y1))) = (span(&columns, c0, c1), span(&rows, r0, r1)) {
                    t.slots.push((name, [x0[0], y0[0], x1[1] - x0[0], y1[1] - y0[0]]));
                }
            }
            let named = at.and_then(|a| a.get("area")).and_then(Value::as_str);
            t.cell = match named.and_then(|name| t.slots.iter().find(|(n, _)| n == name)) {
                Some((_, area)) => *area,
                None => {
                    let r = place.rect;
                    let x = lines(&columns, at.and_then(|a| a.get("col"))).unwrap_or([r[0], r[0] + r[2]]);
                    let y = lines(&rows, at.and_then(|a| a.get("row"))).unwrap_or([r[1], r[1] + r[3]]);
                    [x[0], y[0], x[1] - x[0], y[1] - y[0]]
                }
            };
            (t.columns, t.rows) = (columns, rows);
        }
        NodeType::Frame => {
            t.by = By::Frame { parent: parent.to_string() };
            let pad =
                containers::padding(theme, props).map_err(|e| EngineError::Layout(format!("node `{parent}`: {e}")))?;
            let w = t.within;
            t.within = [w[0] + pad[3], w[1] + pad[0], w[2] - pad[1] - pad[3], w[3] - pad[0] - pad[2]];
            let rect: Option<[f32; 4]> =
                at.and_then(|a| a.get("rect")).and_then(|r| serde_json::from_value(r.clone()).ok());
            t.cell = match (into, rect) {
                (Some(_), _) => place.rect,
                (None, Some([x, y, w, h])) => [t.within[0] + x, t.within[1] + y, w, h],
                (None, None) => t.within,
            };
        }
        _ => {}
    }
    Ok(t)
}

/// Where a node new to the state `snap` resolves would go at the root, laid out as `scene`
/// (PLAN 2.34): the theme's grid in this format and the template's slots, as [`targets`]
/// gives them for a node the grid holds, the new node named `node`. Its cell is `size` from
/// the grid's corner, so a move snaps a box by as many tracks as `size` covers.
pub(crate) fn room(
    theme: &Theme,
    snap: &Snapshot,
    scene: &Scene,
    node: &str,
    size: [f32; 2],
) -> Result<Targets, EngineError> {
    let grid = Grid::from_theme(theme, scene.canvas)?;
    let template = snap.layout.as_deref();
    let (columns, rows) = (grid.columns(), grid.rows());
    let corner = [columns.first().map_or(0.0, |c| c[0]), rows.first().map_or(0.0, |r| r[0])];
    Ok(Targets {
        node: node.to_string(),
        by: By::Grid,
        cell: [corner[0], corner[1], size[0], size[1]],
        columns,
        rows,
        slots: slots(theme, &grid, template)?,
        flow: Vec::new(),
        within: [0.0, 0.0, scene.canvas[0], scene.canvas[1]],
    })
}

/// The boxes a root takes by name in `template`: its slots in this format, then `canvas`
/// and `grid`.
fn slots(theme: &Theme, grid: &Grid, template: Option<&str>) -> Result<Vec<(String, Rect)>, EngineError> {
    let named = template.and_then(|name| theme.slots(name)).into_iter().flat_map(|slots| slots.keys());
    let names = named.map(String::as_str).chain(["canvas", "grid"]);
    names.map(|name| Ok((name.to_string(), grid.cell(theme, template, Some(&json!({ "in": name })))?))).collect()
}

/// The tracks a 1-based range `n` or `[a, b]` spans, `[start, end]`, if the grid has them.
fn lines(tracks: &[[f32; 2]], range: Option<&Value>) -> Option<[f32; 2]> {
    let n = |v: &Value| v.as_u64().filter(|n| *n >= 1).map(|n| n as usize - 1);
    let (a, b) = match range? {
        Value::Array(ab) if ab.len() == 2 => (n(&ab[0])?, n(&ab[1])?),
        one => (n(one)?, n(one)?),
    };
    Some([tracks.get(a)?[0], tracks.get(b)?[1]])
}

/// The track whose `side` (0 its start, 1 its end) lies nearest `at`, from `from` on.
fn nearest(tracks: &[[f32; 2]], side: usize, at: f32, from: usize) -> usize {
    (from..tracks.len())
        .min_by(|&i, &j| (tracks[i][side] - at).abs().total_cmp(&(tracks[j][side] - at).abs()))
        .unwrap_or(from)
}

/// The tracks a box from `at`, `size` long, covers: from the one whose start is nearest its
/// start to the one whose end is nearest its end.
fn covered(tracks: &[[f32; 2]], at: f32, size: f32) -> (usize, usize) {
    let first = nearest(tracks, 0, at, 0);
    (first, nearest(tracks, 1, at + size, first))
}

/// The tracks a box from `at`, `size` long, stands in: those it overlaps by half the shorter
/// of the two or more, first to last; else the one nearest its middle.
fn stands(tracks: &[[f32; 2]], at: f32, size: f32) -> (usize, usize) {
    let end = at + size;
    let on: Vec<usize> = (0..tracks.len())
        .filter(|&i| tracks[i][1].min(end) - tracks[i][0].max(at) >= 0.5 * (tracks[i][1] - tracks[i][0]).min(size))
        .collect();
    if let (Some(&first), Some(&last)) = (on.first(), on.last()) {
        return (first, last);
    }
    let middle = at + size / 2.0;
    let off = |[a, b]: [f32; 2]| (a - middle).max(middle - b).max(0.0);
    let near = (0..tracks.len()).min_by(|&i, &j| off(tracks[i]).total_cmp(&off(tracks[j]))).unwrap_or(0);
    (near, near)
}

/// `count` tracks from the one whose start is nearest `at`, as many as there are.
fn shift(tracks: &[[f32; 2]], count: usize, at: f32) -> (usize, usize) {
    let count = count.clamp(1, tracks.len());
    let first = nearest(&tracks[..=tracks.len() - count], 0, at, 0);
    (first, first + count - 1)
}

/// A 0-based track range as a placement takes it: 1-based, a single track as its number.
fn range((a, b): (usize, usize)) -> Range {
    if a == b { Range::Index(a as u32 + 1) } else { Range::Span([a as u32 + 1, b as u32 + 1]) }
}

/// The named box `cell` covers most, by the share of the two together they have in common;
/// if it covers none, the one whose middle is nearest its middle.
fn most(slots: &[(String, Rect)], cell: Rect) -> Option<&(String, Rect)> {
    let area = |r: &Rect| r[2].max(0.0) * r[3].max(0.0);
    let shared = |r: &Rect| {
        let w = (r[0] + r[2]).min(cell[0] + cell[2]) - r[0].max(cell[0]);
        let h = (r[1] + r[3]).min(cell[1] + cell[3]) - r[1].max(cell[1]);
        let both = w.max(0.0) * h.max(0.0);
        let union = area(r) + area(&cell) - both;
        if union > 0.0 { both / union } else { 0.0 }
    };
    let gap = |r: &Rect| {
        let (dx, dy) = (r[0] + r[2] / 2.0 - cell[0] - cell[2] / 2.0, r[1] + r[3] / 2.0 - cell[1] - cell[3] / 2.0);
        dx * dx + dy * dy
    };
    let best = slots.iter().rev().max_by(|a, b| shared(&a.1).total_cmp(&shared(&b.1)))?;
    if shared(&best.1) > 0.0 { Some(best) } else { slots.iter().rev().min_by(|a, b| gap(&a.1).total_cmp(&gap(&b.1))) }
}

/// Whether `point` falls in `rect`, its edges included, or within [`SLOP`] of it along a
/// side shorter than twice that.
pub(crate) fn reaches(rect: Rect, point: [f32; 2]) -> bool {
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

    #[test]
    fn a_box_stands_in_the_tracks_it_overlaps_most() {
        // Three tracks 90 long, 24 apart.
        let tracks = [[0.0, 90.0], [114.0, 204.0], [228.0, 318.0]];
        // A box smaller than a track, in the first, near the second's start: the first.
        assert_eq!(stands(&tracks, 59.0, 30.0), (0, 0));
        // Across the gutter, mostly in the second: the second; across half of each, both.
        assert_eq!(stands(&tracks, 80.0, 100.0), (1, 1));
        assert_eq!(stands(&tracks, 40.0, 130.0), (0, 1));
        // In a gutter, overlapping neither: the one its middle is nearest.
        assert_eq!(stands(&tracks, 92.0, 10.0), (0, 0));
        assert_eq!(stands(&tracks, 106.0, 6.0), (1, 1));
        // Past the last: the last.
        assert_eq!(stands(&tracks, 400.0, 50.0), (2, 2));
    }
}

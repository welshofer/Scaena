//! Containers (SPEC §3.4, ADR-0008): `stack`, `grid`, and `frame` nodes lay their
//! children out with `taffy`; a `group` draws its children together and lays nothing out.
//!
//! A node joins a container by naming it in `at.parent`. A container's children flow in
//! `at.index` order (default 0), ties in `nodes` order, as CSS `order` does. A node with no
//! container, or in a group, is a *root*: `at` places it on the theme grid, and it fills
//! that box unless its `size` says otherwise. A root container lays its subtree out inside
//! its box.
//!
//! Inside a stack, text and images take the room their content needs and containers wrap
//! theirs, while shapes, charts, and shaders share what is left; everything stretches
//! across the stack. Inside a grid container, children fill their cells and never widen
//! a track (CSS's `min-width: 0`). Inside a frame,
//! `at.rect` is relative to the frame's padding, and a child with no `rect` fills it.
//!
//! Layout is per snapshot (SPEC §5): this runs once when a scene is built, never per frame.
//! Positions are not rounded to whole canvas units, as the theme grid's are not.

use crate::EngineError;
use crate::images::BundleImages;
use crate::layout::{AlignX, AlignY, Grid};
use crate::theme::Theme;
use scaena_core::Snapshot;
use scaena_core::displaylist::Rect;
use scaena_core::document::{Deck, NodeType, Props};
use scaena_core::model::values::{Size as SizeSpec, SizeValue};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use taffy::style_helpers::{auto, fr, length, line, percent};
use taffy::{
    AlignContent, AlignItems, AlignSelf, AvailableSpace, Dimension, Display, FlexDirection, GridPlacement,
    GridTemplateComponent, JustifyContent, LengthPercentage, LengthPercentageAuto, Line, NodeId, Position, Size, Style,
    TaffyTree, TrackSizingFunction,
};

/// A text leaf's size, given what is known of it and the room available.
pub type MeasureText<'m> =
    dyn FnMut(&str, Size<Option<f32>>, Size<AvailableSpace>) -> Result<Size<f32>, EngineError> + 'm;

/// Where every node of one snapshot goes.
#[derive(Debug, Clone, Default)]
pub struct Placement {
    /// Each node's box, canvas units. A group has none.
    pub boxes: HashMap<String, Rect>,
    /// Each node's container.
    pub parents: HashMap<String, String>,
    /// Every visible node in paint order, with its paint key: `(z, nodes index)` for each
    /// node from its root down to it. Keys sort in paint order: siblings by `z`, then
    /// `nodes` order, and a container under its children.
    pub order: Vec<(String, Vec<(i64, usize)>)>,
}

impl Placement {
    /// A node's opacity times its containers', up to its group: a group composites what
    /// is in it as one layer, at its own opacity (SPEC §3.4).
    pub fn opacity(&self, snap: &Snapshot, id: &str) -> f32 {
        let own = |id: &str| snap.nodes[id].get("opacity").and_then(Value::as_f64).unwrap_or(1.0) as f32;
        let mut opacity = own(id);
        let mut at = id;
        while let Some(parent) = self.parents.get(at).filter(|p| !self.is_group(p)) {
            opacity *= own(parent);
            at = parent;
        }
        opacity
    }

    /// Whether `id` is a group: a node with no box of its own.
    pub fn is_group(&self, id: &str) -> bool {
        !self.boxes.contains_key(id)
    }
}

fn is_container(kind: NodeType) -> bool {
    matches!(kind, NodeType::Stack | NodeType::Grid | NodeType::Frame | NodeType::Group)
}

fn kind_name(kind: NodeType) -> String {
    serde_json::to_value(kind).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default()
}

fn in_node(id: &str, message: String) -> EngineError {
    EngineError::Layout(format!("node `{id}`: {message}"))
}

/// Lay out a snapshot's containers: every node's box, its container, and paint order.
pub fn place(
    deck: &Deck,
    theme: &Theme,
    grid: &Grid,
    images: &BundleImages,
    snap: &Snapshot,
    measure: &mut MeasureText,
) -> Result<Placement, EngineError> {
    let kind = |id: &str| deck.nodes[id].node_type;
    let order_of = |id: &str| deck.nodes.get_index_of(id).expect("snapshot nodes are deck nodes");
    let mut placement = Placement::default();

    for (id, props) in &snap.nodes {
        let Some(parent) = props.get("at").and_then(|at| at.get("parent")) else { continue };
        let parent = parent.as_str().ok_or_else(|| in_node(id, format!("`at.parent` {parent} is not a node id")))?;
        if !snap.nodes.contains_key(parent) {
            return Err(in_node(id, format!("its container `{parent}` is not in state `{}`", snap.state_id)));
        }
        if !is_container(kind(parent)) {
            let what = kind_name(kind(parent));
            return Err(in_node(id, format!("`{parent}` is a {what} node, not a stack, grid, frame, or group")));
        }
        placement.parents.insert(id.clone(), parent.to_string());
    }
    for id in snap.nodes.keys().filter(|id| placement.parents.contains_key(*id)) {
        let (mut at, mut steps) = (id.as_str(), 0);
        while let Some(parent) = placement.parents.get(at) {
            (at, steps) = (parent.as_str(), steps + 1);
            if steps > placement.parents.len() {
                return Err(in_node(id, "containers nest in a loop".into()));
            }
        }
    }

    // Children: in flow order for layout, in paint order for drawing.
    let mut children: HashMap<&str, Vec<&str>> = HashMap::new();
    for (id, parent) in &placement.parents {
        children.entry(parent.as_str()).or_default().push(id.as_str());
    }
    let index = |id: &str| snap.nodes[id].get("at").and_then(|at| at.get("index")).and_then(Value::as_u64);
    for kids in children.values_mut() {
        kids.sort_by_key(|id| (index(id).unwrap_or(0), order_of(id)));
    }
    let z = |id: &str| snap.nodes[id].get("z").and_then(Value::as_i64).unwrap_or(0);
    let mut stack: Vec<(&str, Vec<(i64, usize)>)> = snap
        .nodes
        .keys()
        .filter(|id| !placement.parents.contains_key(*id))
        .map(|id| (id.as_str(), vec![(z(id), order_of(id))]))
        .collect();
    // Depth first: popping from the end, so push in reverse paint order.
    stack.sort_by(|a, b| b.1.cmp(&a.1));
    while let Some((id, key)) = stack.pop() {
        let mut kids: Vec<(&str, Vec<(i64, usize)>)> = children
            .get(id)
            .into_iter()
            .flatten()
            .map(|kid| {
                let mut k = key.clone();
                k.push((z(kid), order_of(kid)));
                (*kid, k)
            })
            .collect();
        kids.sort_by(|a, b| b.1.cmp(&a.1));
        placement.order.push((id.to_string(), key));
        stack.extend(kids);
    }

    // Boxes: each root on the theme grid, each container's subtree through taffy.
    let template = snap.layout.as_deref();
    for (id, props) in &snap.nodes {
        let in_group = placement.parents.get(id).is_some_and(|p| kind(p) == NodeType::Group);
        if (placement.parents.contains_key(id) && !in_group) || kind(id) == NodeType::Group {
            continue;
        }
        let at = props.get("at");
        if at.is_some_and(|at| at.get("area").is_some()) {
            return Err(in_node(id, "`at.area` names an area of a grid container, and it is not in one".into()));
        }
        let cell = grid.place(theme, template, at).map_err(|e| match e {
            EngineError::Layout(m) => in_node(id, m),
            other => other,
        })?;
        let sized = props.get("size").is_some();
        if kind(id) == NodeType::Text
            || (!matches!(kind(id), NodeType::Stack | NodeType::Grid | NodeType::Frame) && !sized)
        {
            placement.boxes.insert(id.clone(), cell);
            continue;
        }
        let mut flow =
            Flow { deck, theme, images, snap, children: &children, tree: TaffyTree::new(), ids: HashMap::new() };
        flow.tree.disable_rounding();
        let node = flow.build(id, &Within::Root)?;
        let (_, align_y) = Grid::alignment(theme, template, props)?;
        let wrapper = flow
            .tree
            .new_with_children(
                Style {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    size: Size { width: length(cell[2]), height: length(cell[3]) },
                    justify_content: Some(match align_y {
                        AlignY::Center => JustifyContent::CENTER,
                        AlignY::End | AlignY::Baseline => JustifyContent::FLEX_END,
                        _ => JustifyContent::FLEX_START,
                    }),
                    align_items: Some(AlignItems::STRETCH),
                    ..Style::default()
                },
                &[node],
            )
            .map_err(taffy_error)?;
        let mut failed = None;
        flow.tree
            .compute_layout_with_measure(
                wrapper,
                Size { width: AvailableSpace::Definite(cell[2]), height: AvailableSpace::Definite(cell[3]) },
                |inputs, _, context, style| {
                    taffy::compute_leaf_layout(
                        inputs,
                        style,
                        |_, _| 0.0,
                        |known, available| match context {
                            Some(Leaf::Text(id)) if failed.is_none() => match measure(id, known, available) {
                                Ok(size) => size,
                                Err(e) => {
                                    failed = Some(e);
                                    Size::ZERO
                                }
                            },
                            Some(Leaf::Image { width, height }) => image_size(*width, *height, known, available),
                            _ => Size::ZERO,
                        },
                    )
                },
            )
            .map_err(taffy_error)?;
        if let Some(e) = failed {
            return Err(e);
        }
        flow.read(node, [cell[0], cell[1]], &mut placement.boxes)?;
    }
    Ok(placement)
}

fn taffy_error(e: taffy::TaffyError) -> EngineError {
    EngineError::Layout(format!("container layout: {e}"))
}

/// What a leaf measures by.
enum Leaf {
    Text(String),
    /// The part of the image that shows, in pixels, taken as canvas units.
    Image {
        width: f32,
        height: f32,
    },
}

/// An image's size: its natural size, scaled to keep its aspect ratio when one side is
/// known, and scaled down to fit the width available.
fn image_size(width: f32, height: f32, known: Size<Option<f32>>, available: Size<AvailableSpace>) -> Size<f32> {
    match (known.width, known.height) {
        (Some(w), Some(h)) => Size { width: w, height: h },
        (Some(w), None) => Size { width: w, height: w * height / width },
        (None, Some(h)) => Size { width: h * width / height, height: h },
        (None, None) => {
            let scale = match available.width {
                AvailableSpace::Definite(w) if w < width => w / width,
                AvailableSpace::MinContent => 0.0,
                _ => 1.0,
            };
            Size { width: width * scale, height: height * scale }
        }
    }
}

/// What a node sits in, for its own style.
enum Within {
    /// A root, alone in its cell.
    Root,
    Stack {
        row: bool,
    },
    Grid(BTreeMap<String, [u16; 4]>),
    Frame {
        padding: [f32; 4],
    },
}

struct Flow<'a> {
    deck: &'a Deck,
    theme: &'a Theme,
    images: &'a BundleImages,
    snap: &'a Snapshot,
    children: &'a HashMap<&'a str, Vec<&'a str>>,
    tree: TaffyTree<Leaf>,
    ids: HashMap<NodeId, String>,
}

impl Flow<'_> {
    fn build(&mut self, id: &str, within: &Within) -> Result<NodeId, EngineError> {
        let props = &self.snap.nodes[id];
        let kind = self.deck.nodes[id].node_type;
        let natural = (kind == NodeType::Image).then(|| natural_size(self.images, props)).flatten();
        let mut style = item_style(self.theme, props, kind, within, natural).map_err(|e| in_node(id, e))?;
        let kids: Vec<&str> = self.children.get(id).cloned().unwrap_or_default();
        let node = match kind {
            NodeType::Stack | NodeType::Grid | NodeType::Frame => {
                let padding = padding(self.theme, props).map_err(|e| in_node(id, e))?;
                style.padding = taffy::Rect {
                    top: LengthPercentage::length(padding[0]),
                    right: LengthPercentage::length(padding[1]),
                    bottom: LengthPercentage::length(padding[2]),
                    left: LengthPercentage::length(padding[3]),
                };
                let gap = match props.get("gap") {
                    Some(g) => self.theme.length(g, 0.0).map_err(|e| in_node(id, e.to_string()))?,
                    None => 0.0,
                };
                style.gap = Size { width: LengthPercentage::length(gap), height: LengthPercentage::length(gap) };
                let child_within = match kind {
                    NodeType::Stack => {
                        let row = props.get("axis").and_then(Value::as_str) == Some("x");
                        style.display = Display::Flex;
                        style.flex_direction = if row { FlexDirection::Row } else { FlexDirection::Column };
                        style.align_items = Some(AlignItems::STRETCH);
                        style.justify_content = Some(match props.get("distribute").and_then(Value::as_str) {
                            None | Some("start") => JustifyContent::FLEX_START,
                            Some("center") => JustifyContent::CENTER,
                            Some("end") => JustifyContent::FLEX_END,
                            Some("between") => JustifyContent::SPACE_BETWEEN,
                            Some("around") => JustifyContent::SPACE_AROUND,
                            Some("evenly") => JustifyContent::SPACE_EVENLY,
                            Some(other) => return Err(in_node(id, format!("`distribute` `{other}`"))),
                        });
                        Within::Stack { row }
                    }
                    NodeType::Grid => {
                        let areas = areas(props).map_err(|e| in_node(id, e))?;
                        let (area_cols, area_rows) =
                            areas.values().fold((0, 0), |(c, r), a| (c.max(a[1]), r.max(a[3])));
                        style.display = Display::Grid;
                        style.grid_template_columns =
                            tracks(self.theme, props.get("cols"), area_cols).map_err(|e| in_node(id, e))?;
                        style.grid_template_rows =
                            tracks(self.theme, props.get("rows"), area_rows).map_err(|e| in_node(id, e))?;
                        style.align_content = Some(AlignContent::STRETCH);
                        Within::Grid(areas)
                    }
                    _ => {
                        style.display = Display::Flex;
                        Within::Frame { padding }
                    }
                };
                let mut nodes = Vec::with_capacity(kids.len());
                for kid in kids {
                    if self.deck.nodes[kid].node_type == NodeType::Group {
                        return Err(in_node(kid, format!("a group cannot sit in {} `{id}`", kind_name(kind))));
                    }
                    nodes.push(self.build(kid, &child_within)?);
                }
                self.tree.new_with_children(style, &nodes).map_err(taffy_error)?
            }
            NodeType::Text => {
                self.tree.new_leaf_with_context(style, Leaf::Text(id.to_string())).map_err(taffy_error)?
            }
            NodeType::Image => match natural {
                Some((width, height)) => {
                    self.tree.new_leaf_with_context(style, Leaf::Image { width, height }).map_err(taffy_error)?
                }
                None => self.tree.new_leaf(style).map_err(taffy_error)?,
            },
            NodeType::Group => {
                return Err(in_node(id, "a group cannot sit in a stack, grid, or frame".into()));
            }
            NodeType::Shape | NodeType::Chart | NodeType::Table | NodeType::Shader => {
                self.tree.new_leaf(style).map_err(taffy_error)?
            }
        };
        self.ids.insert(node, id.to_string());
        Ok(node)
    }

    /// Each laid-out node's box, from `node` down, canvas units; `origin` is the top-left
    /// corner of `node`'s parent. As on the slide, `at.offset` moves a node (and what is in
    /// it) after layout, and `at.inset` shrinks a node's own box on every side.
    fn read(&self, node: NodeId, origin: [f32; 2], boxes: &mut HashMap<String, Rect>) -> Result<(), EngineError> {
        let layout = self.tree.layout(node).map_err(taffy_error)?;
        let id = &self.ids[&node];
        let at = self.snap.nodes[id].get("at");
        let [dx, dy] = at
            .and_then(|at| at.get("offset"))
            .and_then(|o| serde_json::from_value(o.clone()).ok())
            .unwrap_or([0.0f32; 2]);
        let inset = at.and_then(|at| at.get("inset")).and_then(Value::as_f64).unwrap_or(0.0) as f32;
        let (x, y) = (origin[0] + layout.location.x + dx, origin[1] + layout.location.y + dy);
        let (w, h) = (layout.size.width, layout.size.height);
        boxes.insert(id.clone(), [x + inset, y + inset, w - 2.0 * inset, h - 2.0 * inset]);
        for child in self.tree.children(node).map_err(taffy_error)? {
            self.read(child, [x, y], boxes)?;
        }
        Ok(())
    }
}

/// The part of an image node's picture that shows (its `crop`), in pixels.
fn natural_size(images: &BundleImages, props: &Props) -> Option<(f32, f32)> {
    let info = images.get(props.get("src")?.as_str()?)?;
    let crop: [f32; 4] =
        props.get("crop").and_then(|c| serde_json::from_value(c.clone()).ok()).unwrap_or([0.0, 0.0, 1.0, 1.0]);
    let (w, h) = (info.width as f32 * crop[2], info.height as f32 * crop[3]);
    (w > 0.0 && h > 0.0).then_some((w, h))
}

/// Shapes, charts, tables, and shaders have no size of their own: in a stack they share
/// the room.
fn grows(kind: NodeType) -> bool {
    matches!(kind, NodeType::Shape | NodeType::Chart | NodeType::Table | NodeType::Shader)
}

/// A node's style as a child of `within`: its `size` and its alignment.
fn item_style(
    theme: &Theme,
    props: &Props,
    kind: NodeType,
    within: &Within,
    natural: Option<(f32, f32)>,
) -> Result<Style, String> {
    let size: SizeSpec = match props.get("size") {
        Some(s) => serde_json::from_value(s.clone()).map_err(|e| format!("`size`: {e}"))?,
        None => SizeSpec { w: None, h: None, min_w: None, max_w: None, min_h: None, max_h: None, aspect: None },
    };
    let (align_x, align_y) = Grid::alignment(theme, None, props).map_err(|e| e.to_string())?;
    let mut style = Style::default();
    let bound = |v: &Option<scaena_core::model::values::Length>| -> Result<LengthPercentageAuto, String> {
        match v {
            Some(l) => Ok(match dimension(theme, &serde_json::to_value(l).expect("a length is JSON"))? {
                Extent::Length(n) => LengthPercentageAuto::length(n),
                Extent::Percent(p) => LengthPercentageAuto::percent(p),
            }),
            None => Ok(auto()),
        }
    };
    style.min_size = Size { width: bound(&size.min_w)?, height: bound(&size.min_h)? };
    style.max_size = Size { width: bound(&size.max_w)?, height: bound(&size.max_h)? };
    if let Some(aspect) = &size.aspect {
        let (w, h) = aspect.split_once(':').ok_or_else(|| format!("`aspect` {aspect}: w:h"))?;
        let (w, h): (f32, f32) = (w.parse().map_err(|_| "`aspect`: w:h")?, h.parse().map_err(|_| "`aspect`: w:h")?);
        style.aspect_ratio = (w > 0.0 && h > 0.0).then_some(w / h);
    }

    // How one axis is sized: a length, `fit` (its content), or a share of the room.
    enum Axis {
        Fixed(Dimension),
        Fit,
        Share(f32),
    }
    let axis = |v: &Option<SizeValue>| -> Result<Option<Axis>, String> {
        let Some(v) = v else { return Ok(None) };
        Ok(Some(match sizing(theme, &serde_json::to_value(v).expect("a size is JSON"))? {
            Sizing::Extent(Extent::Length(n)) => Axis::Fixed(Dimension::length(n)),
            Sizing::Extent(Extent::Percent(p)) => Axis::Fixed(Dimension::percent(p)),
            Sizing::Fit => Axis::Fit,
            Sizing::Share(n) => Axis::Share(n),
        }))
    };
    let (w, h) = (axis(&size.w)?, axis(&size.h)?);
    let cross_align = |a: Option<AlignSelf>, fit: bool| -> Option<AlignSelf> {
        match a {
            Some(a) => Some(a),
            None if fit => Some(AlignSelf::START),
            None => Some(AlignSelf::STRETCH),
        }
    };
    let x_align = match align_x {
        AlignX::Start => None,
        AlignX::Center => Some(AlignSelf::CENTER),
        AlignX::End => Some(AlignSelf::END),
        AlignX::Stretch => Some(AlignSelf::STRETCH),
    };
    // The typographic anchors place text inside a box that spans the row.
    let y_align = match align_y {
        AlignY::Start => None,
        AlignY::Center => Some(AlignSelf::CENTER),
        AlignY::End => Some(AlignSelf::END),
        AlignY::Stretch | AlignY::Cap | AlignY::Baseline | AlignY::XHeight => Some(AlignSelf::STRETCH),
    };
    match within {
        Within::Root | Within::Stack { .. } => {
            let row = matches!(within, Within::Stack { row: true });
            let (main, cross, cross_alignment) = if row { (&w, &h, y_align) } else { (&h, &w, x_align) };
            // A root fills its cell; in a stack, a node with no size of its own shares the room.
            let default_share = matches!(within, Within::Root) || grows(kind);
            match main {
                Some(Axis::Fixed(d)) => {
                    set_main(&mut style, row, *d);
                    style.flex_shrink = 0.0;
                }
                Some(Axis::Fit) => style.flex_grow = 0.0,
                Some(Axis::Share(n)) => {
                    style.flex_grow = *n;
                    style.flex_basis = Dimension::length(0.0);
                }
                None if default_share => {
                    style.flex_grow = 1.0;
                    style.flex_basis = Dimension::length(0.0);
                }
                None => {}
            }
            match cross {
                Some(Axis::Fixed(d)) => {
                    set_main(&mut style, !row, *d);
                    style.align_self = cross_alignment.or(Some(AlignSelf::START));
                }
                Some(Axis::Fit) => style.align_self = cross_align(cross_alignment, true),
                Some(Axis::Share(_)) => style.align_self = Some(AlignSelf::STRETCH),
                None => style.align_self = cross_align(cross_alignment, false),
            }
            // An image keeps its picture's shape as it stretches across a stack.
            if let (Within::Stack { .. }, Some((nw, nh)), None) = (within, natural, style.aspect_ratio) {
                style.aspect_ratio = Some(nw / nh);
            }
        }
        Within::Grid(areas) => {
            let at = props.get("at");
            if let Some(area) = at.and_then(|at| at.get("area")).and_then(Value::as_str) {
                let [c0, c1, r0, r1] = areas.get(area).ok_or_else(|| format!("area `{area}` is not in the grid"))?;
                style.grid_column = lines(*c0, *c1);
                style.grid_row = lines(*r0, *r1);
            } else {
                if let Some(col) = at.and_then(|at| at.get("col")) {
                    let (a, b) = range(col, "col")?;
                    style.grid_column = lines(a, b);
                }
                if let Some(row) = at.and_then(|at| at.get("row")) {
                    let (a, b) = range(row, "row")?;
                    style.grid_row = lines(a, b);
                }
            }
            let self_align = |axis: &Option<Axis>, align: Option<AlignSelf>, style_size: &mut Dimension| match axis {
                Some(Axis::Fixed(d)) => {
                    *style_size = *d;
                    align.or(Some(AlignSelf::START))
                }
                Some(Axis::Fit) => align.or(Some(AlignSelf::START)),
                Some(Axis::Share(_)) => Some(AlignSelf::STRETCH),
                None => align.or(Some(AlignSelf::STRETCH)),
            };
            style.justify_self = self_align(&w, x_align, &mut style.size.width);
            style.align_self = self_align(&h, y_align, &mut style.size.height);
            // A child fills its cell and never widens its track, as CSS's `min-width: 0`
            // has it: an image whose height the row gives would otherwise ask for the width
            // its picture's shape takes, past a narrow grid's edge.
            if size.min_w.is_none() {
                style.min_size.width = LengthPercentageAuto::length(0.0);
            }
            if size.min_h.is_none() {
                style.min_size.height = LengthPercentageAuto::length(0.0);
            }
        }
        Within::Frame { padding } => {
            style.position = Position::Absolute;
            match props.get("at").and_then(|at| at.get("rect")) {
                Some(rect) => {
                    let [x, y, w, h]: [f32; 4] = serde_json::from_value(rect.clone())
                        .map_err(|_| format!("`rect` {rect}: [x, y, w, h], relative to the frame's padding"))?;
                    style.inset = taffy::Rect {
                        left: LengthPercentageAuto::length(padding[3] + x),
                        top: LengthPercentageAuto::length(padding[0] + y),
                        right: auto(),
                        bottom: auto(),
                    };
                    style.size = Size { width: length(w), height: length(h) };
                }
                None => {
                    style.inset = taffy::Rect {
                        top: LengthPercentageAuto::length(padding[0]),
                        right: LengthPercentageAuto::length(padding[1]),
                        bottom: LengthPercentageAuto::length(padding[2]),
                        left: LengthPercentageAuto::length(padding[3]),
                    };
                }
            }
        }
    }
    Ok(style)
}

fn set_main(style: &mut Style, row: bool, d: Dimension) {
    if row {
        style.size.width = d;
    } else {
        style.size.height = d;
    }
}

/// A length as taffy takes it: canvas units, or a percentage of the container.
enum Extent {
    Length(f32),
    Percent(f32),
}

/// A size value (SPEC §3.4): a length, `fit`, or a share of the room: `fill` is
/// `fraction(1)`. Read by hand: as JSON, `"fit"` would also pass for a length token.
enum Sizing {
    Extent(Extent),
    Fit,
    Share(f32),
}

fn sizing(theme: &Theme, v: &Value) -> Result<Sizing, String> {
    match v.as_str() {
        Some("fit") => Ok(Sizing::Fit),
        Some("fill") => Ok(Sizing::Share(1.0)),
        Some(f) if f.starts_with("fraction(") => {
            let n = f.strip_prefix("fraction(").and_then(|f| f.strip_suffix(')')).and_then(|n| n.parse::<f32>().ok());
            n.filter(|n| *n > 0.0).map(Sizing::Share).ok_or_else(|| format!("`{f}`: fraction(n), n above 0"))
        }
        _ => dimension(theme, v).map(Sizing::Extent),
    }
}

fn dimension(theme: &Theme, v: &Value) -> Result<Extent, String> {
    if let Some(p) = v.as_str().and_then(|s| s.strip_suffix('%')) {
        return p.parse::<f32>().map(|p| Extent::Percent(p / 100.0)).map_err(|_| format!("{v} is not a percentage"));
    }
    theme.length(v, 0.0).map(Extent::Length).map_err(|e| e.to_string())
}

/// `padding` as `[top, right, bottom, left]`, CSS shorthand.
fn padding(theme: &Theme, props: &Props) -> Result<[f32; 4], String> {
    let Some(v) = props.get("padding") else { return Ok([0.0; 4]) };
    let one = |v: &Value| theme.length(v, 0.0).map_err(|e| e.to_string());
    let sides: Vec<f32> = match v {
        Value::Array(sides) => sides.iter().map(one).collect::<Result<_, _>>()?,
        single => vec![one(single)?],
    };
    Ok(match sides[..] {
        [all] => [all; 4],
        [v, h] => [v, h, v, h],
        [t, h, b] => [t, h, b, h],
        [t, r, b, l] => [t, r, b, l],
        _ => return Err(format!("`padding` {v}: one to four lengths")),
    })
}

/// A grid container's tracks along one axis: a count of equal shares, or each track's
/// size; with neither, as many equal shares as its `areas` have, else tracks made as needed.
fn tracks(theme: &Theme, v: Option<&Value>, from_areas: u16) -> Result<Vec<GridTemplateComponent<String>>, String> {
    let share = |n: f32| GridTemplateComponent::Single(fr::<f32, TrackSizingFunction>(n));
    match v {
        None => Ok((0..from_areas).map(|_| share(1.0)).collect()),
        Some(Value::Number(n)) => {
            let n = n.as_u64().filter(|n| *n >= 1).ok_or_else(|| format!("{n} tracks: a count, at least 1"))?;
            Ok((0..n).map(|_| share(1.0)).collect())
        }
        Some(Value::Array(sizes)) => sizes
            .iter()
            .map(|s| {
                Ok(match sizing(theme, s)? {
                    Sizing::Extent(Extent::Length(n)) => {
                        GridTemplateComponent::Single(length::<f32, TrackSizingFunction>(n))
                    }
                    Sizing::Extent(Extent::Percent(p)) => {
                        GridTemplateComponent::Single(percent::<f32, TrackSizingFunction>(p))
                    }
                    Sizing::Fit => GridTemplateComponent::Single(auto()),
                    Sizing::Share(n) => share(n),
                })
            })
            .collect(),
        Some(other) => Err(format!("tracks {other}: a count or a list of sizes")),
    }
}

/// A grid container's named areas: each name's `[first col, last col, first row, last row]`,
/// 1-based, from rows of names as CSS `grid-template-areas` writes them.
fn areas(props: &Props) -> Result<BTreeMap<String, [u16; 4]>, String> {
    let Some(v) = props.get("areas") else { return Ok(BTreeMap::new()) };
    let rows: Vec<String> = serde_json::from_value(v.clone()).map_err(|_| format!("`areas` {v}: rows of names"))?;
    let cells: Vec<Vec<&str>> = rows.iter().map(|r| r.split_whitespace().collect()).collect();
    let width = cells.first().map_or(0, Vec::len);
    if width == 0 || cells.iter().any(|r| r.len() != width) {
        return Err(format!("`areas` {v}: every row names the same number of cells"));
    }
    let mut found: BTreeMap<String, [u16; 4]> = BTreeMap::new();
    for (r, row) in cells.iter().enumerate() {
        for (c, name) in row.iter().enumerate().filter(|(_, n)| **n != ".") {
            let (c, r) = (c as u16 + 1, r as u16 + 1);
            let a = found.entry(name.to_string()).or_insert([c, c, r, r]);
            *a = [a[0].min(c), a[1].max(c), a[2].min(r), a[3].max(r)];
        }
    }
    for (name, [c0, c1, r0, r1]) in &found {
        let filled = (*r0..=*r1).all(|r| (*c0..=*c1).all(|c| cells[r as usize - 1][c as usize - 1] == name.as_str()));
        if !filled {
            return Err(format!("area `{name}` is not a rectangle"));
        }
    }
    Ok(found)
}

/// Grid lines spanning tracks `a` to `b`, 1-based and inclusive.
fn lines(a: u16, b: u16) -> Line<GridPlacement<String>> {
    Line { start: line(a as i16), end: line(b as i16 + 1) }
}

/// A 1-based track range: `n` or `[a, b]`.
fn range(v: &Value, axis: &str) -> Result<(u16, u16), String> {
    let n = |v: &Value| v.as_u64().filter(|n| (1..=1000).contains(n)).map(|n| n as u16);
    match v {
        Value::Number(_) => n(v).map(|n| (n, n)),
        Value::Array(ab) if ab.len() == 2 => n(&ab[0]).zip(n(&ab[1])).filter(|(a, b)| a <= b),
        _ => None,
    }
    .ok_or_else(|| format!("`{axis}` {v}: n or [a, b], from 1"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn theme() -> Theme {
        Theme::from_json(include_str!("../../../tests/fixtures/torture.scaena/theme.json")).unwrap()
    }

    /// A one-state deck of `nodes`, every node shown.
    fn deck(nodes: Value) -> Deck {
        let props: serde_json::Map<String, Value> =
            nodes.as_object().unwrap().keys().map(|k| (k.clone(), json!({}))).collect();
        let deck = json!({
            "scaena": scaena_core::FORMAT_VERSION,
            "canvas": { "width": 1920, "height": 1080 },
            "nodes": nodes,
            "states": [{ "id": "one", "props": props }]
        });
        Deck::from_json(&deck.to_string()).unwrap()
    }

    /// Text as 20 cu a character on 50 cu lines, wrapped at the width it gets.
    fn fake_text(deck: &Deck) -> Box<MeasureText<'static>> {
        let chars: HashMap<String, f32> = deck
            .nodes
            .iter()
            .filter_map(|(id, n)| Some((id.clone(), n.props.get("text")?.as_str()?.chars().count() as f32 * 20.0)))
            .collect();
        Box::new(move |id, known, available| {
            let wide = chars[id];
            let room = known.width.or(match available.width {
                AvailableSpace::Definite(w) => Some(w),
                AvailableSpace::MinContent => Some(0.0),
                AvailableSpace::MaxContent => None,
            });
            let width = room.map_or(wide, |r| wide.min(r.max(20.0)));
            let lines = (wide / width).ceil().max(1.0);
            Ok(Size { width: known.width.unwrap_or(width), height: known.height.unwrap_or(50.0 * lines) })
        })
    }

    fn place_deck(deck: &Deck) -> Result<Placement, EngineError> {
        let theme = theme();
        let grid = Grid::from_theme(&theme, [1920.0, 1080.0]).unwrap();
        let snap = &scaena_core::resolve_states(deck).unwrap()[0];
        let mut images = BundleImages::new();
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend(400u32.to_be_bytes());
        png.extend(200u32.to_be_bytes());
        png.extend([8, 6, 0, 0, 0]);
        images.register("assets/photo.png", &png).unwrap();
        place(deck, &theme, &grid, &images, snap, &mut *fake_text(deck))
    }

    fn boxes(nodes: Value) -> HashMap<String, Rect> {
        place_deck(&deck(nodes)).unwrap().boxes
    }

    #[test]
    fn a_stack_hugs_text_and_shares_the_rest_with_shapes() {
        let b = boxes(json!({
            "col": { "type": "stack", "gap": 10, "padding": 20, "at": { "rect": [100, 100, 600, 500] } },
            "title": { "type": "text", "text": "Revenue", "at": { "parent": "col" } },
            "rule": { "type": "shape", "kind": "line", "at": { "parent": "col" }, "size": { "h": 4 } },
            "panel": { "type": "shape", "fill": "accent", "at": { "parent": "col" } }
        }));
        assert_eq!(b["col"], [100.0, 100.0, 600.0, 500.0]);
        // Inside the padding, across the stack; the text as tall as its line.
        assert_eq!(b["title"], [120.0, 120.0, 560.0, 50.0]);
        assert_eq!(b["rule"], [120.0, 180.0, 560.0, 4.0]);
        // The panel takes what is left: 460 tall inside, less 50 + 4 and two gaps.
        assert_eq!(b["panel"], [120.0, 194.0, 560.0, 386.0]);
    }

    #[test]
    fn a_row_distributes_and_index_reorders() {
        let b = boxes(json!({
            "row": { "type": "stack", "axis": "x", "distribute": "between", "at": { "rect": [0, 0, 1000, 100] } },
            "a": { "type": "shape", "at": { "parent": "row" }, "size": { "w": 100 } },
            "b": { "type": "shape", "at": { "parent": "row", "index": 2 }, "size": { "w": 100 } },
            "c": { "type": "shape", "at": { "parent": "row", "index": 1 }, "size": { "w": 100, "h": "fit" } }
        }));
        assert_eq!(b["a"], [0.0, 0.0, 100.0, 100.0]);
        assert_eq!(b["c"][0], 450.0, "index 1 comes before index 2");
        assert_eq!(b["c"][3], 0.0, "a shape that fits its content has none");
        assert_eq!(b["b"], [900.0, 0.0, 100.0, 100.0]);
    }

    #[test]
    fn fractions_and_fixed_sizes_share_a_row() {
        let b = boxes(json!({
            "row": { "type": "stack", "axis": "x", "gap": "space.3", "at": { "rect": [0, 0, 1032, 200] } },
            "one": { "type": "shape", "at": { "parent": "row" }, "size": { "w": "fraction(1)" } },
            "two": { "type": "shape", "at": { "parent": "row" }, "size": { "w": "fraction(2)" } },
            "side": { "type": "chart", "kind": "bar", "data": "@q", "at": { "parent": "row" }, "size": { "w": "25%" } }
        }));
        // space.3 is 16: 1032 - 2 gaps = 1000; the chart takes 25% of the row's 1032.
        assert_eq!(b["side"][2], 258.0);
        let shared = 1000.0 - 258.0;
        assert!((b["one"][2] - shared / 3.0).abs() < 1e-3 && (b["two"][2] - shared * 2.0 / 3.0).abs() < 1e-3, "{b:?}");
    }

    #[test]
    fn an_image_keeps_its_shape_across_a_stack() {
        let b = boxes(json!({
            "col": { "type": "stack", "at": { "rect": [0, 0, 600, 1000] } },
            "photo": { "type": "image", "src": "assets/photo.png", "alt": "", "at": { "parent": "col" } },
            "crop": { "type": "image", "src": "assets/photo.png", "alt": "", "crop": [0, 0, 0.5, 1], "at": { "parent": "col" }, "size": { "w": "fit" } }
        }));
        // 400 × 200 stretched to 600 wide: 300 tall.
        assert_eq!(b["photo"], [0.0, 0.0, 600.0, 300.0]);
        // Its left half, 200 × 200, at its own size.
        assert_eq!(b["crop"], [0.0, 300.0, 200.0, 200.0]);
    }

    #[test]
    fn a_grid_container_places_by_area_line_and_flow() {
        let b = boxes(json!({
            "board": { "type": "grid", "areas": ["head head", "left right"], "rows": [100, "fill"], "gap": 20,
                       "at": { "rect": [0, 0, 1020, 620] } },
            "head": { "type": "shape", "at": { "parent": "board", "area": "head" } },
            "left": { "type": "shape", "at": { "parent": "board", "col": 1, "row": 2 } },
            "right": { "type": "text", "text": "Notes", "at": { "parent": "board" }, "size": { "h": "fit" } }
        }));
        assert_eq!(b["head"], [0.0, 0.0, 1020.0, 100.0]);
        assert_eq!(b["left"], [0.0, 120.0, 500.0, 500.0]);
        // Flowed into the next free cell, as tall as its line.
        assert_eq!(b["right"], [520.0, 120.0, 500.0, 50.0]);
    }

    #[test]
    fn offset_and_inset_work_in_a_container_as_on_the_slide() {
        let b = boxes(json!({
            "col": { "type": "stack", "at": { "rect": [0, 0, 100, 300] }, "gap": 0 },
            "a": { "type": "shape", "at": { "parent": "col", "inset": 10 } },
            "b": { "type": "shape", "at": { "parent": "col", "offset": [5, -5] } }
        }));
        assert_eq!(b["a"], [10.0, 10.0, 80.0, 130.0]);
        assert_eq!(b["b"], [5.0, 145.0, 100.0, 150.0], "moved, without moving its neighbor");
    }

    #[test]
    fn a_frame_places_by_rect_inside_its_padding() {
        let b = boxes(json!({
            "card": { "type": "frame", "padding": [10, 20], "at": { "rect": [100, 100, 400, 300] } },
            "tag": { "type": "shape", "at": { "parent": "card", "rect": [0, 0, 80, 30] } },
            "ground": { "type": "shape", "at": { "parent": "card" } }
        }));
        assert_eq!(b["tag"], [120.0, 110.0, 80.0, 30.0]);
        assert_eq!(b["ground"], [120.0, 110.0, 360.0, 280.0]);
    }

    #[test]
    fn a_root_with_a_size_aligns_in_its_cell() {
        let b = boxes(json!({
            "photo": { "type": "image", "src": "assets/photo.png", "alt": "", "at": { "rect": [0, 0, 1000, 600] },
                       "size": { "w": 400, "h": 200 }, "align": "center" },
            "plain": { "type": "shape", "at": { "rect": [0, 0, 1000, 600] } }
        }));
        assert_eq!(b["photo"], [300.0, 200.0, 400.0, 200.0]);
        assert_eq!(b["plain"], [0.0, 0.0, 1000.0, 600.0], "no size: the cell, as before");
    }

    #[test]
    fn groups_place_on_the_slide_and_paint_their_children() {
        let p = place_deck(&deck(json!({
            "late": { "type": "shape", "z": 1, "at": { "rect": [0, 0, 10, 10] } },
            "g": { "type": "group", "opacity": 0.5 },
            "b": { "type": "shape", "at": { "parent": "g", "rect": [5, 5, 10, 10] }, "opacity": 0.5 },
            "a": { "type": "shape", "z": -1, "at": { "parent": "g", "rect": [0, 0, 10, 10] } }
        })))
        .unwrap();
        let order: Vec<&str> = p.order.iter().map(|(id, _)| id.as_str()).collect();
        // The group, then its children by z; `late` above everything at z 1.
        assert_eq!(order, ["g", "a", "b", "late"]);
        assert_eq!(p.boxes["b"], [5.0, 5.0, 10.0, 10.0]);
        assert!(!p.boxes.contains_key("g"));
        let snap = &scaena_core::resolve_states(&deck(json!({ "g": { "type": "group", "opacity": 0.5 },
            "b": { "type": "shape", "at": { "parent": "g" }, "opacity": 0.5 } })))
        .unwrap()[0];
        assert_eq!((p.opacity(snap, "b"), p.opacity(snap, "g")), (0.5, 0.5), "a group composites at its own opacity");
    }

    #[test]
    fn bad_containers_say_which_node() {
        let err = |nodes: Value| place_deck(&deck(nodes)).unwrap_err().to_string();
        let e = err(json!({ "t": { "type": "text", "text": "x" }, "n": { "type": "shape", "at": { "parent": "t" } } }));
        assert!(e.contains("node `n`") && e.contains("not a stack, grid, frame, or group"), "{e}");
        let e = err(
            json!({ "a": { "type": "stack", "at": { "parent": "b" } }, "b": { "type": "stack", "at": { "parent": "a" } } }),
        );
        assert!(e.contains("loop"), "{e}");
        let e = err(json!({ "s": { "type": "stack" }, "g": { "type": "group", "at": { "parent": "s" } } }));
        assert!(e.contains("a group cannot sit in"), "{e}");
        let e = err(
            json!({ "g": { "type": "grid", "areas": ["a b", "b b"] }, "n": { "type": "shape", "at": { "parent": "g" } } }),
        );
        assert!(e.contains("area `b` is not a rectangle"), "{e}");
        let e = err(json!({ "n": { "type": "shape", "at": { "area": "head" } } }));
        assert!(e.contains("not in one"), "{e}");
    }
}

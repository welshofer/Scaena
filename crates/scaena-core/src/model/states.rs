//! A state's transition and choreography (SPEC §3.9). Its deltas are typed by the node
//! types themselves: see `StateDelta` in [`super::deck_schema`].

use super::values::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The transition into a state: a bare duration, or the object form (SPEC §3.9). A state
/// without one cuts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Transition {
    Duration(Duration),
    Spec(TransitionSpec),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransitionSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<Duration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ease: Option<Easing>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spring: Option<Spring>,
    #[serde(rename = "match", default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("default" = "id"))]
    pub match_by: Option<MatchBy>,
    /// How the slide changes, as a presentation app's slide transitions do (PLAN 2.111): the
    /// slide left and the slide entered each drawn whole, one moving, uncovered, or faded over the
    /// other, nothing morphing between them. Without one, what both states show morphs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<TransitionStyle>,
    /// Which way a `push`, a `cover`, a `reveal`, a `wipe`, a `circleWave`, or a `diamondWave`
    /// moves: `left`, the default, which brings the slide entered in from the right, `right`, `up`,
    /// or `down`. A `radial`'s hand starts so the slide entered shows first on that side: straight
    /// up for `left`, at three o'clock for `up`, six for `right`, and nine for `down`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("default" = "left"))]
    pub direction: Option<TransitionDirection>,
}

/// A slide transition (PLAN 2.111).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum TransitionStyle {
    /// The slide entered fades in over the slide left.
    Dissolve,
    /// The slide entered pushes the slide left off the canvas.
    Push,
    /// The slide entered slides in over the slide left.
    Cover,
    /// The slide left slides away, uncovering the slide entered.
    Reveal,
    /// An edge sweeps across, the slide entered behind it.
    Wipe,
    /// Circles grow across the slide, the slide entered inside them (PLAN 2.149).
    Circle,
    /// Circles grow in a wave across the slide, the way `direction` goes.
    CircleWave,
    /// Diamonds grow across the slide together.
    Diamond,
    /// Diamonds grow in a wave across the slide, the way `direction` goes.
    DiamondWave,
    /// A clock's hand sweeps round the slide's middle, the slide entered in its wake.
    Radial,
}

/// Which way a slide transition moves (PLAN 2.111).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TransitionDirection {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MatchBy {
    Id,
    None,
}

/// One unit of a state's choreography: a target's motion, or a sequence or parallel group.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ChoreoItem {
    Target(Box<ChoreoTarget>),
    Sequence(ChoreoSequence),
    Parallel(ChoreoParallel),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChoreoTarget {
    pub target: Targets,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split: Option<SplitUnit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enter: Option<PresetRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<PresetRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emphasis: Option<PresetRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anim: Option<AnimTracks>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stagger: Option<NonNegative>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay: Option<NonNegative>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<Duration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ease: Option<Easing>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spring: Option<Spring>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("default" = "after"))]
    pub timing: Option<Timing>,
}

/// One node, or several.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Targets {
    One(Id),
    Many(IdList),
}

/// Whether choreography runs during the state's transition or once it rests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Timing {
    With,
    After,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChoreoSequence {
    pub sequence: Vec<ChoreoItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay: Option<NonNegative>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timing: Option<Timing>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChoreoParallel {
    pub parallel: Vec<ChoreoItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay: Option<NonNegative>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timing: Option<Timing>,
}

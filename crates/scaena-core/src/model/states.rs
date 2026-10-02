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

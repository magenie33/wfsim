use super::*;

/// One Focus node that reaches the Warframe, at max rank.
#[derive(Debug, Clone)]
pub struct FocusNode {
    pub id: String,
    pub name: String,
    pub text: String,
    /// No Operator action is needed; the node always applies.
    pub always: bool,
    pub when: String,
    pub effects: Vec<FrameEffect>,
    pub tags: Vec<TagGrant>,
    /// **THE OPERATOR ACTION THAT EARNS IT, and for how long** — set only on a
    /// node the fight can SIMULATE. An action list naming the action earns it
    /// (`data::casting`); otherwise it is assumed or not, as the Operator build
    /// says.
    pub trigger: Option<(NodeTrigger, f64)>,
}

/// An Operator action a Focus node is conditioned on (`data::apl::Action`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeTrigger {
    /// `Action::OperatorSling`.
    OperatorSling,
}

/// **A WAYBOUND NODE — UNBOUND FROM ITS SCHOOL, AND PERMANENT.**
///
/// *"Unlike all other Ways, these can be 'unbound' from the Focus school they
/// are part of, therefore showing up (and treated as) as an additional unlocked
/// Way in any selected school afterwards"* (W`Focus`). Two per school, ten in
/// all, and unlocking one cannot be undone — which is why they are shown at max
/// rank and are not a choice anyone makes in a build.
///
/// **IT CANNOT CARRY AN EFFECT, and that is the point.** All ten are
/// `waybound=y|passive=y|warframe=|operator=y` on the wiki — the empty
/// `warframe=` flag says the node does not reach the Warframe, so none of them
/// pays a weapon anything. A struct that COULD state an effect and did not
/// would read as a gap somebody has yet to fill; one that cannot says there is
/// nothing to fill.
#[derive(Debug, Clone, Deserialize)]
pub struct WayboundNode {
    pub id: String,
    pub name: String,
    pub text: String,
}

/// A Focus school. Only the ACTIVE school's nodes apply: "Active and Passive ways
/// are only usable in the specific focus school they belong to" (W`Focus`).
#[derive(Debug, Clone)]
pub struct FocusSchool {
    pub id: String,
    pub name: String,
    pub nodes: Vec<FocusNode>,
    /// The school's two Waybounds. Every school's apply whichever is active, so
    /// the panel shows all ten and lets nobody edit them.
    pub waybound: Vec<WayboundNode>,
    /// The school's Tektolyst Artifact: one per school, seated by the Operator.
    pub artifact: Option<ArtifactDef>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ArtifactDef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct RawNode {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) text: String,
    #[serde(default)]
    pub(super) always: bool,
    #[serde(default)]
    pub(super) when: String,
    #[serde(default)]
    pub(super) effects: Vec<RawEffect>,
    #[serde(default)]
    pub(super) tags: Vec<RawTag>,
    #[serde(default)]
    pub(super) trigger: Option<NodeTrigger>,
    #[serde(default)]
    pub(super) duration_seconds: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RawSchool {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) nodes: Vec<RawNode>,
    pub(super) waybound: Vec<WayboundNode>,
    #[serde(default)]
    pub(super) artifact: Option<ArtifactDef>,
    #[serde(default)]
    pub(super) source: SourceFile,
}

pub fn focus_schools() -> &'static [FocusSchool] {
    static F: OnceLock<Vec<FocusSchool>> = OnceLock::new();
    F.get_or_init(|| {
        crate::data::files_under("focus/")
            .map(|(p, text)| {
                let r: RawSchool = serde_norway::from_str(text).unwrap_or_else(|e| panic!("{p}: {e}"));
                FocusSchool {
                    id: r.id,
                    name: r.name,
                    nodes: r
                        .nodes
                        .iter()
                        .map(|n| {
                            assert!(n.always || !n.when.is_empty(), "{p}: {} says neither `always` nor `when`", n.id);
                            assert!(
                                n.trigger.is_some() == n.duration_seconds.is_some() && !(n.always && n.trigger.is_some()),
                                "{p}: {} — a `trigger` needs a `duration_seconds` and a conditional node",
                                n.id
                            );
                            FocusNode {
                                id: n.id.clone(),
                                name: n.name.clone(),
                                text: n.text.clone(),
                                always: n.always,
                                when: n.when.clone(),
                                effects: n
                                    .effects
                                    .iter()
                                    .map(|e| effect(p, e))
                                    .inspect(|e| assert!(!matches!(e, FrameEffect::Arcane(_)), "{p}: an arcane's rule on a node"))
                                    .collect(),
                                tags: tags_of(p, &n.tags),
                                trigger: n.trigger.zip(n.duration_seconds),
                            }
                        })
                        .collect(),
                    waybound: {
                        assert_eq!(r.waybound.len(), 2, "{p}: a school has exactly two Waybounds");
                        r.waybound
                    },
                    artifact: r.artifact,
                    url: r.source.url,
                }
            })
            .collect()
    })
}

pub fn focus_school(id: &str) -> Option<&'static FocusSchool> {
    focus_schools().iter().find(|s| s.id == id)
}

// ---- the Tektolyst Artifact ------------------------------------------------

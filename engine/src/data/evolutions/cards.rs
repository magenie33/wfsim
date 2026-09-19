use super::*;

/// One configurable buff an evolution grants — everything the Sim's and
/// the Optimizer's buff cards need, with no caller-side knowledge of which
/// effect produced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvoBuffCard {
    /// The `apply_buff_config` key this card writes.
    pub id: &'static str,
    pub max_stacks: u32,
    /// PERMANENT stacks (no in-sim trigger, no decay): the count is a
    /// static choice for the run, so the card defaults locked.
    pub permanent: bool,
    /// WHERE THE CARD OPENS, and there are only two rules: a permanent buff
    /// starts full, an earned one starts at zero.
    ///
    /// A third briefly existed — "one reload's worth", for Mounting Momentum —
    /// and it was wrong twice over. Nothing a player sets should depend on the
    /// weapon's stats when the ceiling is the same 99 for every weapon, and it contradicted the sim, which opens that buff at zero
    /// because an empty magazine takes the pile. A card that defaults to six
    /// while the fight starts at none is the plainest kind of lie a panel can
    /// tell.
    pub opens_at: CardOpens,
}

/// See [`EvoBuffCard::opens_at`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardOpens {
    /// Earned in the run.
    Zero,
    /// Nothing decays it, so a lull does not cost it.
    Full,
}

impl EvolutionDef {
    /// EVERY configurable buff this evolution grants.
    ///
    /// The match below is EXHAUSTIVE on purpose: adding an `EvoEffect`
    /// variant fails to compile until someone states whether it is a buff
    /// the user can configure. That is the whole point — a buff that
    /// exists in the engine but not on the cards is invisible, and the
    /// only way to keep the two in step is to make forgetting impossible.
    /// `permanent` is the ONE thing this has to get right: a permanent buff
    /// has no trigger and no decay, so it survives a lull and starts full,
    /// while every timed buff starts EARNED at zero (docs/BUFFS.md).
    pub fn buff_cards(&self) -> Vec<EvoBuffCard> {
        self.active_effects()
            .filter_map(|e| match e {
                // THE MELEE FIVE. Unconditional stat changes — no trigger, no
                // stacks, nothing to configure — so no card, the same answer
                // `FlatBaseDamage` and its siblings get one arm down.
                // A MELEE INCARNON IS A BUFF AND GETS A BUFF'S CARD. It is
                // entered by a heavy attack rather than by an animation, it is
                // on for a stated number of seconds, and it changes numbers on
                // swings the weapon already has — so it belongs where the
                // reader looks for a window, and the two knobs mean what they
                // mean everywhere else (docs/BUFFS.md): STACKS is "you walked
                // in with it up", NO TIMEOUT is the window never closing.
                //
                // ONLY THE TIER THAT STATES THE WINDOW draws it. A card that
                // only lowers the arming — the Praedos's Swift Transmute —
                // states no seconds and is not a second buff.
                EvoEffect::IncarnonWindow { seconds: Some(_), .. } => Some(EvoBuffCard {
                    id: "melee_incarnon",
                    max_stacks: 1,
                    // EARNED, like every other timed buff: the fight opens
                    // un-armed and buys the window with a heavy attack.
                    permanent: false,
                    opens_at: CardOpens::Zero,
                }),
                EvoEffect::BaseDamageBonus(_)
                | EvoEffect::InitialCombo(_)
                | EvoEffect::ComboCountOnSlamHit(_)
                | EvoEffect::IncarnonWindow { .. }
                | EvoEffect::MeleeRange(_)
                | EvoEffect::FollowThroughBonus(_)
                | EvoEffect::SlamRadiusBonus(_)
                | EvoEffect::HeavyWindUpSpeed(_)
                | EvoEffect::ProcConversion { .. } => None,
                // No card: nothing to configure about a payout that cannot
                // happen. The disclosure line is what this perk gets.
                EvoEffect::OutOfScope { .. } => None,
                // PAYS NOTHING, which is the whole point — the sim reproduces
                // the game, and the game pays nothing here.
                EvoEffect::LiveBug { .. } => None,
                EvoEffect::AssumedMaxMultishot { max_stacks, .. } => Some(EvoBuffCard {
                    id: "evo_multishot",
                    max_stacks: *max_stacks,
                    permanent: true,
                    opens_at: CardOpens::Full,
                }),
                EvoEffect::StackingDamageOnPlainHit { max_stacks, .. } => Some(EvoBuffCard {
                    id: "on_plain_hit_damage",
                    max_stacks: *max_stacks,
                    permanent: false,
                    opens_at: CardOpens::Zero,
                }),
                // A BUFF, not a silent stat: the run holds it from t = 0, but
                // it is earned by an empty reload and the bar has to say so. Permanent — nothing decays it — and one
                // stack, which is what "on/off" is in this vocabulary.
                EvoEffect::FlatBaseDamageOnEmptyReload(_) => Some(EvoBuffCard {
                    id: "evo_reload_damage",
                    max_stacks: 1,
                    permanent: true,
                    opens_at: CardOpens::Full,
                }),
                EvoEffect::StackingReloadSpeedOnHeadshot { max_stacks, .. } => Some(EvoBuffCard {
                    id: "on_headshot_reload_speed",
                    max_stacks: *max_stacks,
                    permanent: false,
                    opens_at: CardOpens::Zero,
                }),
                // OPENS FULL WHERE THE CARD SAYS A MISSION NEVER TAKES IT
                // — a buff you are already carrying rather
                // than one you keep up. NOT `permanent`, which means something
                // narrower here: no trigger the sim can fire, so the count is a
                // static choice. This one is earned by an event the fight
                // performs several times over, so its lock stays live and a
                // card set to zero builds back.
                EvoEffect::StackingGrant {
                    trigger, grant, max_stacks, card_opens_full, ..
                } => Some(EvoBuffCard {
                    id: stacking_card_id(*trigger, *grant),
                    max_stacks: *max_stacks,
                    permanent: false,
                    opens_at: if *card_opens_full { CardOpens::Full } else { CardOpens::Zero },
                }),
                EvoEffect::StackingMultishotOnFiring { max_stacks, .. } => Some(EvoBuffCard {
                    id: "on_firing_multishot",
                    max_stacks: *max_stacks,
                    permanent: false,
                    opens_at: CardOpens::Zero,
                }),
                EvoEffect::StackingMultishotOnStatus { max_stacks, .. } => Some(EvoBuffCard {
                    id: "on_status_multishot",
                    max_stacks: *max_stacks,
                    permanent: false,
                    opens_at: CardOpens::Zero,
                }),
                EvoEffect::StackingFireRatePerShellReloaded { max_stacks, .. } => {
                    Some(EvoBuffCard {
                        id: "per_shell_fire_rate",
                        max_stacks: *max_stacks,
                        permanent: false,
                        opens_at: CardOpens::Zero,
                    })
                }
                EvoEffect::StackingFireRateOnHeadshot { max_stacks, .. } => Some(EvoBuffCard {
                    id: "on_headshot_fire_rate",
                    max_stacks: *max_stacks,
                    permanent: false,
                    opens_at: CardOpens::Zero,
                }),
                // READY RETALIATION IS NOT A CARD, and the distinction is the
                // one this list exists for. A card is a CONTROL: it configures
                // a buff the sim cannot earn on its own (Fevered Frenzy's
                // permanent stacks) or locks one it can. This buff is earned by
                // an event the sim already simulates — a reload from empty —
                // and there is nothing for a player to set, so a card here
                // would be a control that does nothing.
                //
                // It is still VISIBLE, which is what BUFFS.md actually requires:
                // `buff_roster` lists it and the replay draws its window, the
                // same way Pressurized Magazine's `on_reload_fr` is drawn
                // without an evolution card.
                // LINGERING JUDGEMENT earns its window from a headshot STREAK,
                // so the card starts at zero like every other earned buff.
                EvoEffect::HeadshotDamageOnStreak { .. } => Some(EvoBuffCard {
                    id: "evo_headshot_streak",
                    max_stacks: 1,
                    permanent: false,
                    opens_at: CardOpens::Zero,
                }),
                EvoEffect::ReloadSpeedOnEmptyReload { .. }
                // Nor is Executioner's Fortune: it is a roll on an event the
                // sim already has, and its whole effect is a magazine counter.
                | EvoEffect::InstantReloadOnHeadshot { .. }
                // Nor is Spiteful Defilement: its condition is the TARGET's,
                // read live, with nothing for a player to set.
                | EvoEffect::CritDamageBelowStatusCount { .. }
                // Static stat changes — nothing to configure at runtime.
                | EvoEffect::FlatBaseStatusChanceByForm { .. }
                | EvoEffect::FlatBaseCritMultiplier(_)

                | EvoEffect::Indirect(..)
                | EvoEffect::AmmoMaxSet(_)
                | EvoEffect::FlatBaseDamage(_)
                | EvoEffect::FlatBaseCritChance(_)
                | EvoEffect::FlatBaseMultishot(_)
                | EvoEffect::FlatBaseStatusChance(_)
                | EvoEffect::FlatBaseMagazine(_)
                | EvoEffect::FieldDurationOnEmptyReload(_)
                | EvoEffect::MultishotBeyondRange { .. }
                | EvoEffect::MultishotOnLastRound { .. }
                | EvoEffect::BaseDamagePerFullBurst { .. }
                | EvoEffect::ArmorStripPerPunctureStatus(_)
                | EvoEffect::MultishotConsumesAmmo(_)
                | EvoEffect::ConditionOverload { .. }
                | EvoEffect::FireRateBonus { .. }
                | EvoEffect::BaseDamageBelowHalfHealth { .. }
                | EvoEffect::GatedByTenno { .. }
                | EvoEffect::DerivedStat { .. }
                | EvoEffect::CritChanceByBodyPart { .. }
                | EvoEffect::RoundRestoreOnStatusHit { .. }
                | EvoEffect::InstantReloadOnKill { .. }
                | EvoEffect::MagGrowthOnEmptyReload { .. }
                | EvoEffect::CritOnUndamaged { .. }
                | EvoEffect::ReloadSpeedBonus(_)
                | EvoEffect::CritMultiplierBelowCritChance { .. }
                | EvoEffect::PostModCritChance(_)
                | EvoEffect::PostModStatusChance(_)
                | EvoEffect::HeadshotDamage(_)
                | EvoEffect::IncarnonChargeRate(_) => None,
                // Rolled per instance, not a buff with an uptime.
                EvoEffect::ChanceDamageOnNoncrit { .. } => None,
                // The transformation grants no CARD: what it unlocks is a
                // FORM, whose own weapon entry carries every stat it brings.
                EvoEffect::UnlocksForm(_)
                | EvoEffect::Inert(_)
                | EvoEffect::Qualifier(_) => None,
            })
            .collect()
    }
}

/// THE BUFF CARD'S ID, derived from what the buff IS rather than carried in the
/// yaml. It is a durable name — the roster, the saved config and the sampler all
/// key on it — so it is a finite reviewable table and not a formatted string.
pub(super) fn stacking_card_id(
    trigger: crate::model::BuffTrigger,
    grant: crate::model::BuffGrant,
) -> &'static str {
    use crate::model::BuffGrant as G;
    use crate::model::BuffTrigger as T;
    match (trigger, grant) {
        (T::Firing, G::FireRate) => "on_firing_fire_rate",
        (T::Firing, G::BaseDamage) => "on_firing_damage",
        (T::StatusApplied, G::FireRate) => "on_status_fire_rate",
        (T::StatusApplied, G::BaseDamage) => "on_status_damage",
        (T::Headshot, G::FireRate) => "on_headshot_fire_rate",
        (T::Headshot, G::BaseDamage) => "on_headshot_damage",
        (T::ConsecutiveHeadshot, G::FlatBaseDamage) => "on_weakpoint_streak_damage",
        (T::ConsecutiveHeadshot, G::HeadshotDamage) => "on_weakpoint_streak_headshot_damage",
        (T::Hit, G::FlatBaseDamage) => "on_hit_damage",
        (T::PlainHit, G::BaseDamage) => "on_plain_hit_damage",
        (T::ReloadComplete, G::BaseDamage) => "on_reload_damage",
        (T::ReloadComplete, G::FireRate) => "on_reload_fire_rate",
        (T::Kill, G::FlatBaseDamage) => "on_kill_damage",
        (T::ReloadFromEmpty, G::FlatBaseDamage) => "on_empty_reload_damage",
        (T::ReloadFromEmpty, G::BaseCritDamage) => "on_empty_reload_crit_damage",
        (T::PunchThrough, G::CritChance) => "on_punch_through_crit_chance",
        (T::PunchThrough, G::FireRate) => "on_punch_through_fire_rate",
        // A pair nobody has written a card for yet. It is still a real buff and
        // still runs; it just shares one generic id, which is visible the first
        // time two of them appear on one weapon and is the point at which the
        // pair earns a name above.
        _ => "stacking_grant",
    }
}

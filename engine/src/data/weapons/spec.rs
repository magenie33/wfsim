use super::*;

/// A perk definition — either a `data/perks/*.yaml` entry or an inline
/// block in a weapon's `perks:` list. Both modes register the perk in the
/// GLOBAL namespace: define once anywhere, reference by bare id from
/// everywhere else (data/README.md).
#[derive(Debug, Clone, Deserialize)]
pub struct PerkSpec {
    pub id: String,
    #[serde(default)]
    pub grants: Option<GrantsSpec>,
}

/// One entry of a weapon's `perks:` list: a bare id string (a reference —
/// resolved against the table AND every inline definition) or a full
/// inline definition.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum PerkRef {
    Id(String),
    Inline(PerkSpec),
}

impl PerkRef {
    /// Resolve to the perk definition (global-namespace lookup for ids).
    pub fn resolve(&self) -> &PerkSpec {
        match self {
            PerkRef::Inline(p) => p,
            PerkRef::Id(id) => {
                perk(id).unwrap_or_else(|| panic!("missing perk yaml: {id}"))
            }
        }
    }

    pub fn id(&self) -> &str {
        match self {
            PerkRef::Id(id) => id,
            PerkRef::Inline(p) => &p.id,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct GrantsSpec {
    #[serde(default)]
    pub injected_element: Option<InjectedElementSpec>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InjectedElementSpec {
    #[serde(rename = "type")]
    pub element: String,
    pub amount: f64,
}

/// One registered form of a weapon: which yaml ENTRY provides it, what kind it
/// is, and whether it is the one the weapon is normally fired in.
#[derive(Debug, Clone, Copy)]
pub struct FormRef {
    /// The weapon entry id backing this form — the key `base_panel` takes.
    pub weapon_id: &'static str,
    pub kind: FormKind,
    /// The arsenal's form: the group's roster entry (the wiki module says it
    /// per weapon with `_TooltipAttackDisplay`).
    pub is_default: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WeaponSpec {
    /// A FORM INHERITS ITS WEAPON. The id of the entry this one is a form of —
    /// every WEAPON-LEVEL field this entry does not state is filled in from
    /// there, and only what actually DIFFERS is written down here.
    ///
    /// WHY IT EXISTS. 88 of the roster's entries are form siblings rather than
    /// weapons. Restating each weapon's mastery rank, disposition, polarities,
    /// riven family, internal name, magazine and reload on every sibling is 313
    /// identical values written twice and 1,004 (group, field) pairs where some
    /// siblings carry a field and others do not — and a real error hides in
    /// that noise, as when one variant's alt-fire carries its BASE form's
    /// accuracy and another's carries the alt-fire's. Nothing can catch it
    /// unless something knows the two entries are the same weapon.
    ///
    /// With this, a difference is the only thing on the page.
    ///
    /// Applies to [`INHERITED`] and to nothing else: the ATTACK is never
    /// inherited (it is the entire reason a form is a separate entry), and
    /// neither is `co_behavior`, which the catalog gives PER ATTACK — the
    /// Mandonel's two forms take different classes from two different rows.
    #[serde(default)]
    pub inherits: Option<String>,
    /// EVERY ADMISSION, STRUCTURED — filled in at load time beside the rendered
    /// `unmodeled:` strings, so a localized page can re-render one instead of
    /// looking up the whole English sentence.
    ///
    /// A weapon never writes this; it writes `unmodeled:` and this is derived.
    #[serde(default)]
    pub unmodeled_parts: Vec<UnmodelledPart>,
    pub id: String,
    pub name: String,
    /// WHAT THE GAME CALLS THIS FORM — its page's own name for the attack
    /// block, verbatim. [`FormKind::label`] is the fallback and it is OUR
    /// word: nothing in the game is a "Base Form". docs/FORM_NAMES.md.
    #[serde(default)]
    pub form_name: Option<String>,
    /// THIS ENTRY CANNOT AIM DOWN SIGHTS, so nothing gated on aiming pays.
    ///
    /// On the wiki "Zoom" IS the word for aiming — its page opens "Zoom (or
    /// aiming, aiming down sights (ADS))", and the Galvanized mods link the
    /// word as `[[Zoom|aiming]]`. So the Vasto's "cannot Zoom" is a statement
    /// about the aim STATE, not about magnification.
    ///
    /// DE settled what that costs, in a patch note about Mesa's Regulators:
    /// "Removed ability to unintentionally equip Hydraulic Crosshairs and
    /// Sharpened Bullets on Mesa's Regulators. Although the buff appeared to
    /// trigger, it never actually applied due to the 'on aim' criteria not
    /// being fulfilled."
    ///
    /// PER FORM, not per weapon: the Vasto aims fine and its Incarnon form does
    /// not, and each form is its own entry.
    #[serde(default)]
    pub cannot_zoom: bool,
    /// SECONDS THE MELEE COMBO COUNTER SURVIVES with nothing added to it.
    ///
    /// Five on almost every melee weapon, and stated per weapon because the
    /// wiki names the exceptions itself — Guandao Prime 6, Pulmonars 9,
    /// Vitrica 10, Xoris infinite. A gun leaves it at 0, which is never read
    /// there: nothing outside a melee form asks the combo counter anything.
    #[serde(default)]
    pub combo_duration_seconds: f64,
    /// WHAT THIS ENTRY DOES NOT MODEL, in the reader's own language, one
    /// sentence per gap.
    ///
    /// The enemy files have carried this since the target card was written, and
    /// weapons had nowhere to put it: a yaml COMMENT is honest to whoever opens
    /// the file and invisible to everyone else. The bulk Incarnon intake made
    /// that expensive — a weapon whose base attack has parts this entry does not
    /// carry (a bow's uncharged shot, the Angstrum's explosion, the Stug's
    /// blobs) reads as a complete weapon, and its number is not the weapon's
    /// number.
    ///
    /// Prose, deliberately, and the ONE place in a weapon file where prose is a
    /// value rather than a comment — the same exception `enemies/` already
    /// carries, for the same reason: it is shown to a reader verbatim.
    #[serde(default)]
    pub unmodeled: Vec<String>,
    /// WHAT THIS ENTRY DOES THAT NOBODY CAN EXPLAIN, and the engine reproduces
    /// anyway — the weapon's half of the `live_bugs:` an arcane, an ability and
    /// an enemy already carry.
    ///
    /// IT IS NOT AN `unmodeled:` LINE and must not be filed as one: that banner
    /// says "the number below is a FLOOR", and this says the opposite — the
    /// number is right, it was measured, and the reason is unknown. A player
    /// building around it is owed both facts.
    ///
    /// The case it was added for is the Laetum's Incarnon form, whose Secondary
    /// Irradiate echo measures 3.6x the hit where the arcane's own card says
    /// 1.8x and every pure single-target weapon delivers 1.8x (MEASUREMENTS
    /// M59). The engine carries it as `echo_multiplier` and said so nowhere a
    /// reader could see, while the arcane card beside it printed 180%.
    ///
    /// Prose, for the same reason `unmodeled:` is: it is shown verbatim.
    #[serde(default)]
    pub live_bugs: Vec<String>,
    /// DE's ACCURACY stat, as the Arsenal prints it. REFERENCE ONLY — the
    /// model reads [`AttackSpec::spread`], which is the primary value.
    ///
    /// The yaml has carried this on 144 entries since the intake and NOTHING
    /// deserialized it, so serde dropped every one of them: a number in the
    /// repo that no code could see.
    ///
    /// It is DERIVED and it is fuzzy: the wiki defines it
    /// as `100 / (average spread in degrees)` and prints it as a CATEGORY
    /// ("Very High"), so it is one rounded scalar standing in for a window —
    /// and it is a WEAPON-level field, which cannot describe a form at all.
    /// The engine therefore does not read it; `AttackSpec::spread` is what the
    /// aim model uses and it comes per attack from the same wiki module.
    #[serde(default)]
    pub accuracy: Option<f64>,
    pub slot: String,
    pub class: String,
    /// Which DEPLOYMENT the fields on this entry describe (Arch-Guns:
    /// "atmosphere"). `None` = the weapon has only one.
    #[serde(default)]
    pub deployment: Option<String>,
    /// The OTHER deployments, by name, each stating only what it overrides.
    /// An Arch-Gun is the same weapon on the ground and in Archwing — same
    /// damage, same mods, same riven — and only its sustain differs, so this
    /// is a SCENARIO axis rather than a second weapon.
    #[serde(default)]
    pub deployments: BTreeMap<String, DeploymentSpec>,
    /// An ADVERSARY weapon's valence bonus — what it CAN have. See
    /// [`ValenceSpec`]; absent on every weapon that is not one.
    #[serde(default)]
    pub valence: Option<ValenceSpec>,
    /// Does this weapon apply MICROWAVE — the Nukor family's own invisible
    /// status? See `fight::DebuffState::microwave`. Two weapons in the game
    /// have it and the wiki names both.
    #[serde(default)]
    pub applies_microwave: bool,
    /// INDEPENDENT PROCS this attack lands — status effects that come from a
    /// specific weapon rather than from the damage-type draw
    /// (`data/debuffs/independent_procs.yaml`).
    ///
    /// A LIST OF IDS, not a flag per effect. `applies_microwave` above is the
    /// older shape and the reason this one is not: an effect that arrives with
    /// the next weapon should cost a row in a table, not a field on every
    /// weapon in the roster. The engine implements the ids
    /// it knows and panics on one it does not, which is the same contract
    /// `charge_on` has.
    ///
    /// The only member so far is `lifted` — the Mausolon's alt-fire explosion.
    /// It carries no damage; what it carries is a status TYPE, and Condition
    /// Overload counts it (`Status_Effect` §Independent from Damage, and
    /// MECHANICS §"Condition Overload").
    #[serde(default)]
    pub independent_procs: Vec<String>,
    /// Does this weapon's INNATE headshot bonus multiply the additive bracket
    /// instead of joining it? A PER-WEAPON anomaly, not a class rule: the wiki
    /// lists innate bonuses (Kuva Chakkhurr) among the ADDITIVE sources and
    /// then singles one out — "Cernos Prime's headshot bonus is unique and
    /// stacks multiplicatively with Primary Deadhead's headshot bonus".
    #[serde(default)]
    pub headshot_bonus_multiplicative: bool,
    /// Which FORM of its weapon this entry is — a kind from the closed
    /// [`FormKind`] vocabulary. REQUIRED: a form is registered, never guessed,
    /// so a new entry cannot quietly inherit someone else's mode.
    pub form: String,
    /// Is this the form the weapon is normally fired in — the arsenal's, which
    /// the wiki module names per weapon with `_TooltipAttackDisplay`? Exactly
    /// one entry per transform group declares it, and that entry is the
    /// weapon's roster row. Declared rather than inferred from
    /// `transforms_from`, because two forms need not be a transformation:
    /// tapping a bow instead of drawing it switches form for free.
    #[serde(default)]
    pub default_form: bool,
    /// The mod POOLS this weapon draws from, as a union — `data/mods/<pool>/`.
    /// A weapon is not served by one list: a launcher takes both the
    /// primary-wide pool and the rifle class pool, and takes no assault-rifle
    /// or bow mods, which is why those are pools of their own.
    #[serde(default)]
    pub mod_pools: Vec<String>,
    /// THE CHAMBER RECORD this entry is assembled from — `tombfinger_secondary`
    /// in `data/kitguns/chambers/`. Its presence is what makes the weapon
    /// MODULAR, and it is the whole of that difference.
    ///
    /// A Kitgun has no published stat line: every number on this entry is its
    /// DEFAULT assembly's, which `every_modular_entry_states_its_default_assembly`
    /// holds. [`spec_assembled`] composes the real ones over the top, exactly
    /// as an evolution overrides a panel — so nothing downstream of
    /// `base_panel` has to learn what a Kitgun is.
    #[serde(default)]
    pub kitgun: Option<String>,
    /// ROUNDS A SECOND under Pax Charge, filled in by [`spec_assembled`] from
    /// the chamber. Not written in any weapon yaml: it is the chamber's, and a
    /// roster entry that restated it would be the same number written twice.
    #[serde(skip)]
    pub recharge_per_second: Option<f64>,
    /// Riven disposition — the multiplier every riven stat on this weapon is
    /// scaled by. It belongs to the WEAPON, not to the riven, which is why
    /// **AN UNEXPLAINED, MEASURED COEFFICIENT ON SECONDARY IRRADIATE'S ECHO.**
    ///
    /// The echo is `1.8 × the hit` at max rank on every pure single-target
    /// weapon measured. On the LAETUM'S INCARNON FORM it deals **3.6×**:
    ///
    /// ```text
    /// base form      1536 direct  ->  2764.8 echo   = 1.80x   (ordinary)
    /// Incarnon form   320 direct  ->  1152   echo   = 3.60x
    ///                 960 direct  ->  3456   echo   = 3.60x
    /// ```
    ///
    /// IT IS NOT THE AoE TRIGGERING IT TWICE: only a DIRECT hit ever triggers
    /// this echo, measured, which is why `spread_from_echo` is called from the
    /// direct path alone. That the Incarnon form's radial is involved is a
    /// LEAD, so this stays a per-ENTRY number rather than a rule about AoE
    /// weapons. NOT INHERITED, because the base Laetum measures 1.8.
    #[serde(default = "one")]
    pub echo_multiplier: f64,
    /// one riven reads differently on two guns.
    #[serde(default)]
    pub disposition: Option<f64>,
    /// Which RIVEN this weapon takes, by the family's English name — one
    /// riven fits every variant in it, which is why it is a name and not an
    /// id. It is the key `data/rivens/pools.yaml` is surveyed by: DE rolls a
    /// pool per family, so a Boar riven and a Boar Prime riven are one thing.
    #[serde(default)]
    pub riven_family: Option<String>,
    /// WHICH RIVEN POOL, where it is not the one the mod pools lead to: a
    /// primary Kitgun takes its chamber's card, which is a pistol riven.
    #[serde(default)]
    pub riven_class: Option<String>,
    /// AN EXALTED WEAPON — one an ability summons (Valkyr Talons, by Hysteria).
    /// It is the weapon's half of DE's `POWER_WEAPON` tag: a card whose
    /// incompatibility tags carry it (`excludes_weapon: [power_weapon]`) is
    /// refused from the pool — Blood Rush, Weeping Wounds, the Amalgams.
    #[serde(default)]
    pub exalted: bool,
    /// A STANCE THE WEAPON CANNOT TAKE OFF, by mod id. Seated on every build,
    /// never removed, and its slot takes no Forma (Valkyr Talons' Hysteria,
    /// MEASUREMENTS M94) — so it is the weapon's fact and a build only repeats it.
    #[serde(default)]
    pub fixed_stance: Option<String>,
    /// THE ONLY WARFRAMES THAT CAN HOLD IT, by id. Empty on almost every weapon,
    /// which any wielder can carry, the Prototype included; an Exalted weapon
    /// names its frame (Valkyr Talons: Valkyr and Valkyr Prime), and a build
    /// linking any other wielder is held by the first of these instead.
    #[serde(default)]
    pub wielders: Vec<String>,
    /// WHAT IT IS CALLED IN ONE WIELDER'S HANDS, by frame id — the arsenal
    /// renames the weapon rather than the stats ("Valkyr Prime Talons").
    #[serde(default)]
    pub wielder_names: BTreeMap<String, String>,
    /// `by_round` — the magazine refills a SHELL AT A TIME (Strun, Felarx,
    /// Onos). It is the wiki module's `ReloadStyle`, and it is not cosmetic:
    /// a bigger magazine makes the reload LONGER, so a magazine mod buys
    /// capacity and pays for it in downtime. Modelled as one flat block until
    /// 2026-08-08, which made Ammo Stock read as free capacity on exactly the
    /// weapons the game charges for it (calibrating the Felarx).
    #[serde(default)]
    pub reload_style: Option<String>,
    /// The three parts of a by-round reload, where the weapon's page states
    /// them — the Felarx's are 0.8 s to start, 0.4 s a shell, 0.5 s to end.
    ///
    /// Where they are NOT stated, the engine derives a per-shell time from the
    /// published total and the base magazine and leaves the fixed parts at
    /// zero. That reproduces the published number exactly at a full magazine
    /// and scales correctly with capacity, which is the behaviour that was
    /// missing; it only understates a PARTIAL reload, and this sim reloads
    /// from empty.
    #[serde(default)]
    pub reload_start_seconds: Option<f64>,
    #[serde(default)]
    pub reload_per_shell_seconds: Option<f64>,
    #[serde(default)]
    pub reload_end_seconds: Option<f64>,
    /// The weapon's own rank ceiling — 30 for almost everything, 40 for the
    /// Kuva/Tenet/Coda families and the Paracesis. It decides CAPACITY, since
    /// capacity "correlates to their Rank" (wiki `Mod Capacity`) and a rank-40
    /// weapon climbs two ranks per Forma to reach it.
    ///
    /// The data has carried it since the roster was written and nothing read
    /// it: capacity was the literal 60 in four places instead.
    #[serde(default = "rank_30")]
    pub max_rank: u32,
    #[serde(default)]
    pub polarities: Vec<String>,
    #[serde(default)]
    pub exilus_polarity: Option<String>,
    /// THE STANCE SLOT'S OWN POLARITY, which decides what a stance GRANTS: a
    /// stance is an Aura, not a cost — *"All Stances provide a bonus mod
    /// capacity of 5 when maxed, doubling it to 10 when placed on the matching
    /// polarity"* (wiki, Stance). See [`crate::rules::capacity::stance_capacity`].
    #[serde(default)]
    pub stance_polarity: Option<String>,
    #[serde(default)]
    pub magazine: Option<f64>,
    /// Reserve rounds outside the magazine — the wiki's "Ammo Max".
    ///
    /// Present on nearly every weapon and, until now, read by nobody: the sim
    /// treats reserves as INFINITE by default because it
    /// does not model ammo PICKUPS, and a weapon that can be resupplied mid
    /// fight would otherwise run dry for a reason the game does not have.
    #[serde(default)]
    pub ammo_max: Option<f64>,
    /// ROUNDS ONE AMMO PICKUP GIVES THIS WEAPON — the wiki's "Ammo Pickup",
    /// which is a per-WEAPON stat and not a per-class constant: *"Area of
    /// Effect Weapons tend to have lower base Ammo Pickup than normal"*, and
    /// DE publishes a per-weapon override list. `None` on a weapon with no
    /// reserve to fill. Read by `engine::ammo` when a kill drops one.
    #[serde(default)]
    pub ammo_pickup: Option<f64>,
    /// Can this weapon NOT be refilled mid-fight? A ground Arch-Gun is the
    /// case this exists for: "Archguns only have a limited amount of ammo",
    /// and when it is gone the weapon is removed for a five-minute cooldown
    /// (wiki Arch-Gun). Everything else is resupplied from ammo pickups, which
    /// is what the Infinite-ammo default stands in for.
    ///
    /// THIS IS NOT "has a reserve" — that one is DERIVED from `ammo_max`, and
    /// the two were one flag until 2026-08-04. Conflating them meant the
    /// Infinite-ammo box was ticked AND DISABLED on every weapon but one,
    /// because "cannot be resupplied" was being read as "has no reserve at
    /// all". A Torid has 60 rounds behind its magazine;
    /// what it also has is a way to get more.
    #[serde(default)]
    pub no_resupply: bool,
    /// A status-triggered crit-chance LOCK — Gotva Prime's passive, and the
    /// first of its kind in the roster.
    #[serde(default)]
    pub super_crit_on_status: Option<SuperCritSpec>,
    /// WEAK-POINT STACKS — the Knell family's "Death Knell".
    #[serde(default)]
    pub weakpoint_stacks: Option<WeakpointStacksSpec>,
    /// WHAT A KILL BY THIS ATTACK LEAVES STANDING — the Ballistica Prime's
    /// ghosts. COUNTED AND NOTHING ELSE: see notes, and the entry's own
    /// `unmodeled:` for what a ghost does that this does not.
    #[serde(default)]
    pub spawn_on_kill: Option<SpawnOnKillSpec>,
    /// The Ocucor's tendrils — see [`TendrilSpec`].
    #[serde(default)]
    pub tendrils: Option<TendrilSpec>,
    /// Pyrana Prime's second gun — see [`KillStreakSummonSpec`].
    #[serde(default)]
    pub kill_streak_summon: Option<KillStreakSummonSpec>,
    /// The sniper's Shot Combo Counter — see [`SniperCombo`]. `None` on every
    /// weapon that is not a sniper rifle, which is what the mechanic is keyed
    /// on in game: it is not a class-wide rule the engine could infer from
    /// `class: sniper`, because the Minimum Combo is per weapon.
    #[serde(default)]
    pub sniper_combo: Option<SniperCombo>,
    /// ...and its scope's own buff — see [`ScopeSpec`].
    #[serde(default)]
    pub scope: Option<ScopeSpec>,
    /// Where this CONTINUOUS weapon's damage ramp starts, as a fraction of
    /// full damage. Omitted means the wiki's "for most weapons" 20%; state it
    /// only for a weapon whose page gives a different number.
    #[serde(default)]
    pub beam_ramp_floor: Option<f64>,
    #[serde(default)]
    pub reload_seconds: Option<f64>,
    /// AN INNATE RELOAD-SPEED TERM on a reload from empty, in the bucket the
    /// mods are in (the Catabolyst family's -20%). Every reload this arena
    /// performs is from empty, so it applies to every one.
    #[serde(default)]
    pub reload_from_empty_speed: Option<f64>,
    /// A MAGAZINE THAT REFILLS ITSELF — see [`Battery`]. `None` on every weapon
    /// that reloads.
    #[serde(default)]
    pub battery: Option<Battery>,
    #[serde(default)]
    pub co_behavior: Option<String>,
    /// How much of the base the CO term computes on (the catalog's "CO Damage
    /// Bonus Relative To Base Damage" column) when the WEAPON, not one of its
    /// evolutions, is what narrows it. A bow's charged shot: 0.5 — "CO-bonus
    /// only applies to base (uncharged) damage; bows have innate 2x damage
    /// multiplier when fully charged" (CO catalog, Cernos Prime row).
    /// Unset = 1.0, the normal case.
    #[serde(default)]
    pub co_base_fraction: Option<f64>,
    /// Innate additive headshot-damage bonus — the module's per-attack
    /// `ExtraHeadshotDmg` (Cernos Prime: 0.5). Joins the same additive
    /// headshot bracket the arcane and evolution bonuses use.
    #[serde(default)]
    pub headshot_damage_bonus: Option<f64>,
    /// THE WEAPON'S OWN HEADSHOT MULTIPLIER, which REPLACES the body part's.
    ///
    /// A head is worth what the ENEMY's body part says, on every weapon but a
    /// handful: the Tenet Arca Plasmor states *"1x headshot multiplier"* and
    /// then *"Although it has a 1x headshot multiplier (meaning it does no
    /// extra damage), this can be increased using Primary Deadhead."* So the
    /// head is still a HEAD and only the enemy's own multiplier is overruled.
    ///
    /// NOT `headshot_damage_bonus`, the additive bracket beside Deadhead's,
    /// which is where the module states this weapon's value:
    /// `ExtraHeadshotDmg = -2` on seven primaries. Put through that bracket
    /// `1 + (-2)` is NEGATIVE and a headshot would heal the target, so the
    /// datamined figure encodes a multiplier applied somewhere this engine does
    /// not and the wiki's "1x" is what gets transcribed. The two fields
    /// compose.
    ///
    /// It also silences the CRITICAL HEADSHOT doubling, which the engine gates
    /// on a part multiplier above 1x — correctly, since the wiki's rule is about
    /// a weak point worth more than 1x and this weapon's head is not one.
    #[serde(default)]
    pub headshot_multiplier: Option<f64>,
    #[serde(default)]
    pub transform_group: Option<String>,
    #[serde(default)]
    pub transforms_from: Option<String>,
    #[serde(default)]
    pub transforms_to: Option<String>,
    pub attack: AttackSpec,
    #[serde(default)]
    pub gauge_form: Option<GaugeFormSpec>,
    #[serde(default)]
    pub pseudo_reload: Option<PseudoReloadSpec>,
    /// The weapon's perks: id references into `data/perks/` or inline
    /// one-off definitions (each entry of a transform group lists its own —
    /// Frenzy is active in both Dual Toxocyst forms).
    #[serde(default)]
    pub perks: Vec<PerkRef>,
}

pub(super) fn yes() -> bool {
    true
}

pub(super) fn one() -> f64 {
    1.0
}

/// ONE ADMISSION, as the page needs it.
///
/// `text` is the finished English. `template` and `params` are present when the
/// admission named a REASON, and they are what lets a locale translate the
/// sentence once rather than once per set of numbers.
#[derive(Debug, Clone, Deserialize)]
pub struct UnmodelledPart {
    pub text: String,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub params: BTreeMap<String, String>,
}

/// THE OCUCOR'S TENDRILS: a kill spawns an energy tendril, up to `max`, and
/// any magazine event clears them all.
///
/// WHAT IS DELIBERATELY ABSENT: damage. A tendril reaches for a DIFFERENT
/// enemy, and the wiki is explicit about the one that reaches this fight's
/// target — *"Tendrils homing in on the main beam's target are only cosmetic,
/// and don't deal any additional damage or status effects."* So in a
/// single-target arena a tendril's own damage is zero, and modelling it would
/// inflate the weapon by up to four beams that the source says are not there.
///
/// The COUNT still matters, which is why this type exists at all: Sentient
/// Surge scales crit chance and status chance with how many tendrils are up,
/// and those land on the MAIN beam, which is real damage against this target.
/// The passive therefore reaches the fight through the mod and not through
/// itself.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct TendrilSpec {
    /// The cap. Four on the Ocucor.
    pub max: u32,
    /// How far a tendril reaches, metres. Twenty on the Ocucor, and Ruinous
    /// Extension extends it — a mod this engine has never had anything for.
    #[serde(default = "tendril_range_default")]
    pub range_m: f64,
    /// How far off the reticle a tendril will ACQUIRE a body, degrees. Forty on
    /// the Ocucor; it then holds to sixty, which nothing here needs because
    /// nobody moves.
    #[serde(default = "tendril_cone_default")]
    pub acquire_deg: f64,
}

pub(super) fn tendril_range_default() -> f64 {
    20.0
}
pub(super) fn tendril_cone_default() -> f64 {
    40.0
}

/// THE SCOPE, as the only part of zoom that is a damage number.
///
/// A sniper's zoom levels each carry a buff (wiki `Sniper Rifle` §Zoom Buffs),
/// and the arena models the BUFF while modelling none of the optics: it has no
/// distance and no field of view (docs/UNMODELLED.md), so nothing here is
/// traded for the magnification and the highest level is not a choice — it is
/// strictly better and free. The scope therefore sits at its top level
/// whenever the Tenno is aiming, and that is stated on the weapon's card.
///
/// Only the headshot-damage kind is declared, because it is the only kind the
/// roster's snipers grant. The Lanka's and Komorex's are called out by the same
/// section as exceptions to the additive rule, so a weapon that grants crit
/// chance or a critical multiplier gets its own field when one is added rather
/// than this one reinterpreted.
///
/// *"These zoom buffs, which are intrinsic to the weapon and cannot be
/// modified, generally stack additively with similar buffs from mods"* — so it
/// joins the headshot bracket rather than multiplying it.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct ScopeSpec {
    /// The top zoom level's magnification, carried for the card's sentence.
    ///
    /// `None` where the page publishes none — the Vesper 77's laser sight is an
    /// aim bonus with no stated zoom level, and inventing a 1.0 for it would put
    /// a number on the card that no source ever wrote.
    #[serde(default)]
    pub magnification: Option<f64>,
    /// ...and its headshot-damage bonus at that level, as a fraction. The
    /// Vectis family's kind.
    #[serde(default)]
    pub headshot_damage: f64,
    /// ...or a CRITICAL CHANCE bonus, which is what the Lanka's scope grants
    /// (+20/+30/+50% across its three levels). Relative to the unmodded base,
    /// like every other crit-chance bucket entry.
    #[serde(default)]
    pub crit_chance: f64,
    /// ...or a CRITICAL MULTIPLIER bonus — the Rubico family's kind, and the
    /// Perigale's (+35/+50% and +20/+40%).
    ///
    /// A scope grants exactly ONE of these three in the published table, so the
    /// two it does not grant stay zero. They are separate fields rather than a
    /// kind + value because each lands in a DIFFERENT bucket, and a single
    /// value with a tag would just move the match somewhere less obvious.
    #[serde(default)]
    pub crit_multiplier: f64,
    /// ...or a FLAT critical chance applied AFTER mods, which is the Lanka's
    /// and the reason the mechanic page calls its zoom bonus an exception:
    /// *"The zoom bonus adds a flat +20/30/50 critical chance, applied after
    /// mods"* (wiki `Lanka`). That is a different layer from `crit_chance`
    /// above — a relative bucket term is multiplied by the weapon's base, and
    /// this is added to the finished number — so it needs its own field or the
    /// Lanka's +50% would be worth 50% of 25% instead of 50 points.
    #[serde(default)]
    pub crit_chance_post_mod: f64,
}

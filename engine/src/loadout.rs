//! Pipeline layer [1]: mod resolution — a chosen mod set becomes panel stats.
//!
//! Buckets (docs/MECHANICS.md, docs/GLOSSARY.md): every relative bonus of one
//! kind sums additively into its bucket, then buckets combine by their real
//! rules (crit chance = base × (1 + Σcc); elemental amount = ModifiedBase ×
//! bonus; reload time = base / (1 + Σreload); …). Elemental entries enter the
//! layer-[2] hierarchy in **mod order** ([`crate::elements`]).
//!
//! Conditional/stacking effects resolve under a [`StackPolicy`] — today only
//! `AssumedMax` (docs/OPTIMIZER.md §3: every stacking buff at full stacks).

/// What a build gives up to Primary Compression, before the arcane's own two
/// per-metre ramps are applied to it.
#[derive(Debug, Clone, Copy)]
pub struct Compression {
    /// Metres of blast radius surrendered while aiming — the MODDED radius
    /// times this weapon's row, times the four fifths the arcane takes.
    pub radius_lost: f64,
    /// The row's Stacking Behavior: true = the bonus joins the base-damage
    /// bucket, false = it multiplies beside it.
    pub adds: bool,
}

/// What Primary Compression LEAVES of a blast radius while aiming: *"x0.2
/// explosion radius"*. Everything else about the arcane is per-weapon; this
/// fifth is not.
pub const COMPRESSION_RADIUS_KEPT: f64 = 0.2;
use crate::damage::{DamageType, DamageVector};
use crate::elements::{self, ElementalInput};
use crate::mods::Polarity;

/// Combat faction — the key for faction-damage mods (Bane/Expel/Cleanse/Smite,
/// "System A"). Distinct from [`crate::enemy_data::ScalingFaction`] (stat
/// scaling) and from the per-type vulnerability column ("System B"). `Unknown`
/// = no faction mod ever applies (e.g. Zariman Thrax, faction "Unknown").
/// Strict matching: Grineer mods do NOT hit Corrupted/Narmer units. The
/// `Corrupted` variant covers the Void/Orokin enemies the "Expel Orokin"
/// family targets. Wiki `Faction_Damage_Bonus`, docs/MECHANICS.md §2/§8.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Faction {
    Grineer,
    Corpus,
    Infested,
    Corrupted,
    Murmur,
    Sentient,
    Unknown,
}

impl Faction {
    /// Map a data string (mod `faction:` field, enemy `combat_faction:`) to a
    /// faction. `orokin` aliases `Corrupted`; unrecognized → `Unknown`.
    pub fn from_name(s: &str) -> Faction {
        match s.trim().to_ascii_lowercase().as_str() {
            "grineer" => Faction::Grineer,
            "corpus" => Faction::Corpus,
            "infested" => Faction::Infested,
            "corrupted" | "orokin" => Faction::Corrupted,
            "murmur" | "the_murmur" | "the murmur" => Faction::Murmur,
            "sentient" => Faction::Sentient,
            _ => Faction::Unknown,
        }
    }
}

/// The stat bucket a conditional buff feeds ([`ModEffect::CondBuff`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CondBucket {
    BaseDamage,
    Multishot,
    CritChance,
    CritDamage,
    StatusChance,
    StatusDamage,
    FireRate,
    /// Archgun Ace's second half. Reload is not a per-hit bucket, so this
    /// contributes only under AssumedMax — under Emergent the sim would have
    /// to hold a live reload-speed timer, which nothing else needs yet.
    ReloadSpeed,
}

impl CondBucket {
    /// Printed on a card, so it is words and not the variant name — a
    /// conditional fire-rate buff used to read "+50% FireRate".
    pub fn label(&self) -> &'static str {
        match self {
            CondBucket::BaseDamage => "Base Damage",
            CondBucket::Multishot => "Multishot",
            CondBucket::CritChance => "Crit Chance",
            CondBucket::CritDamage => "Crit Damage",
            CondBucket::StatusChance => "Status Chance",
            CondBucket::StatusDamage => "Status Damage",
            CondBucket::FireRate => "Fire Rate",
            CondBucket::ReloadSpeed => "Reload Speed",
        }
    }
}

/// A player STATE a mod can be conditional on. One variant per field of
/// [`crate::tenno_data::TennoState`] — the two are meant to be read together.
///
/// `Aiming` is in here rather than beside it: it was a bool threaded through
/// `resolve` while the other states lived on the Tenno, which is two homes for
/// one kind of fact and two places to remember when the third state lands
/// (user, 2026-08-02). A card says "while X"; the fight says who is doing what;
/// one enum joins them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TennoCondition {
    Aiming,
    Invisible,
    Airborne,
}

impl TennoCondition {
    /// Is this condition true of `t`?
    pub fn holds(self, t: &crate::tenno_data::Tenno) -> bool {
        match self {
            TennoCondition::Aiming => t.state.aiming,
            TennoCondition::Invisible => t.state.invisible,
            TennoCondition::Airborne => t.state.airborne,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            TennoCondition::Aiming => "while aiming",
            TennoCondition::Invisible => "while Invisible",
            TennoCondition::Airborne => "while Airborne",
        }
    }
}

/// One resolved effect of a mod at its equipped rank.
///
/// NOT `Copy`: [`ModEffect::WhileTenno`] nests an effect, which needs
/// indirection. Every arm still binds only `Copy` payloads, so `match *e`
/// works unchanged.
#[derive(Debug, Clone, PartialEq)]
pub enum ModEffect {
    /// DOUBLE TAP: `(per stack, max stacks, seconds)` — a damage bonus that
    /// climbs with CONSECUTIVE HITS and stands on its own multiplier.
    ///
    /// "Multiplicatively stacks with damage bonuses like Serration and Faction
    /// Damage Bonus" (wiki), so it is NOT the base-damage bucket: it joins the
    /// chain of independent multipliers beside the faction bonus and Eclipse.
    ///
    /// The count is PER TRIGGER PULL, not per pellet, and the card's own
    /// arithmetic is what pins it: "the bonus is applied on hit to all pellets
    /// as damage * 20% * (hits - 1)", worked through as "with a modded
    /// multishot of 3, the first trigger pull would do +40% bonus damage, the
    /// second +100%, the third +160%". So every pellet of a pull gets the SAME
    /// bonus, computed from the running total INCLUDING that pull, less one —
    /// which is also why an unmodded weapon gets nothing on its first shot.
    /// SYNTH CHARGE: *"bonus damage to the final shot in the Magazine"*.
    ///
    /// ITS OWN MULTIPLIER — "Damage stacks multiplicatively with Hornet Strike,
    /// and any area damage the weapon may have is also affected" — so it is a
    /// factor beside Double Tap's and never a base-damage bucket term.
    ///
    /// THREE THINGS SWITCH IT OFF, all three the mod's own words: it "has no
    /// effect on Continuous Weapons even if they meet the magazine
    /// requirements", it "does not have an effect on any Incarnon fire modes",
    /// and it is only EQUIPPABLE where the weapon's BASE magazine is 6 or
    /// higher. The first two are resolved against the form; the third is an
    /// equip rule, because a magazine mod can neither buy it nor lose it.
    LastRoundDamage(f64),
    ConsecutiveHitDamage { per_stack: f64, max_stacks: u32, duration: f64 },
    /// Additive base-damage bucket (Hornet Strike).
    BaseDamage(f64),
    /// Additive multishot bucket (total pellets = base × (1 + Σ)).
    Multishot(f64),
    /// Relative crit chance (base_cc × (1 + Σ)).
    CritChance(f64),
    /// Relative crit damage (base_cd × (1 + Σ)).
    CritDamage(f64),
    /// Relative status chance.
    StatusChance(f64),
    /// Relative fire rate (negative for Creeping Bullseye's downside).
    FireRate(f64),
    /// Relative CHARGE rate — shortens the draw ONLY (Shell Rush "+50% Charge
    /// Rate"). Its own bucket rather than `FireRate` because a charge-rate mod
    /// must not also speed up an uncharged form: on the Larkspur Prime that
    /// would hand the hit-scan attack a bonus the card never grants.
    ChargeRate(f64),
    /// Reload speed bonus (time = base / (1 + Σ)).
    ReloadSpeed(f64),
    /// Hunter Munitions / Internal Bleeding: chance for a CRITICAL hit to
    /// apply a Slash status, rolled per pellet and INDEPENDENT of status
    /// chance and of the weapon's damage types (wiki: "not affected by the
    /// weapon's Status Chance, or damage type distribution, besides being
    /// indirectly affected by its Critical Chance").
    SlashOnCrit(f64),
    /// Status-damage bucket (Pistol Elementalist) — scales status payloads.
    StatusDamage(f64),
    /// Primary element: ModifiedBase × bonus enters the hierarchy at this
    /// mod's position.
    Element(DamageType, f64),
    /// Combined-element mod (Magnetic Might): added outside the hierarchy.
    CombinedElement(DamageType, f64),
    /// PHYSICAL damage mod (Impact / Puncture / Slash). Scales the BASE of that
    /// physical type — `base_t × (1 + Σ)` — a SEPARATE multiplier that is
    /// MULTIPLICATIVE with base damage (Serration applies after), and does NOT
    /// enter the elemental hierarchy. No effect on a type the weapon lacks
    /// (wiki Damage/Calculation; MECHANICS.md §2).
    Physical(DamageType, f64),
    /// A CONDITIONAL/triggered buff's contribution to a stat bucket, valued at
    /// its assumed-max total (per_stack × max_stacks). Applied ONLY under
    /// `StackPolicy::AssumedMax` (the panel/optimizer's optimistic view); the
    /// emergent sim leaves it to the timeline. For triggered-buff mods whose
    /// trigger isn't event-modeled (on_ability_cast / on_reload / on_hit / …).
    CondBuff(CondBucket, f64),
    /// Galvanized Diffusion's on-kill multishot stacks.
    OnKillMultishot {
        per_stack: f64,
        max_stacks: u32,
        duration: f64,
    },
    /// Condition Overload payload (Galvanized Shot): +per_stack per
    /// status TYPE on the target, per on-kill stack, direct hits only.
    ConditionOverload {
        per_stack: f64,
        max_stacks: u32,
        duration: f64,
    },
    /// Galvanized Crosshairs' single refreshable buff: on HEADSHOT,
    /// +bonus relative crit chance (while aiming) for `duration`.
    OnHeadshotCritChance { bonus: f64, duration: f64 },
    /// Galvanized Crosshairs' stacks: on HEADSHOT KILL, +per_stack
    /// relative crit chance; each stack has its OWN duration (per-stack
    /// expiry FIFO — unlike the other Galvanized mods' decay).
    OnHeadshotKillCritChance {
        per_stack: f64,
        max_stacks: u32,
        duration: f64,
    },
    /// INDIRECT stat bonus (recoil, accuracy, ammo, projectile speed…):
    /// an additive bucket per stat, resolved into
    /// [`ResolvedPanel::indirect`]. Indirect = excluded from the
    /// theoretical-DPS formula, but a real input to practical combat —
    /// a future shooter model can consume them (recoil/accuracy → hit &
    /// headshot probability; projectile speed → travel time vs moving
    /// targets; ammo/holstered reload → long-fight sustain). Noise and
    /// dodge/roll speed are stealth/survivability, never DPS.
    Indirect(IndirectStat, f64),
    /// Reflex Draw: temporary handling buff on weapon swap-in. Conditional
    /// and handling-only — never a static panel stat.
    OnEquipHandling { recoil: f64, accuracy: f64, duration: f64 },
    /// An effect gated on the player AIMING (`condition: while_aiming` in the
    /// data: Galvanized Crosshairs / Scope, Argon Scope, Hydraulic Crosshairs,
    /// Sharpened Bullets, Bladed Rounds, Pressurized Magazine, the Catalyzers).
    ///
    /// The sim used to satisfy this silently — every aim-gated buff fired
    /// whether or not the scenario implied aiming, which flatters any build
    /// carrying one (user, 2026-07-30). It is now a SCENARIO knob: resolve with
    /// `aiming = false` and the wrapped effect contributes nothing at all.
    /// Wrapping rather than adding a flag to each variant keeps every other
    /// arm of the resolver unaware that aiming exists.
    /// Gated on what the PLAYER is doing — "while aiming", "while Invisible",
    /// "while Airborne". Asked of [`crate::tenno_data::Tenno`], the fight's
    /// second actor, so a card's condition and the fight's state meet in one
    /// place instead of aiming having its own parameter (user, 2026-08-02).
    WhileTenno(TennoCondition, Box<ModEffect>),
    /// Faction damage bonus (Bane/Expel/Cleanse/Smite): +v total damage vs a
    /// MATCHING enemy faction. Its own multiplicative bucket, ADDITIVE with
    /// other faction sources; **double-dips on DoT ticks** (applied twice).
    /// Conditional on the target's faction — no effect vs a non-match.
    FactionDamage(Faction, f64),
    /// Magazine capacity bonus (+v of base magazine, additive; floored to a
    /// whole round). Feeds reload cadence / long-fight sustain.
    MagazineCapacity(f64),
    /// Blast RANGE (+v of base radius) — Firestorm/Fulmination. The mods say
    /// "+X% Blast Range", NOT Blast damage: reading that description as an
    /// element is what had Primed Firestorm inventing +44% Blast damage on
    /// every AoE weapon.
    ///
    /// It scales every part that HAS a radius: the radial explosion and the
    /// lingering field. The field is measured (2026-07-30) and the wiki says so
    /// too ("Firestorm mods will now affect Torid gas clouds"). No single-target
    /// damage consequence — the target stands at the epicentre either way — but
    /// it is what the panel states, and Primary Compression reads the MODDED
    /// radius (MECHANICS §7).
    BlastRadius(f64),
    /// Status-duration bonus (+v): scales status-effect DoT DURATION (→ more
    /// ticks) and slows Heat's armour-strip ramp. No effect on instant procs.
    StatusDuration(f64),
    /// Weak Point damage (Pistol Acuity). The LISTED value; on a true weak
    /// point (humanoid head) the actual bonus is 1.5× the listed value ADDED
    /// to the part's Weak Point Multiplier, and the sum is MULTIPLICATIVE
    /// with the headshot-multiplier bracket (wiki Pistol_Acuity notes:
    /// Butcher 3x head + rank-10 Acuity = 3 + 3.5 × 1.5 = 8.25x).
    WeakpointDamage(f64),
    /// Weak Point crit chance (Pistol Acuity): a NORMAL relative crit-chance
    /// bonus (additive with Pistol Gambit — the multiplicative-crit behavior
    /// was a bug fixed in 38.5) that is only active on weak-point hits.
    WeakpointCritChance(f64),
    /// Sharpened Bullets: on ANY kill, +bonus relative crit damage (while
    /// aiming — the sim assumes constant aiming) for `duration` seconds.
    OnKillCritDamage { bonus: f64, duration: f64 },
    /// SENTIENT SURGE: crit chance and status chance per ACTIVE TENDRIL.
    ///
    /// The two travel together because they ARE one number — "Status Chance /
    /// Crit Chance Increase" is a single column on the wiki's rank table, and
    /// no rank of this mod raises one without the other. The magazine refill
    /// is its own column there and its own effect here.
    PerTendril { crit_chance: f64, status_chance: f64 },
    /// HATA-SATYA: relative crit chance per HIT, and the RELOAD takes it back.
    ///
    /// The pile has no clock at all — "Resets upon reloading or holstering" —
    /// which is why it is not a [`StackingBuff`] with a duration. It is the
    /// same shape as the Ocucor's tendrils one variant up (earned on an event,
    /// cleared by a magazine event, capped) with the event swapped: a hit
    /// instead of a kill.
    ///
    /// `max_stacks` is the CEILING divided by the per-stack value, floored:
    /// the wiki caps the bonus at "500% at all mod ranks" and ranks only the
    /// rate, so at max rank 1.2% a stack the last stack that fits is the 416th
    /// (499.2%) and a 417th would overshoot the published ceiling.
    CritChancePerHit { per_stack: f64, max_stacks: u32 },
    /// ...and that refill: a fraction of the magazine back on every kill,
    /// drawn from the reserve ("This mod does not generate ammo").
    MagazineRefillOnKill(f64),
    /// A SYNDICATE AUGMENT's radial (Gilded Truth grants Truth). The payload
    /// belongs to the SYNDICATE and is looked up by id — six effects shared by
    /// dozens of cards, so the card names one rather than restating it.
    ///
    /// `amount` is the card's own number ("+1 Truth"). The sim does not read
    /// it: the points a gauge needs depend on the mod's RANK and are 1000 at
    /// max, and every mod here simulates at max rank.
    SyndicateRadial { syndicate: &'static str, amount: f64 },
    /// Pressurized Magazine: on reload, +bonus relative fire rate (while
    /// aiming) for `duration` seconds.
    OnReloadFireRate { bonus: f64, duration: f64 },
    /// Deadly Efficiency: "On Reload From Empty: +X% Damage for Xs" — a
    /// relative BASE-damage bonus whose window opens when the reload
    /// COMPLETES, not when the magazine runs out (owner, 2026-08-01). The
    /// distinction is worth a modelled buff: at rank 10 it is +220% for 17 s,
    /// and under Emergent it used to contribute nothing at all.
    OnReloadDamage { bonus: f64, duration: f64 },
    /// EXIMUS ADVANTAGE: a relative BASE-damage window opened by a weak-point
    /// hit on an EXIMUS, and by nothing else.
    ///
    /// Two questions at once, which is what earns it a variant rather than a
    /// `kind: buff` with an `on_headshot` trigger: that would arm on any target
    /// and hand +600% base damage to a build the mod does nothing for. The
    /// target-side half is read live in the sim, where `Target::eximus` is in
    /// hand.
    ///
    /// The trigger is the WEAK POINT, not the head: "Despite the description
    /// specifying headshots, the effect can be trigger on weak-point hits"
    /// (wiki) — the same reading [`BuffTrigger::Headshot`] already carries. It
    /// REFRESHES rather than stacking, so one window with its clock restarted.
    OnEximusWeakpointDamage { bonus: f64, duration: f64 },
    /// Hemorrhage: each `from` status APPLIED rolls `chance` to also apply
    /// one `to` status (at most one roll per damage instance, and never
    /// alongside another `to` proc in the same instance). The chance is
    /// ×`low_rate_mult` while the weapon's LIVE fire rate is strictly below
    /// `low_rate_threshold` (exactly at the threshold gets no bonus).
    ProcConversion {
        from: DamageType,
        to: DamageType,
        chance: f64,
        low_rate_threshold: f64,
        low_rate_mult: f64,
    },
}

/// Indirect stat targets (each its own additive bucket) — outside the
/// theoretical-DPS formula, inside practical combat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndirectStat {
    Recoil,
    Noise,
    AmmoMax,
    ProjectileSpeed,
    HolsteredReload,
    DodgeSpeed,
    AcrobaticSpeed,
    Accuracy,
    /// Punch-through depth in METERS (multi-target only; no single-target DPS
    /// effect until the 2D multi-target model lands).
    PunchThrough,
    /// Aim zoom (FOV) — pistol zoom carries no damage bonus (unlike snipers).
    Zoom,

    // ---- 2D groundwork (2026-08-01) ------------------------------------
    // Everything below was `kind: unmodeled` — LOADED AS NOTHING, so the mod
    // equipped and the number vanished. They are real stats with no
    // SINGLE-TARGET damage payload, which is what this bucket is for: the
    // value now survives into the panel, the API and a build's saved state,
    // and the 2D world reads them instead of re-deriving them from card text.
    /// Weapon RANGE, as a fraction (Ballista Measure +20%). Not beam length.
    Range,
    /// Beam LENGTH in metres, flat (Sinister Reach +12 m, Ruinous Extension
    /// +8 m). A separate stat from `Range`: different unit, different weapons.
    BeamRange,
    /// Movement speed while AIMING (Agile Aim).
    MovementSpeed,
    /// Sprint speed — a WARFRAME stat the weapon carries (Amalgam Serration),
    /// which is exactly why an Amalgam mod cannot go on a companion weapon.
    SprintSpeed,
    /// Ammo PICKUP conversion: what another slot's ammo drop is worth to this
    /// weapon (Ammo Mutation, Vigilante Supplies). Needs a pickup economy.
    AmmoConversion,
    /// Chance to resist staggers/knockdowns while aiming (Resolute Focus).
    StaggerResist,
    /// Chance to reduce the stagger a SELF-inflicted radial attack causes
    /// (Cautious Shot) — the self-damage side of an AoE weapon.
    SelfStagger,
    /// Extra double jumps refreshed on kill while airborne (Aerial Ace) — a
    /// COUNT, not a fraction.
    DoubleJump,
    /// Flat damage a killed enemy explodes for (Combustion Beam). Real damage,
    /// but it needs a second enemy to land on.
    KillExplosion,
    /// Chance for a status to spread to enemies within 6 m (Shivering
    /// Contagion). Also multi-target only.
    StatusSpread,
    /// A SYNDICATE RADIAL's scale — "+1 Truth" on Gilded Truth. The EFFECT is
    /// `ModEffect::SyndicateRadial`, which names the syndicate; this bucket
    /// only exists so the card's number has somewhere to print.
    SyndicateRadial,
}

impl IndirectStat {
    pub fn label(&self) -> &'static str {
        match self {
            IndirectStat::Recoil => "Recoil",
            IndirectStat::Noise => "Noise Reduction",
            IndirectStat::AmmoMax => "Ammo Reserve",
            IndirectStat::ProjectileSpeed => "Projectile Speed",
            IndirectStat::HolsteredReload => "Holstered Reload/s",
            IndirectStat::DodgeSpeed => "Dodge Speed",
            IndirectStat::AcrobaticSpeed => "Acrobatic Speed",
            IndirectStat::Accuracy => "Accuracy",
            IndirectStat::PunchThrough => "Punch Through",
            IndirectStat::Zoom => "Zoom",
            IndirectStat::Range => "Range",
            IndirectStat::BeamRange => "Beam Range",
            IndirectStat::MovementSpeed => "Movement Speed (aiming)",
            IndirectStat::SprintSpeed => "Sprint Speed",
            IndirectStat::AmmoConversion => "Ammo Pickup Conversion",
            IndirectStat::StaggerResist => "Stagger Resist (aiming)",
            IndirectStat::SelfStagger => "Self-Stagger Reduction",
            IndirectStat::SyndicateRadial => "Syndicate Radial",
            IndirectStat::DoubleJump => "Double Jumps",
            IndirectStat::KillExplosion => "Explosion on Kill",
            IndirectStat::StatusSpread => "Status Spread Chance",
        }
    }

    /// How the stored number READS. Most of these are fractions and print as
    /// a percentage, but three are not, and printing "+1200.0%" for a 12 m
    /// beam extension is worse than not stating it at all.
    pub fn unit(&self) -> &'static str {
        match self {
            IndirectStat::PunchThrough | IndirectStat::BeamRange => "m",
            IndirectStat::DoubleJump => "x",
            IndirectStat::KillExplosion => "",
            _ => "%",
        }
    }

    /// The stat's value as it READS. One implementation, because there are two
    /// callers — this enum's own effect line and the API's stat table — and
    /// they disagreeing is how "+800% Beam Range (m)" reached the picker for a
    /// mod that grants 8 metres.
    pub fn format(&self, v: f64) -> String {
        let unit = self.unit();
        if unit == "%" {
            return pct(v);
        }
        let a = v.abs();
        let s = if (a - a.round()).abs() < 1e-6 {
            format!("{}", a.round() as i64)
        } else {
            format!("{a:.2}").trim_end_matches('0').trim_end_matches('.').to_string()
        };
        format!("{}{s}{unit}", if v >= 0.0 { "+" } else { "−" })
    }
}

/// Is the char at `i` a rank-varying `X` placeholder in a description
/// template? Matches the data convention (docs: description-X): a bare `X`
/// (`+X%`, `+X Punch Through`), the multiplier form `xX`, and the UNIT forms
/// `Xm` / `Xs` / `Xx` — but never a letter inside a word.
///
/// The unit suffixes are the subtle ones. Without them "…for Xs" and "Stacks
/// up to Xx." were not placeholders at all, so no value could ever be
/// substituted and the card showed a literal X — which is exactly how
/// Galvanized Chamber came to read "Stacks up to Xx."
fn is_x_at(b: &[char], i: usize) -> bool {
    if b[i] != 'X' {
        return false;
    }
    let prev_ok = i == 0 || !b[i - 1].is_ascii_alphabetic() || b[i - 1] == 'x';
    // A unit letter counts only when the word ENDS there: "Xm"/"Xs"/"Xx" are
    // placeholders, "Xmod" or "Xstack" would be a word starting with X.
    let unit_ends = |j: usize| b.get(j + 1).is_none_or(|c| !c.is_ascii_alphabetic());
    let next_ok = match b.get(i + 1) {
        None => true,
        Some('%') => true,
        Some('m' | 's' | 'x') => unit_ends(i + 1),
        Some(c) => !c.is_ascii_alphabetic(),
    };
    prev_ok && next_ok
}

/// What a description placeholder expects, read off the character right after
/// it. The `X` in "for Xs" wants a DURATION and the one in "up to Xx" a stack
/// cap; every other `X` wants the effect's rank-varying value.
///
/// Filling by POSITION alone put Galvanized Crosshairs' 12-second duration in
/// its crit slot and printed "+1200% Critical Chance" — a description that
/// writes its duration as a literal supplies no slot for it, so the values
/// after it all shifted up one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XKind {
    /// A rank-varying stat.
    Value,
    /// Seconds — "for Xs".
    Duration,
    /// A stack cap — "up to Xx".
    Stacks,
}

/// The kind of every `X` in a template, in order.
pub fn x_kinds(template: &str) -> Vec<XKind> {
    let b: Vec<char> = template.chars().collect();
    (0..b.len())
        .filter(|&i| is_x_at(&b, i))
        .map(|i| match b.get(i + 1) {
            Some('s') => XKind::Duration,
            Some('x') => XKind::Stacks,
            _ => XKind::Value,
        })
        .collect()
}

/// The LINE each `X` sits on, in the same order as [`x_kinds`].
///
/// A card breaks its lines where DE breaks them, and a line is one sentence
/// about one effect — "+X% Life Steal" then "+X Purity". That makes the line
/// the unit that says WHICH effect a placeholder is asking about, which is the
/// only thing position cannot say.
pub fn x_lines(template: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let b: Vec<char> = template.chars().collect();
    let mut line = 0usize;
    for i in 0..b.len() {
        if b[i] == '\n' {
            line += 1;
        } else if is_x_at(&b, i) {
            out.push(line);
        }
    }
    out
}

/// Number of rank-varying `X` placeholders in a description template.
pub fn count_x(template: &str) -> usize {
    let b: Vec<char> = template.chars().collect();
    (0..b.len()).filter(|&i| is_x_at(&b, i)).count()
}

/// Fill a description template's `X` placeholders with concrete values, in
/// order. Values are stored as BONUSES (schema): before `%` they render
/// ×100; in the multiplier form `xX` they render +1 (a stored 0.3 shows as
/// `x1.3`); any other position renders the raw number. Extra `X`s beyond
/// `vals` stay as-is (the caller's honest fallback).
pub fn fill_x(template: &str, vals: &[f64]) -> String {
    let b: Vec<char> = template.chars().collect();
    let mut out = String::new();
    let mut vi = 0;
    for i in 0..b.len() {
        if is_x_at(&b, i) && vi < vals.len() {
            let mut v = if b.get(i + 1) == Some(&'%') {
                vals[vi] * 100.0
            } else if i > 0 && b[i - 1] == 'x' {
                vals[vi] + 1.0
            } else {
                vals[vi]
            };
            // The template carries the sign ("+X%" / "-X%"); the stored
            // value may carry it too (corrupted downsides are negative
            // bonuses) — render the magnitude to avoid "--15%".
            if i > 0 && matches!(b[i - 1], '+' | '-' | '−') {
                v = v.abs();
            }
            let s = format!("{v:.2}");
            out.push_str(s.trim_end_matches('0').trim_end_matches('.'));
            vi += 1;
        } else {
            out.push(b[i]);
        }
    }
    out
}

/// "+60%" / "−15%" / "+109.5%" from a fraction (true minus sign).
pub fn pct(x: f64) -> String {
    let p = x.abs() * 100.0;
    let s = if (p - p.round()).abs() < 1e-6 {
        format!("{}", p.round() as i64)
    } else {
        format!("{p:.2}").trim_end_matches('0').trim_end_matches('.').to_string()
    };
    if x >= 0.0 { format!("+{s}%") } else { format!("−{s}%") }
}

impl ModEffect {
    /// One display line for this effect — OUR statement of what the model
    /// actually computes (true values; tooltip lies already corrected in
    /// the data). The single source for every effect list in the UI.
    pub fn describe(&self) -> String {
        use ModEffect::*;
        match *self {
            // The gate is stated as a suffix so the inner line reads normally.
            WhileTenno(c, ref inner) => format!("{} ({})", inner.describe(), c.label()),
            LastRoundDamage(v) => format!(
                "{} damage on the magazine's LAST round — its own multiplier, not the                  base-damage bucket (nothing on a continuous weapon or an Incarnon form)",
                pct(v)
            ),
            ConsecutiveHitDamage { per_stack, max_stacks, duration } => format!(
                "{} damage per consecutive hit, up to {max_stacks} ({}), for {duration}s — its own multiplier, not the base-damage bucket",
                pct(per_stack), pct(per_stack * f64::from(max_stacks))
            ),
            BaseDamage(v) => format!("{} Base Damage", pct(v)),
            Multishot(v) => format!("{} Multishot", pct(v)),
            CritChance(v) => format!("{} Crit Chance", pct(v)),
            // Both halves in one line, because they are one column on the card
            // and always equal; the refill is a separate sentence because it
            // answers a different question.
            PerTendril { crit_chance, .. } => {
                format!("{} Crit Chance and Status Chance per active tendril", pct(crit_chance))
            }
            CritChancePerHit { per_stack, max_stacks } => format!(
                "On Hit: {} Crit Chance per stack ×{max_stacks} ({}), cleared by a reload",
                pct(per_stack),
                pct(per_stack * f64::from(max_stacks))
            ),
            MagazineRefillOnKill(v) => format!("on kill, {} of the magazine back", pct(v)),
            SyndicateRadial { syndicate, amount } => {
                let d = crate::syndicates_data::get(syndicate);
                match d {
                    Some(d) => format!(
                        "+{amount} {} — {:.0} {:?} in {:.0} m once the weapon earns {:.0} affinity, every {:.0}s",
                        d.name, d.damage, d.element, d.radius_m, d.affinity_to_fill, d.cooldown_seconds
                    ),
                    None => format!("+{amount} {syndicate}"),
                }
            }
            CritDamage(v) => format!("{} Crit Damage", pct(v)),
            StatusChance(v) => format!("{} Status Chance", pct(v)),
            FireRate(v) => format!("{} Fire Rate", pct(v)),
            ChargeRate(v) => format!("{} Charge Rate", pct(v)),
            ReloadSpeed(v) => format!("{} Reload Speed", pct(v)),
            StatusDamage(v) => format!("{} Status Damage", pct(v)),
            SlashOnCrit(v) => format!("{} chance to apply Slash on Critical", pct(v)),
            Element(t, v) => format!("{} {t:?}", pct(v)),
            CombinedElement(t, v) => format!("{} {t:?}", pct(v)),
            Physical(t, v) => format!("{} {t:?}", pct(v)),
            CondBuff(b, v) => {
                format!("{} {} (conditional, assumed active)", pct(v), b.label())
            }
            OnKillMultishot { per_stack, max_stacks, duration } => {
                format!("On Kill: {} Multishot per stack ×{max_stacks}, {duration}s", pct(per_stack))
            }
            ConditionOverload { per_stack, max_stacks, duration } => {
                format!(
                    "On Kill: {} Damage per status type ×{max_stacks}, {duration}s (direct hits)",
                    pct(per_stack)
                )
            }
            OnHeadshotCritChance { bonus, duration } => {
                format!("On Headshot: {} Crit Chance, {duration}s", pct(bonus))
            }
            OnHeadshotKillCritChance { per_stack, max_stacks, duration } => {
                format!("On Headshot Kill: {} Crit Chance per stack ×{max_stacks}, {duration}s", pct(per_stack))
            }
            Indirect(stat, v) => format!("{} {}", stat.format(v), stat.label()),
            OnEquipHandling { recoil, accuracy, duration } => {
                format!("On Equip: {} Recoil, {} Accuracy, {duration}s", pct(recoil), pct(accuracy))
            }
            FactionDamage(fac, v) => format!("{} Damage to {fac:?}", pct(v)),
            MagazineCapacity(v) => format!("{} Magazine Capacity", pct(v)),
            BlastRadius(v) => format!("{} Blast Range (radius)", pct(v)),
            StatusDuration(v) => format!("{} Status Duration", pct(v)),
            WeakpointDamage(v) => format!("{} Weak Point Damage", pct(v)),
            WeakpointCritChance(v) => format!("{} Weak Point Crit Chance", pct(v)),
            OnKillCritDamage { bonus, duration } => {
                format!("On Kill: {} Crit Damage, {duration}s", pct(bonus))
            }
            OnReloadDamage { bonus, duration } => {
                format!("On reload from empty: {} Damage, {duration}s", pct(bonus))
            }
            OnEximusWeakpointDamage { bonus, duration } => {
                format!("On Eximus weak-point hit: {} Damage, {duration}s", pct(bonus))
            }
            OnReloadFireRate { bonus, duration } => {
                format!("On Reload: {} Fire Rate, {duration}s", pct(bonus))
            }
            ProcConversion { from, to, chance, low_rate_threshold, low_rate_mult } => {
                format!(
                    "{from:?} status: {} chance to also apply {to:?} (×{low_rate_mult} below {low_rate_threshold} fire rate)",
                    pct(chance)
                )
            }
        }
    }
}

/// A mod as the resolver sees it (stats at the equipped rank).
#[derive(Debug, Clone)]
pub struct ModDef {
    pub id: &'static str,
    /// DE's own name for the card. The yaml has always carried it and the
    /// engine used to DROP it, leaving `webapi` to rebuild a display name from
    /// the id — which is lossy in both directions: "Semi-Shotgun Cannonade"
    /// came back as "Semi Shotgun Cannonade" (so its wiki link 404'd),
    /// "Hell's Chamber" lost its apostrophe, and "Bane of Grineer" gained a
    /// capital O (user, 2026-08-03).
    pub name: &'static str,
    /// Drain at the EQUIPPED (max) rank.
    pub base_drain: u32,
    /// Max rank (drain rises 1/rank from rank 0, so rank-0 drain = base_drain − max_rank).
    pub max_rank: u32,
    pub polarity: Polarity,
    /// Card rarity (frame colour). Display-only — no mechanical effect.
    pub rarity: Rarity,
    /// Exilus (utility) mod: may occupy the exilus slot in addition to
    /// regular slots. Exilus mods are handling/QoL effects with no damage
    /// model, so the optimizer skips them.
    pub exilus: bool,
    /// Mods sharing a family are mutually exclusive (wiki Incompatible).
    pub family: Option<&'static str>,
    /// Weapon property required to EQUIP this mod at all — "continuous" for
    /// the beam-only mods. Distinct from `requires`, which is a calc-layer
    /// gate: that one equips and sits inert, this one is never offered.
    pub requires_weapon: Option<&'static str>,
    /// The weapons this mod may be equipped on, and nothing else. Empty means
    /// "any weapon whose pool carries it" — see `mods_data`.
    pub exclusive_to: &'static [&'static str],
    /// DE's INCOMPATIBILITY tags for this mod, lowercased — the mirror of
    /// `requires_weapon`, and the reason Amalgam Serration is not offered on
    /// a sentinel weapon while plain Serration is (wiki: "This mod cannot be
    /// equipped on Sentinel weapons", tags `SENTINEL_WEAPON, POWER_WEAPON`).
    /// An Amalgam mod's second half buffs the WARFRAME, which is why the
    /// weapon a companion carries cannot hold one.
    pub excludes_weapon: Vec<&'static str>,
    /// The MOD SET this mod belongs to (`data/mod_sets/<id>.yaml`). A set
    /// bonus is granted by the group, not by any member, and it scales per
    /// equipped member with no threshold — see [`crate::mod_sets_data`].
    pub set: Option<&'static str>,
    /// Weapon TRAIT this mod's effects require to apply (else the whole mod is
    /// inert — a calc-layer gate, NOT an equip block). Declared only for
    /// general effects that would otherwise be misapplied (Semi-Pistol
    /// Cannonade → `semi_auto`); self-gating effects (beam range) declare none.
    pub requires: Option<&'static str>,
    /// Stats this mod LOCKS from being modified while equipped (Pistol Acuity →
    /// `multishot`, Semi-Pistol Cannonade → `fire_rate`): every mod's bonus to
    /// that bucket is zeroed in `resolve`.
    pub disables: Vec<&'static str>,
    pub effects: Vec<ModEffect>,
    /// Does this mod have an effect the sim knowingly does NOT model?
    ///
    /// False for almost every mod. True means the CARD must say so — an
    /// `unmodeled` effect is DROPPED at load, so a mod carrying one loads as a
    /// mod that does nothing and says nothing, which is exactly how it looks to
    /// a player who equips it and sees no change (reported 2026-08-05 about
    /// Primary Debilitate). The reason lives in the YAML comment beside the
    /// effect, where a maintainer reads it — not in a field the app renders.
    pub unmodeled: bool,
    /// ...and does it act on something this simulator does not HAVE — Warframe
    /// energy, enemy behaviour, traversal, reviving? Never a todo: building it
    /// would not move a damage figure. Told apart from `unmodeled` because
    /// saying "not modelled" for both makes the model's own edge look like
    /// unfinished work (2026-08-05).
    pub out_of_scope: bool,
}

/// Mod card rarity — determines the in-game frame colour (bronze / silver /
/// gold / white). Purely cosmetic; carried for UI display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Legendary,
}

impl ModDef {
    /// The primary element this mod adds, if any (position-sensitive).
    pub fn primary_element(&self) -> Option<DamageType> {
        self.effects.iter().find_map(|e| match e {
            ModEffect::Element(t, _) => Some(*t),
            _ => None,
        })
    }
}

/// How stacking/conditional effects are valued (docs/OPTIMIZER.md §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackPolicy {
    /// Full stacks / 100% uptime on every conditional buff.
    AssumedMax,
    /// On-kill stacking buffs start at their configured INITIAL stacks
    /// (full, per user 2026-07-24 correction) and then evolve purely by
    /// mechanics: kills refresh/grant, timeouts decay one stack.
    Emergent,
    /// A COMPANION's weapon. Galvanized (and other conditional
    /// on-kill/on-headshot/on-reload) effects can be EQUIPPED and only their
    /// unconditional BASE part applies.
    ///
    /// The reason is not that a companion is excluded from the buff — it is
    /// not. The TRIGGER belongs to the Tenno: the on-kill roll comes from the
    /// Tenno's own weapons, and the stacks it grants then apply to the Tenno
    /// AND the companion (user, 2026-07-31). What this arena cannot do is
    /// simulate the two together — it fires ONE weapon — so when that weapon
    /// is the companion's, nothing on the field can generate the stacks and
    /// only the base is honest.
    ///
    /// So this is an ARENA limit, not a game rule, and it is the wrong answer
    /// the moment a Tenno weapon and a companion weapon are simulated side by
    /// side. (wiki `Galvanized_Mods`; user 2026-07-25, corrected 2026-07-31)
    BaseOnly,
}

/// "No timeout": the duration a LOCKED buff card runs on.
///
/// Locking is not a flag the engine has to remember to consult — it OVERWRITES
/// the buff's duration (user, 2026-08-04). Every clock in the sim is
/// `expiry = now + duration`, so an infinite duration gives a buff that starts
/// where its card says, still climbs on every trigger, and never expires —
/// which is exactly what the label promises, expressed in the one place that
/// can express it.
///
/// The flag it replaces (`pinned` / `locked`) had to be honoured at every read
/// site, and was missed at several across three buff families: the stacks
/// decayed anyway and the trigger was skipped, so "no timeout" came to mean
/// "decays to zero and can never come back". A duration cannot be forgotten —
/// there is nothing left to thread.
pub const NO_TIMEOUT: f64 = f64::INFINITY;

/// A live on-kill stacking buff spec handed to the sim under
/// [`StackPolicy::Emergent`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StackSpec {
    /// Contribution per stack (multishot: already × base pellets; CO:
    /// per-type rate).
    pub per_stack: f64,
    pub max_stacks: u32,
    /// Per-refresh duration; decay = lose ONE stack and reset (the
    /// Galvanized family's graceful decay). [`NO_TIMEOUT`] when the buff card
    /// is locked — the stacks then climb as usual and never fall off.
    pub duration: f64,
    /// Stacks at t = 0 (user setting: full by default, 0 for a cold
    /// start; afterwards mechanics rule either way).
    pub initial_stacks: u32,
}

/// A non-stacking timed buff (a single refreshable window) handed to the sim:
/// Galvanized Crosshairs' on-headshot crit, Sharpened Bullets' on-kill crit
/// damage, Pressurized Magazine's on-reload fire rate. Unifies what used to be
/// three parallel `Option<(f64, f64)>` fields.
/// EXECUTIONER'S FORTUNE: a headshot's chance to FILL THE MAGAZINE, no reload
/// played and no time spent.
///
/// It is not a reload-speed bonus and it is not a percentage refill — it is the
/// reload happening for free, which is why it lives beside the magazine rather
/// than in the reload bucket. Like every other magazine refill in this engine
/// it draws from the RESERVE, so a dry one gives nothing.
///
/// **It does nothing in an Incarnon form**, and the wiki's "Does not affect
/// Incarnon Form" is not a special case bolted on: what this refills is a
/// MAGAZINE, and an Incarnon form has max CHARGES instead. A charge pool is
/// converted from weakpoint hits and sits outside the ammo economy, so it has
/// no reload to make instant.
/// LINGERING JUDGEMENT: a headshot STREAK arms extra headshot damage.
///
/// The bonus joins the ADDITIVE headshot bracket, beside Primary Deadhead's
/// ("stacks additively with Primary Deadhead's headshot damage bonus"), so a
/// build already carrying the arcane gets far less out of it than +50% reads.
#[derive(Debug, Clone, Copy)]
pub struct HeadshotStreak {
    /// Headshots needed (2), inside `within` seconds.
    pub hits: u32,
    pub within: f64,
    /// The headshot-damage bonus while the window is open (0.50).
    pub value: f64,
    /// How long the window lasts once armed (8 s).
    pub duration: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct InstantReload {
    /// Per qualifying headshot (0.10 on the Furis pair, 0.20 on the Phenmor).
    pub chance: f64,
    /// Must the headshot also KILL? The Phenmor says so; the Furis pair do not.
    pub needs_kill: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimedBuff {
    /// The ABSOLUTE bonus this buff contributes while active.
    pub value: f64,
    /// Window length; each trigger refreshes it to `now + duration`.
    /// [`NO_TIMEOUT`] when the buff card is locked, so the window a trigger
    /// opens never closes again.
    pub duration: f64,
    /// Active at t = 0? (per-buff seed — cc_on_headshot starts active, the
    /// on-kill/on-reload buffs start inactive under today's defaults).
    pub initial_active: bool,
}

/// How the Condition Overload bonus behaves — PER WEAPON (user,
/// 2026-07-24: "some weapons take it as an independent multiplier, some
/// fold it into base damage, and some don't benefit at all"; the wiki
/// CO-mechanic catalog classifies weapons):
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoBehavior {
    /// Joins the base-damage bucket (additive with Hornet Strike):
    /// direct hit × (1 + bd + co × types) / (1 + bd).
    AdditiveWithBaseDamage,
    /// A free-standing final multiplier on direct hits:
    /// direct hit × (1 + co × types).
    Independent,
    /// The bonus simply does not apply on this weapon.
    Inert,
}

/// A QUESTION ABOUT THE PLAYER that a weapon perk asks — "With Sprint Speed 1.2
/// or Higher", "With Armor Over 450", "With Energy Max Over 700".
///
/// One vocabulary rather than a field pair per grant. The first two of these
/// (Condition Overload, fire rate) each carried their own `_gated` value and
/// `_min_*` threshold on [`WeaponBase`], and the note left there said the third
/// should turn them into one mechanism. This is that.
///
/// Answered in [`resolve_for`], where the Tenno is — `apply` works on the raw
/// weapon and the player is not there. The neutral player claims nothing
/// (sprint 0.9, no armor, no energy), so a gated perk pays zero until someone
/// says which frame is holding the gun.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TennoGate {
    /// `sprint_speed >= x`
    SprintAtLeast(f64),
    /// `armor > x`
    ArmorOver(f64),
    /// `energy_max > x`
    EnergyMaxOver(f64),
    /// `overshields` — Haven Foray, Guardian's Might: "With Overshields".
    /// A yes/no rather than a threshold, which is what the card asks.
    HasOvershields,
    /// `channeling` — Daring Reverie, Hunter's Mantra: "With Channeled Ability
    /// active". A yes/no, and its definition is the card's own note: the
    /// ability must be DRAINING ENERGY over time.
    ChannelingAbility,
    /// `solo_weapon` — the Vasto's Lone Gun: "With No Primary Equipped".
    ///
    /// The first gate that asks about the LOADOUT rather than about the frame
    /// or what it is doing, and the difference matters: this arena has always
    /// fired one weapon for a whole engagement, which says nothing about what
    /// else is in the other two slots. See [`crate::tenno_data::TennoState`].
    SoloWeapon,
}

impl TennoGate {
    /// Does this player satisfy it?
    pub fn open(self, tenno: &crate::tenno_data::Tenno) -> bool {
        match self {
            TennoGate::SprintAtLeast(x) => tenno.sprint >= x,
            TennoGate::ArmorOver(x) => tenno.armor > x,
            TennoGate::EnergyMaxOver(x) => tenno.energy > x,
            TennoGate::HasOvershields => tenno.state.overshields,
            TennoGate::ChannelingAbility => tenno.state.channeling,
            TennoGate::SoloWeapon => tenno.state.solo_weapon,
        }
    }

    /// The sentence a card shows. English is the source; the overlay translates.
    pub fn describe(self) -> String {
        match self {
            TennoGate::SprintAtLeast(x) => format!("at sprint speed {x} or higher"),
            TennoGate::ArmorOver(x) => format!("with armor over {x}"),
            TennoGate::EnergyMaxOver(x) => format!("with max energy over {x}"),
            TennoGate::HasOvershields => "with overshields".to_string(),
            TennoGate::ChannelingAbility => "with a channeled ability active".to_string(),
            TennoGate::SoloWeapon => "with no other weapon equipped".to_string(),
        }
    }
}

/// WHAT A GATED PERK GRANTS. One arm per bracket, and each keeps its own —
/// the same rule [`BuffGrant`] follows, and for the same reason: a multishot
/// bonus and a crit-damage one are not interchangeable numbers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GatedGrant {
    /// Per status type, into the Condition Overload rate.
    ConditionOverload,
    /// A fraction of the BASE fire rate, additive with fire-rate mods.
    FireRate,
    /// A fraction of the base multishot, into the multishot bucket.
    Multishot,
    /// Added to the weapon's base crit multiplier, so crit-damage mods
    /// multiply it (the card says "Base Critical Damage Multiplier").
    BaseCritDamage,
    /// A fraction, into the projectile-speed indirect bucket.
    ProjectileSpeed,
    /// A fraction, into the ACCURACY indirect bucket — which narrows the cone
    /// a pellet draws inside (`spread`). It was worth nothing until the arena
    /// had a distance, which is why the three Hunter's Mantra cards that grant
    /// it were declared out of scope until 2026-08-15.
    Accuracy,
    /// An ABSOLUTE add to the weapon's base damage, folded exactly as an
    /// ungated one is — [`WeaponBase::add_flat_base_damage`] is the one
    /// implementation, so "+40 with overshields" and a plain "+40" cannot come
    /// out as different panels.
    FlatBaseDamage,
    /// An ABSOLUTE add to the weapon's base MAGAZINE — Lone Gun's "+14 Base
    /// Magazine Capacity" beside its "+40 Base Damage".
    ///
    /// Folded into `magazine_size` BEFORE the magazine mods multiply, which is
    /// where `apply` puts the ungated spelling, so a gated +14 and a plain +14
    /// are the same weapon down to the by-shell reload and a charged form's
    /// ammo ratio. "Increased Base Magazine Capacity does not affect Incarnon
    /// Form" is the same `incarnon.is_none()` guard `apply` uses.
    FlatBaseMagazine,
}

/// A weapon's unmodded panel (fixed evolutions folded in — they alter the
/// weapon's BASE stats before mods).
#[derive(Debug, Clone)]
pub struct WeaponBase {
    /// WHICH FORM this panel is, from the yaml entry's own `form:`.
    ///
    /// A two-weapons pair is two entries and two panels, and until now neither
    /// knew which of the two it was — every evolution applied to both. Eleven
    /// evolutions say *"Does not affect Incarnon Form"* and this is what lets
    /// them be obeyed rather than transcribed and ignored.
    pub form: crate::weapons_data::FormKind,
    /// Indirect stats the WEAPON itself brings, before any mod — today only
    /// EVOLUTIONS write here (Practiced Grip's +50% accuracy, Marksman's
    /// Hand's recoil, Swift Deliverance's projectile speed). `resolve` seeds
    /// the panel's `indirect` from this and mods add into the same buckets, so
    /// an evolution's handling stat lands exactly where a mod's does.
    pub indirect: Vec<(IndirectStat, f64)>,
    pub base_vector: DamageVector,
    pub base_crit_chance: f64,
    pub base_crit_damage: f64,
    pub base_status_chance: f64,
    pub base_fire_rate: f64,
    /// CHARGE trigger (bows): the draw before the shot, unmodded. `Some` moves
    /// the cadence off `1 / fire_rate` — a charged weapon fires once its draw
    /// completes, and fire-rate bonuses shorten THAT. `base_fire_rate` stays
    /// the listed stat (Cernos Prime: 1.0), which is what reads it as a stat
    /// (Hemorrhage's below-2.5 gate) still sees.
    pub charge_seconds: Option<f64>,
    /// See [`crate::weapons_data::AttackSpec::charge_ammo_per_second`]. Set,
    /// the charge spends the magazine and the damage rides on it.
    pub charge_ammo_per_second: Option<f64>,
    /// See [`crate::weapons_data::SustainedFireRate`]. It rides through the mod
    /// layer UNTOUCHED — it is a fraction of whatever rate the build ends up
    /// with, so a fire-rate mod raises the ceiling and the floor together.
    pub sustained_fire_rate: Option<crate::weapons_data::SustainedFireRate>,
    /// See [`crate::weapons_data::Battery`]. Untouched by the mod layer too:
    /// the regen rate is the weapon's and a magazine mod changes only how many
    /// rounds it has to refill.
    pub battery: Option<crate::weapons_data::Battery>,
    /// Ammo spent per shot / per beam tick (weapon data `attack.ammo_cost`).
    pub ammo_cost: f64,
    /// See `weapons_data::WeaponSpec::headshot_bonus_multiplicative`.
    pub headshot_bonus_multiplicative: bool,
    /// Does a fire-rate bonus shorten the DRAW? False for Arch-Guns, whose
    /// fire rate paces only the interval — see `weapons_data`.
    pub fire_rate_shortens_draw: bool,
    /// Which charge formula paces it — see [`crate::weapons_data::ChargeCadence`].
    pub charge_cadence: crate::weapons_data::ChargeCadence,
    /// A BURST trigger's shape, unmodded — see [`crate::weapons_data::BurstSpec`].
    pub burst: Option<crate::weapons_data::BurstSpec>,
    /// What a fire-rate MOD's bonus is multiplied by on this weapon — 2.0 for
    /// bows, whose cards all print "(x2 for Bows)". It reaches the mod bucket
    /// only: a mod-granted BUFF (Pressurized Magazine's on-reload fire rate)
    /// carries no such clause, so it is not doubled. UNVERIFIED for buffs; no
    /// bow-eligible fire-rate buff is in the roster to measure it with.
    pub fire_rate_mod_multiplier: f64,
    /// Stored pellet count (wiki Multishot).
    pub base_multishot: f64,
    /// Extra additive multishot from non-mod sources at assumed-max
    /// (Fevered Frenzy's 20 stacks = +1.0).
    /// Flat BASE damage an evolution grants through a PERMANENT buff rather
    /// than unconditionally — Boar Prime's Reified Bane, "On Reload From
    /// Empty: +14 Base Damage". `base_vector` already carries it (the buff
    /// starts full, like Fevered Frenzy's multishot); this records how much of
    /// it is the buff's, so a buff card can take it back off.
    pub reload_damage_buff: f64,
    pub buff_multishot_bonus: f64,
    /// Stack count behind `buff_multishot_bonus` (Fevered Frenzy: 20). The
    /// stacks are PERMANENT (no timer, cleared only by death) and their
    /// trigger (ability cast) cannot fire in the sim — so the count is a
    /// static per-buff CHOICE, full by default. 0 = no such buff.
    pub buff_ms_max_stacks: u32,
    pub magazine_size: f64,
    /// Base reserve rounds (wiki "Ammo Max"), before mods.
    pub ammo_reserve: f64,
    /// Has this weapon a reserve behind its magazine at all? Derived from
    /// `ammo_max`: false only where the weapon states none, which today is
    /// every sentinel weapon ("Ammo Max: ∞ / Ammo Type: None").
    pub has_reserve: bool,
    /// Gotva Prime's passive: a status-triggered crit-chance SET. See
    /// `weapons_data::SuperCritSpec`.
    pub super_crit_on_status: Option<crate::weapons_data::SuperCritSpec>,
    /// Where this weapon's beam ramp starts (0.20 unless it says otherwise).
    pub beam_ramp_floor: f64,
    /// Does this weapon apply MICROWAVE? See `dummy::DebuffState::microwave`.
    pub applies_microwave: bool,
    /// See `weapons_data::WeaponSpec::independent_procs`. No mod adds or
    /// removes one — it is what the weapon DOES, not what the build asks for.
    pub independent_procs: &'static [&'static str],
    /// Damage types forced on every DIRECT hit — see
    /// `weapons_data::AttackSpec::forced_procs`.
    pub forced_procs: Vec<DamageType>,
    /// ONE PULL, ONE ELEMENT EACH — see
    /// `weapons_data::AttackSpec::pellet_elements`. Empty on every weapon
    /// whose projectiles share an element, which is all but one of them.
    pub pellet_elements: Vec<DamageType>,
    /// See `weapons_data::AttackSpec::multishot_adds_damage`.
    pub multishot_adds_damage: bool,
    /// See `weapons_data::AttackSpec::attractor_seconds`.
    pub attractor_seconds: Option<f64>,
    /// How many tendrils this weapon can hold up (0 = it has none). See
    /// `weapons_data::TendrilSpec` for why the COUNT is modelled and the
    /// tendrils' own damage is not.
    pub tendril_max: u32,
    /// How far a tendril reaches and how far off the reticle it will take a
    /// body — see [`crate::weapons_data::TendrilSpec`]. Both zero on every
    /// weapon that has no tendrils.
    pub tendril_range_m: f64,
    pub tendril_acquire_deg: f64,
    /// The sniper's Shot Combo Counter, before `resolve` asks whether the
    /// Tenno is aiming — see `weapons_data::SniperCombo`.
    pub sniper_combo: Option<crate::weapons_data::SniperCombo>,
    /// ...and the scope's headshot bonus at its top zoom level, likewise
    /// unspent until `resolve` (0.0 = no scope).
    pub scope_headshot_damage: f64,
    /// ...or the scope's CRIT bonuses, for the weapons whose zoom grants those
    /// instead. Spent by `resolve`, like the headshot one, and only while
    /// aiming. See `weapons_data::ScopeSpec`.
    pub scope_crit_chance: f64,
    pub scope_crit_multiplier: f64,
    /// The Lanka's kind — see `weapons_data::ScopeSpec::crit_chance_post_mod`.
    pub scope_crit_chance_post_mod: f64,
    /// ...and can it NOT be refilled mid-fight? See `WeaponSpec::no_resupply`.
    /// Separate from the above on purpose — most weapons have a reserve AND a
    /// way to top it up.
    pub no_resupply: bool,
    pub base_reload: f64,
    /// A BY-ROUND reload, as `(start, per shell, end)` seconds. `None` = the
    /// ordinary one-block reload. See `WeaponSpec::reload_style`: the whole
    /// point is that the magazine size is IN the reload time, so a magazine
    /// mod on a Strun or a Felarx costs what the game charges for it.
    pub by_round_reload: Option<(f64, f64, f64)>,
    /// Unconditional CO rate baked into the weapon config (Carnage
    /// Reign's +33% per status type) — additive with mod CO sources.
    pub innate_co_per_type: f64,
    /// The same thing, waiting on the PLAYER: an evolution's Condition Overload
    /// that states a sprint speed. It joins `innate_co_per_type` in
    /// `resolve_for`, where the Tenno exists, or contributes nothing.
    /// EVERY GRANT THIS WEAPON MAKES CONDITIONAL ON THE PLAYER, as
    /// `(gate, grant, value)`. Carried rather than spent because `apply` works
    /// on the raw weapon and the Tenno is not there; folded in `resolve_for`,
    /// which has both.
    pub gated: Vec<(TennoGate, GatedGrant, f64)>,
    /// King's Gambit: a MULTIPLIER on crit chance for a hit that did NOT land on
    /// a weak point. 1.0 = ordinary.
    ///
    /// VERBATIM (Sicarus_Incarnon_Genesis): "x0 Critical Chance on Bodyshots,
    /// +150% Critical Chance on Weakpoint Hits", with the note that settles the
    /// bracket — "Bodyshot modifier is MULTIPLICATIVE with all sources of
    /// Critical Chance, effectively making non-headshot critical hits
    /// impossible". Its other half is additive and already has a home:
    /// "Weakpoint modifier is ADDITIVE with mods such as Pistol Gambit", which
    /// is `weakpoint_cc_rel`.
    pub bodyshot_cc_mult: f64,
    /// GALVANIC RELOAD: `(status, chance, rounds)` — "On hitting a target
    /// affected by an Electricity status, 40% chance to restore 1 round in the
    /// magazine from ammo pool".
    ///
    /// ONCE PER SHOT, not per pellet: the card says "The bonus can only apply
    /// once per enemy hit", and this is a shotgun family where the difference is
    /// tenfold. The rounds come FROM THE AMMO POOL, so a dry reserve restores
    /// nothing — and a refill is not a reload, the same rule
    /// `mag_refill_on_kill` follows.
    pub round_restore_on_status: Option<(crate::damage::DamageType, f64, f64)>,
    /// Exact Penance: the chance a KILL — from anywhere, including a status
    /// kill — reloads instantly. See the ResolvedPanel field for why it is not
    /// `instant_reload_on_headshot`.
    pub instant_reload_on_kill: Option<f64>,
    /// THIS FORM CANNOT AIM DOWN SIGHTS — see
    /// [`crate::weapons_data::WeaponSpec::cannot_zoom`]. `resolve_for` answers
    /// the aim question FALSE for it whatever the scenario says, so every
    /// `while_aiming` mod, arcane and evolution pays nothing here.
    pub cannot_zoom: bool,
    /// RESONANT RESTORE: `(per stack, max stacks)` — "On Reload From Empty:
    /// Increase Base Magazine Capacity by +N. Stacks up to Nx", in the card's
    /// own units so `resolve` can scale it: the card says BASE capacity, which
    /// is the number a magazine mod multiplies.
    pub mag_growth_on_empty_reload: Option<(f64, u32)>,
    /// King's Gambit's weak-point half, held on the WEAPON so it can seed the
    /// same bucket the mods write to — "Weakpoint modifier is additive with
    /// mods such as Pistol Gambit". Same shape as `evo_reload_bonus`.
    pub evo_weakpoint_cc_rel: f64,
    /// Double Tap: `(per stack, max stacks, seconds)`. See
    /// [`ModEffect::ConsecutiveHitDamage`].
    pub consecutive_hit_damage: Option<(f64, u32, f64)>,
    /// Wiseman's Regard: `(rate, cap)` — "Increase Base Status Chance by 30% of
    /// current Critical Chance, up to 40%".
    ///
    /// "CURRENT" is the MODDED value, and the wiki's own arithmetic proves it:
    /// the Dera's mirror perk notes that "+366.7% Status Chance is needed to max
    /// out the Critical Chance bonus", and 0.30 x (1 + 3.667) is exactly the
    /// 1.40 that a 35% cap at 25% a point demands. "BASE" is where the grant
    /// LANDS, so the stat's own mods multiply it afterwards.
    pub base_status_from_crit: Option<(f64, f64)>,
    /// High Ground: the mirror — base crit chance from current status chance.
    pub base_crit_from_status: Option<(f64, f64)>,
    /// THE SECOND perk to ask about the player's sprint speed, and the second
    /// grant to be carried rather than spent in `apply`: Deadly Pace's "With
    /// Sprint Speed 1.2 or Higher: +80% Fire Rate".
    ///

    /// Feigned Retreat / Swift Conclusion: a share of the BASE-DAMAGE BUCKET
    /// that applies only while the target is under half health.
    ///
    /// VERBATIM (wiki, Sicarus Incarnon Genesis): *"Bonus damage is additive
    /// with mods such as Hornet Strike but does not take into account the Base
    /// Damage increase from this perk."* Both halves of that sentence are
    /// obeyed: it joins the bucket Serration feeds, and the rate stored here is
    /// already scaled so that the perk's OWN flat base damage is excluded from
    /// what it multiplies — resolved at the end of `apply`, which is the first
    /// moment the evolved base exists.
    pub bd_below_half_health: f64,
    /// Vicious Promise: "+40% Base Critical Chance / +2x Base Critical Damage
    /// Multiplier ON UNDAMAGED ENEMIES".
    ///
    /// A condition on the TARGET like the one above, and the first that asks
    /// whether the fight has started rather than how far it has got. Both are
    /// BASE grants, so `resolve` converts each into the post-mod number worth
    /// the same — `flat x (1 + mods)` — for the same reason the flat
    /// base-damage buff does: the panel resolves crit once, and a live grant
    /// has to land in the bracket the card names.
    pub cc_on_undamaged: f64,
    /// The other half of the same card. See above.
    pub cd_on_undamaged: f64,
    /// This weapon's Condition Overload behavior class.
    pub co_behavior: CoBehavior,
    /// CO base effectiveness = `original_base / evolved_base`, i.e. how much of
    /// the CO term the weapon's own evolutions dilute.
    ///
    /// **1.0 on every weapon but Dual Toxocyst.** Including a perk's flat base
    /// THE ORIGINAL BASE — the damage the GunCO term computes on, in the same
    /// units as `base_vector.total()`.
    ///
    /// AN ABSOLUTE, NOT A FRACTION (owner, 2026-08-16). It was
    /// `co_base_fraction`, a ratio recomputed as `original / evolved` wherever
    /// something raised the panel, and the ratio was the wrong noun: it
    /// described the ARITHMETIC of one particular loadout instead of the FACT
    /// underneath, which is that a weapon has an original base and some things
    /// add to it while others only add to what it prints.
    ///
    /// What the fraction could not express, and this can:
    ///
    ///   · TWO SOURCES THAT DISAGREE. A weapon carrying two flat-damage perks,
    ///     one feeding the term and one not, has no single ratio — the catalog
    ///     says the Despair is exactly that (one tier-2 option excluded, the
    ///     other not) and it only worked because nobody equips both.
    ///   · A NEW MECHANIC. Anything that raises base damage says whether it
    ///     feeds this, and the GunCO code does not change. Under the ratio,
    ///     a new source meant recomputing `original / evolved` at a new site,
    ///     which is the shape that keeps producing the same bug.
    ///
    /// A weapon may DECLARE a starting value below its own base
    /// (`co_base_fraction` in the yaml, 0.5 on a bow's charged entry); that is
    /// the only place a fraction is still written down, because that is how the
    /// catalog prints it.
    pub co_base: f64,
    /// Buff-injected elements as RELATIVE bonuses (element, bonus): each
    /// contributes ModifiedBase × bonus at the END of the hierarchy
    /// (rule 8) — Frenzy's +100% Toxin on the base Dual Toxocyst.
    pub injected_elements: Vec<(DamageType, f64)>,
    /// Weapon traits a mod's `requires` is checked against (e.g. `semi_auto`,
    /// `beam`). A mod requiring a trait the weapon lacks is inert.
    pub traits: &'static [&'static str],
    /// Incarnon-form transformation economy. `Some` marks this form's
    /// magazine as CHARGE-BACKED (a fixed "Max Charges" resource fed by the
    /// weakpoint gauge, entirely outside the ammo system): magazine mods and
    /// ammo efficiency are INERT on it. There is no reload; instead two
    /// transition times (transmute in = the base form's reload; transmute
    /// out = the officially-unnamed revert), each scaled by the reload
    /// formula `base / (1 + reload bonus)`. `magazine_size` / `base_reload`
    /// still carry the pseudo-reload (270 / 3.35) the plain sim consumes.
    pub gauge_form: Option<GaugeForm>,
    /// Evolution-granted additive fire rate (Rapid Wrath) — joins the
    /// fire-rate-mod bucket.
    pub evo_fire_rate_bonus: f64,
    /// Reload-speed bonus from evolutions, into the same bucket the mods feed.
    pub evo_reload_bonus: f64,
    /// READY RETALIATION's window — see
    /// [`crate::evolutions_data::EvoEffect::ReloadSpeedOnEmptyReload`]. Same
    /// bucket as `evo_reload_bonus`, but only while the window is open.
    /// READY RETALIATION: *"On Reload From Empty: +100% Reload Speed"*, as a
    /// plain bonus rather than a timed buff.
    ///
    /// IT IS SCOPED TO THE RELOAD ACTION — it arrives when the reload starts
    /// and is gone when the reload ends (owner, 2026-08-11). So it cannot
    /// lapse halfway through, and it cannot spill onto anything that is
    /// not that reload.
    ///
    /// That is also what makes the perk loadable at all on the other eleven
    /// weapons that have it. Only the Phenmor's page publishes a window (6 s),
    /// and the rest state the bonus and nothing else — which read as missing
    /// data while the model was a timer, and reads as "there is nothing to
    /// state" once the window is the action.
    pub rs_on_empty_reload: f64,
    /// FLENSING SPIKES: *"Remove 20% of enemy Armor per Puncture Status"*, as a
    /// fraction per live Weakened stack (0.0 = the weapon does not have it).
    ///
    /// A THIRD ARMOUR-STRIP SOURCE. The engine had two — Corrosive and Heat,
    /// the two the game itself strips with — and this is a weapon PERK doing it
    /// off a status that strips nothing on its own. It multiplies with the
    /// other two the same way they multiply with each other, and at Puncture's
    /// five-stack cap 20% a stack is the whole of the armour.
    pub armor_strip_per_puncture: f64,
    /// EXECUTIONER'S FORTUNE — see [`InstantReload`].
    pub instant_reload_on_headshot: Option<InstantReload>,
    /// LINGERING JUDGEMENT — see [`HeadshotStreak`].
    pub headshot_streak: Option<HeadshotStreak>,
    /// SPITEFUL DEFILEMENT: `(threshold, bonus)` — add `bonus` to the crit
    /// DAMAGE, after mods and flat, while the target carries fewer than
    /// `threshold` distinct status types.
    pub cd_below_status_count: Option<(u32, f64)>,
    /// Prelude of Might: `(bonus, threshold)` — add `bonus` to the crit damage
    /// MULTIPLIER while the resolved crit chance stays under `threshold`.
    /// Resolved late for that reason: it is the only evolution whose condition
    /// reads the panel rather than the fight.
    pub crit_mult_below_cc: Option<(f64, f64)>,
    /// FLAT crit/status chance added AFTER mods (Elemental Excess) — a
    /// different layer from the base-stat one `base_crit_chance` carries.
    pub post_mod_crit_chance: f64,
    pub post_mod_status_chance: f64,
    /// Additive headshot-damage bonus (Caput Mortuum), inside the headshot
    /// bracket `(1 + Σ)`. Direct hits only — a radial never headshots.
    pub headshot_damage_bonus: f64,
    /// Devouring Attrition: `(chance, bonus)` — on an instance that did
    /// NOT crit, `chance` to multiply it by `(1 + bonus)`. Its own
    /// multiplier, applied to the direct hit and the radial alike.
    pub noncrit_bonus: Option<(f64, f64)>,
    /// Overwhelming Attrition's stacking damage buff.
/// Every stacking buff this weapon grants — see [`StackingBuff`]. A Vec
    /// rather than one field per buff, so the roster, the config reader and the
    /// stack sampler can each walk it instead of naming buffs one at a time.
    pub stacking_buffs: Vec<StackingBuff>,
    /// A RADIAL (AoE) attack part fired alongside the direct hit — the
    /// Laetum Incarnon's 300 Radiation explosion. Separate damage vector,
    /// crit and status stats; the directly-hit enemy takes both parts.
    /// See MECHANICS §7 "Radial (AoE) attack parts" for the rule set.
    pub radial: Option<RadialBase>,
    /// THE CONE this attack fires into, as the data states it and before
    /// accuracy mods — see [`crate::weapons_data::SpreadSpec`]. `None` = not
    /// transcribed, and the entry admits it.
    pub spread: Option<crate::weapons_data::SpreadSpec>,
    /// DIRECT-hit damage falloff as the weapon data states it, unscaled — see
    /// [`Falloff`], which is this after Projectile Speed has moved the window.
    pub falloff: Option<crate::weapons_data::FalloffSpec>,
    /// This attack's row in Primary Compression's per-weapon table — see
    /// [`crate::weapons_data::CompressionSpec`] and docs/CATALOGS.md §2. `None`
    /// means the weapon has no AoE for the arcane to compress, so it is worth
    /// nothing rather than unknown (the catalog rule).
    pub compression: Option<crate::weapons_data::CompressionSpec>,
    /// A LINGERING FIELD left by every landed projectile of this attack — the
    /// Torid's Toxin cloud. Grenades STICK, so a directly-hit enemy takes the
    /// impact AND every tick. MECHANICS §7 "Lingering damage FIELDS".
    pub lingering: Option<LingeringBase>,
    /// CONTINUOUS (beam) weapon — trigger "Held". Two rules change, both wiki:
    /// `fire_rate` is TICKS per second, and multishot beams hitting one target
    /// MERGE into a single instance.
    pub continuous: bool,
    /// Renewed Horror: the multiplier a reload-from-EMPTY applies to the next
    /// shot's field duration (1.0 = the evolution is not installed).
    pub field_duration_on_empty_reload: f64,
    /// Lone Enforcer: `(fraction of base multishot, metres)`, paid only when
    /// the target is standing further away than that. Settled against the arena
    /// in `DummyParams::from_panel` — see [`EvoEffect::MultishotBeyondRange`].
    pub multishot_beyond_range: Option<(f64, f64)>,
    /// Continuous-beam geometry, when this form is one.
    pub beam: Option<BeamGeometry>,
    /// Final Fusillade: a FLAT multishot add on the LAST round of the magazine
    /// (0.0 = not installed). Base form only — the evolution loader drops it on
    /// a charge-backed form, so this is always 0.0 there.
    pub multishot_on_last_round: f64,
    /// The same window, in the OTHER BRACKET: "+5 **Base** Multishot on final
    /// magazine burst", which the wiki notes is "added before mods, and is
    /// thus multiplied by multishot bonuses". So it raises what the weapon's
    /// base pellet count IS for that burst, and every relative bonus — the mod
    /// bucket, a Galvanized stack, an arcane's grant — reads the raised number.
    pub base_multishot_on_last_round: f64,
    /// Plentiful Mayhem: multishot spends ammo, and what it GENERATES deals
    /// +v damage (0.0 = not installed). Both forms carry it; the rule differs
    /// by form and the sim reads that off `continuous`.
    pub multishot_ammo_bonus: f64,
}

/// WHAT TRIGGERS A STACKING BUFF. One arm per trigger, forever — a new buff
/// that fires on an event already listed here costs no engine code at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuffTrigger {
    /// Overwhelming Attrition: a hit that neither crits nor applies a status.
    PlainHit,
    /// Lethal Rearmament, Headcracker: any weak-point hit.
    Headshot,
    /// Stormburst: a hit on a target that ALREADY carries this status. The
    /// condition is on the TARGET, not on the shot — which is why it could not
    /// be expressed as a static `AssumedMaxMultishot` (that would grant the
    /// buff to a build with no Electricity in it at all) and can be expressed
    /// here: a live buff is bumped inside the fight, where the target's
    /// debuffs are in hand.
    HitEnemyWithStatus(crate::damage::DamageType),
    /// Mounting Momentum: a completed RELOAD, not a shot. The first trigger in
    /// this vocabulary that is not something the weapon does to a target — and
    /// the first that grants more than one stack at a time, because what it
    /// grants is one per SHELL loaded (see [`StackingBuff::stacks_per_trigger`]).
    ReloadComplete,
    /// Fresh Havoc, Mauler's Magazine: "On Reload From Empty".
    ///
    /// IN THIS ARENA THAT IS ALMOST — not quite — every reload. The loop only
    /// reloads when it cannot fire, so both reload sites are from-empty by
    /// construction and this trigger would be [`BuffTrigger::ReloadComplete`]
    /// under another name. The exception is what earns it a variant: entering
    /// the Incarnon form FULLY RELOADS the base magazine whether or not it was
    /// empty, and the Soma's card says "Switching to Incarnon Form from empty
    /// will ALSO trigger the buff" — so the transform pays this only when the
    /// base magazine had run out, where `ReloadComplete` would pay every cycle.
    ReloadFromEmpty,
    /// Reaver's Rapture: a COMPLETED BURST, every round of it landing.
    ///
    /// THE MOMENT IS THE LAST ROUND OF THE BURST, so the burst that earns the
    /// stack does not carry it — the next one does. The wiki's own qualifiers
    /// all point the same way and all of them are already true here: "not
    /// affected by multishot or punch through" (it is one count per burst, not
    /// per pellet), "counts object hits", and "activates even if the first hit
    /// of a burst kills the target" — this arena has one target that respawns,
    /// every round hits it, so every completed burst is a full burst hit.
    ///
    /// A magazine that does not divide by the burst count leaves a partial
    /// burst at the end, and a partial burst is not one: the count restarts
    /// with the magazine, so those rounds earn nothing.
    FullBurst,
    /// Crimson Overture: A KILL, wherever it came from.
    ///
    /// Counted off `RunResult::kills` rather than bumped at the six places a
    /// kill can happen (a direct hit, a DoT tick, a field tick, …), because
    /// "remember to also bump it here" is how five of six get done. The loop
    /// already reads kills this way for Sentient Surge's refill and the Ocucur's
    /// tendrils; this is the same mark-and-diff.
    ///
    /// A consequence that matches every other trigger here: the kill is seen at
    /// the START of the next shot, so the shot that earned the stack does not
    /// carry it.
    Kill,
    /// Blazing Barrel: FIRING — the round leaving the barrel, whether or not
    /// it hits and whatever it hits.
    ///
    /// The first trigger here that asks nothing of the target at all, which is
    /// why it is counted where the round is SPENT rather than in the pellet
    /// loop: one shot is one stack however many pellets it threw, and a
    /// shotgun is the family this perk is on.
    ///
    /// THE SHOT THAT EARNS THE STACK DOES NOT CARRY IT — its multishot was
    /// rolled before the round was spent. Same moment Reaver's Rapture uses,
    /// for the same reason.
    Firing,
    /// Paragon Essence: a STATUS EFFECT landing on the target, of any type.
    ///
    /// Distinct from [`BuffTrigger::HitEnemyWithStatus`], which asks what the
    /// target is ALREADY carrying: this one fires on the proc itself, so a
    /// build that lands nothing never earns it however long the fight runs.
    StatusApplied,
    /// Striking Succession: ANY hit — a pellet reaching the target, crit or
    /// not, status or not.
    ///
    /// The permissive sibling of [`BuffTrigger::PlainHit`], which fires only on
    /// a hit that did NEITHER. Two cards, two sentences, and reading one as the
    /// other is worth several stacks a second on a high-crit build.
    Hit,
    /// Well Rehearsed: CONSECUTIVE weak-point hits — the only trigger here that
    /// can be UNDONE by the next shot.
    ///
    /// VERBATIM (wiki, Sybaris Incarnon Genesis): *"The stack resets after
    /// reloading … It also resets after bodyshots"*. So a body hit takes the
    /// whole pile, which is why this cannot be `Headshot` with a clock: the
    /// pile's life is decided by what you hit next, not by time.
    ///
    /// With a headshot rate below 100% the cap is a real target rather than a
    /// given — three in a row at 50% is one run in eight.
    ConsecutiveHeadshot,
}

/// WHAT A STACKING BUFF FEEDS. One arm per grant, and each keeps its own
/// bracket — that is the part which cannot be generalised and must not be:
/// fire rate is additive on the BASE rate, reload speed scales the reload, and
/// base damage joins the damage bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuffGrant {
    BaseDamage,
    ReloadSpeed,
    FireRate,
    /// Stormburst: "+0.4 Multishot", a FLAT add rather than a percentage of
    /// the weapon's base — so it joins `ms_eff` beside Final Fusillade's, not
    /// the multishot BUCKET.
    Multishot,
    /// Blazing Barrel on the Strun family: "+0.05 **Base** Multishot".
    ///
    /// "Base" is the whole difference and the wiki spells out what it buys on
    /// the neighbouring perk (Forceful Finality, "+5 BASE Multishot"): it is
    /// "added before mods, and is thus multiplied by multishot bonuses". So a
    /// build carrying Hell's Chamber gets 0.05 x that bucket a stack, where
    /// [`BuffGrant::Multishot`] would have given it a flat 0.05.
    BaseMultishot,
    /// Blazing Barrel on the Sybaris and the Stug: "+5% Multishot" — a
    /// PERCENTAGE of the weapon's base, which is what every multishot MOD
    /// grants, so it joins their bucket rather than either flat bracket.
    ///
    /// Three brackets for one stat reads like over-modelling until the cards
    /// are laid side by side: the same perk NAME grants a flat base add on one
    /// family and a percentage on another, and they are different numbers on
    /// any build that carries a multishot mod.
    MultishotPercent,
    /// Striking Succession: *"Increase Base Damage by +15"* — an ABSOLUTE add
    /// to the weapon's base, not a share of the base-damage bucket.
    ///
    /// The difference is Serration: a bucket bonus is diluted by every other
    /// bonus in that bucket and a base add is not, because it raises the number
    /// the bucket multiplies. They are the same only on an unmodded weapon.
    ///
    /// `per_stack` therefore CHANGES UNITS at `resolve`, the way
    /// [`BuffGrant::FireRate`]'s does: the flat number on [`WeaponBase`], and
    /// on [`ResolvedPanel`] the share of the bucket that is worth exactly the
    /// same — `flat * (1 + bd) / base`, which the mods make a constant. That
    /// keeps ONE live-base-damage path in the sim instead of a second bracket
    /// that would have to be kept in step with it.
    FlatBaseDamage,
    /// Mauler's Magazine: *"Increase Base Critical Damage Multiplier by +1x"* —
    /// the BASE multiplier, so the crit-damage MODS multiply the grant, exactly
    /// as they multiply Prelude of Might's.
    ///
    /// `per_stack` therefore changes units at `resolve` the way
    /// [`BuffGrant::FlatBaseDamage`]'s does — `+1x` leaves as `1 * (1 + cd)`,
    /// the post-mod multiplier it is worth — which keeps ONE crit-damage sum in
    /// the sim rather than a live bracket that would have to be kept in step
    /// with the static one.
    BaseCritDamage,
    /// Sequential Skullbuster: *"On Consecutive Weakpoint Hits: +30% Headshot
    /// Damage"*. Joins the ADDITIVE headshot bracket — the same `(1 + Σ)` that
    /// Primary Deadhead and Lingering Judgement land in, since the wiki lists
    /// every innate headshot source there but Cernos Prime's.
    HeadshotDamage,
}

impl BuffGrant {
    /// The `disables:` key this grant feeds — the SAME vocabulary a locking
    /// mod writes ("multishot", "fire_rate"). Derived rather than listed: a
    /// lock says "set to its default ignoring other bonuses, even negative
    /// effects" (MEASUREMENTS M30), and a live buff is a bonus like any other,
    /// so the stat is the only thing the two have to agree on. Adding a grant
    /// without answering this is a compile error, which is the point — the
    /// FireRate arm was once the only one that knew about locks, and Stormburst
    /// went on paying +1.2 multishot under Secondary Acuity because Multishot
    /// had simply never been added beside it (owner, 2026-08-11).
    pub fn locked_stat(self) -> &'static str {
        match self {
            BuffGrant::BaseDamage | BuffGrant::FlatBaseDamage => "base_damage",
            BuffGrant::BaseMultishot | BuffGrant::MultishotPercent => "multishot",
            BuffGrant::ReloadSpeed => "reload_speed",
            BuffGrant::FireRate => "fire_rate",
            BuffGrant::Multishot => "multishot",
            BuffGrant::BaseCritDamage => "crit_damage",
            BuffGrant::HeadshotDamage => "headshot_damage",
        }
    }
}

/// HOW A STACK LEAVES. `docs/BUFFS.md` has named these three since the buff
/// vocabulary was written; two were implemented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuffDecay {
    /// The Galvanized family: on timeout ONE stack drops and the timer
    /// RESTARTS, so any new stack refreshes the whole pile. One hit per
    /// window holds every stack.
    LoseOneAndReset,
    /// Each stack carries its OWN clock and expires on it, oldest first —
    /// FIFO. Strictly harsher: holding N stacks needs N hits per window, not
    /// one. Stormburst is the roster's first (owner, 2026-08-07).
    PerStackExpiry,
}

/// ONE STACKING BUFF, and one place its identity is written.
///
/// It replaced three structs with identical fields — `PlainHitBuff`,
/// `HeadshotReloadBuff`, `HeadshotFireRateBuff` — that differed only in what
/// triggered them and what they fed. The duplication was not the real cost:
/// each buff's IDENTITY had to be repeated in four places (the evolution's
/// buff card, the sim's replay roster, the config reader, the stack sampler),
/// those four could disagree, and one of them is a control the player sees.
/// Headcracker shipped with the card and none of the other three, so the panel
/// offered a stacks/lock control that did nothing.
///
/// The shape is not invented: `ArcBuffSpec` already carries exactly this
/// (owner + trigger + grant + the four numbers) in a `Vec`, and `ArcRuntime`
/// already bumps by trigger and totals by grant. This is that pattern, applied
/// to the buffs a WEAPON grants.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StackingBuff {
    /// The buff-card id — the SINGLE source of this buff's identity. The
    /// roster, the config and the sampler all key on it, so they cannot drift.
    pub id: &'static str,
    pub trigger: BuffTrigger,
    pub grant: BuffGrant,
    /// Per stack. For [`BuffGrant::FireRate`] this is a FRACTION of the base
    /// rate on [`WeaponBase`] and the ABSOLUTE rate it is worth on
    /// [`ResolvedPanel`] — `resolve` converts it, because the sim adds it
    /// beside `fire_rate` inside the bracket fire-rate mods live in and
    /// carries no unmodded rate to re-derive it from.
    pub per_stack: f64,
    pub max_stacks: u32,
    pub duration: f64,
    /// Rolled per trigger. 1.0 unless the perk says otherwise — Headcracker's
    /// "This effect has a 50% chance of activating" is the only 0.5 so far.
    pub chance: f64,
    /// See [`BuffDecay`]. Defaults to the Galvanized family because that is
    /// what every buff here did before the third one was implemented.
    pub decay: BuffDecay,
    /// Stacks at t = 0 — the buff card's other knob, the first being
    /// `duration` ([`NO_TIMEOUT`] when it is locked).
    pub initial_stacks: u32,
    /// HOW MANY STACKS ONE TRIGGER GRANTS. One, for every buff written before
    /// Mounting Momentum — and that perk grants one per SHELL LOADED, so the
    /// count is a property of the weapon (its modded magazine) rather than of
    /// the card.
    ///
    /// It is resolved the same way `per_stack` is: `WeaponBase` carries the
    /// RULE (0 = "one per shell") and `resolve` turns it into the number,
    /// because the modded magazine does not exist until the mods are in. That
    /// is also what makes the trade-off real — a magazine mod buys stacks and
    /// pays for them in reload time (`by_round_reload`).
    pub stacks_per_trigger: u32,
    /// Does this buff count SHELLS rather than reloads?
    ///
    /// The same fact `stacks_per_trigger: 0` states on [`WeaponBase`], kept
    /// after `resolve` has turned it into a number — because by then "13" and
    /// "one per shell" are indistinguishable, and the Incarnon route needs the
    /// difference. Entering the form is one reload that loads several shells,
    /// so a shell-counting buff gets one per shell and a reload-counting buff
    /// gets one, and nothing else can tell them apart.
    pub per_shell: bool,
    /// AN EVENT THAT TAKES THE WHOLE PILE, for a buff that has no clock.
    ///
    /// Mounting Momentum is cleared the instant the magazine reaches zero —
    /// not when the reload finishes, and not on a timer (owner,
    /// 2026-08-08). It changes what the perk IS: firing a magazine dry and
    /// reloading it earns one magazine's worth and no more, and the only
    /// way to the 99-stack cap is to keep topping up a magazine that never
    /// empties.
    pub cleared_by: ClearedBy,
}

/// See [`StackingBuff::cleared_by`]. A buff with a duration needs none of
/// this — its clock is what ends it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClearedBy {
    /// Only its own clock, which is every buff written before this one.
    #[default]
    Nothing,
    /// The magazine reaching zero.
    EmptyMagazine,
    /// THE MAGAZINE BEING REFILLED — a reload completing, or either Incarnon
    /// transform completing, because swapping either way fully reloads the base
    /// form's magazine (wiki).
    ///
    /// One rule rather than a list of events, and the same one Ready
    /// Retaliation is spent by. Reaver's Rapture states it as three separate
    /// sentences — "resets on Reload", "resets when activating incarnon" — and
    /// they are one fact.
    ///
    /// THE MOMENT IS THE COMPLETION, not the start: a reload that has begun has
    /// not refilled anything yet.
    MagazineRefilled,
    /// A RELOAD — the action, not the refill, and the difference is one event.
    ///
    /// VERBATIM (wiki, Strun Incarnon Genesis, Blazing Barrel): *"resets
    /// entirely upon reloading. Entering Incarnon Form counts as reloading but
    /// exiting does not."* Swapping OUT refills the base form's magazine, so a
    /// buff keyed on the refill dies there — and this one is stated not to.
    ///
    /// Kept as a second variant rather than folded into the one above because
    /// the two disagree on exactly one of the four events that end a magazine,
    /// and picking either as "close enough" is a stack count nobody can
    /// reproduce.
    Reload,
}

/// What happens when a second field lands on a target that already has one.
///
/// Weapon DATA, not a global rule — the Torid STACKS (✅ measured, MEASUREMENTS
/// M13) but a future weapon may refresh, and the answer is worth up to ~5x
/// sustained single-target DPS on a 5-round magazine, so it is not a constant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FieldStacking {
    /// N concurrent tick streams, one per grenade — what the Torid does.
    #[default]
    Stack,
    /// One field, re-armed: a second grenade resets duration instead of adding
    /// a stream.
    Refresh,
}

/// A weapon's LINGERING FIELD attack part — an area that persists and TICKS
/// rather than landing once (Torid's Toxin cloud), unmodded. MECHANICS §7.
#[derive(Debug, Clone)]
pub struct LingeringBase {
    pub base_vector: DamageVector,
    pub base_crit_chance: f64,
    pub base_crit_damage: f64,
    pub base_status_chance: f64,
    /// Ticks per second (the data module's `FireRate` for the part: Torid 1).
    pub tick_rate: f64,
    /// How long the field lives (`EffectDuration`: Torid 10 s).
    pub duration_s: f64,
    pub radius_m: f64,
    pub falloff_start_m: f64,
    /// Torid's cloud is `reduction 1.0` — damage falls to ZERO at the rim,
    /// unlike the Laetum radial's 0.2.
    pub falloff_reduction: f64,
    pub stacking: FieldStacking,
    /// Does this field take Condition Overload? **Default NO** — the normal
    /// rule is what the mods say on the tin: CO boosts DIRECT hits only, and an
    /// AoE part should get nothing. The Torid's cloud is an ANOMALY that the CO
    /// catalog records with a row of its own (user, 2026-07-30: "in theory it
    /// would not get it, but the programmer let it"), so the weapon declares it
    /// rather than the engine assuming every field behaves that way.
    pub takes_condition_overload: bool,
}

/// The lingering field after mod resolution.
#[derive(Debug, Clone, Copy, Default)]
pub struct ResolvedLingering {
    pub damage: DamageVector,
    pub modified_base: f64,
    pub crit_chance: f64,
    pub crit_damage: f64,
    pub status_chance: f64,
    /// The field's own UNMODDED stats — the bases its RELATIVE live buffs
    /// multiply (same rule as the radial: a bucket scales whichever base it is
    /// applied to). Torid's cloud is 15% / 2.0x / 25%, none of which match its
    /// grenade impact's 15% / 2.0x / 23%.
    pub base_crit_chance: f64,
    pub base_crit_damage: f64,
    pub base_status_chance: f64,
    pub tick_rate: f64,
    pub duration_s: f64,
    /// Geometry, carried through unmodded — single-target stands at the
    /// epicentre, but the panel states it (and Firestorm enlarges it in game).
    pub radius_m: f64,
    pub falloff_start_m: f64,
    pub falloff_reduction: f64,
    pub stacking: FieldStacking,
    /// See [`LingeringBase::takes_condition_overload`] — CO on an AoE part is
    /// the exception, not the default.
    pub takes_condition_overload: bool,
}

impl ResolvedLingering {
    /// WHAT A BODY `d` METRES FROM THE CLOUD'S CENTRE TAKES, as a fraction —
    /// the same shape as [`ResolvedRadial::falloff_at`], because a cloud falls
    /// off the same way an explosion does and nothing about it being persistent
    /// changes that.
    ///
    /// Nothing outside the cloud, and the full amount before the window opens.
    pub fn falloff_at(&self, d: f64) -> f64 {
        if d >= self.radius_m {
            return 0.0;
        }
        if d <= self.falloff_start_m {
            return 1.0;
        }
        let span = self.radius_m - self.falloff_start_m;
        if span <= 0.0 {
            return 1.0;
        }
        (1.0 - self.falloff_reduction * ((d - self.falloff_start_m) / span)).max(0.0)
    }
}

/// A weapon's radial (explosion) attack part, unmodded.
#[derive(Debug, Clone)]
pub struct RadialBase {
    pub base_vector: DamageVector,
    pub base_crit_chance: f64,
    pub base_crit_damage: f64,
    pub base_status_chance: f64,
    /// Blast radius = the falloff `end` distance.
    pub radius_m: f64,
    /// See [`crate::weapons_data::RadialSpec::takes_blast_radius_mods`].
    pub takes_blast_radius_mods: bool,
    /// Linear falloff window and the fraction of damage REMOVED at max
    /// distance: `mult(d) = 1 − reduction × clamp((d−start)/(end−start))`.
    /// Only bites once the sim has targets away from the epicentre.
    pub falloff_start_m: f64,
    pub falloff_reduction: f64,
    /// See [`crate::weapons_data::RadialSpec::forced_procs`] — the EXPLOSION's
    /// own, which is not the direct part's.
    pub forced_procs: crate::damage::ForcedProcs,
    /// Does this explosion take Condition Overload? **Default NO** — the mods
    /// say CO boosts DIRECT hits, so an AoE part is not supposed to receive it
    /// at all. Some entries do anyway, and the CO catalog lists them one at a
    /// time: the Zylok's Incarnon radial has a row reading "Radial hit only
    /// receives CO bonus on target directly hit by bullet", which the sim's
    /// single-target arena always is. Declared per weapon because it is a
    /// per-entry quirk, never a rule (MECHANICS §6).
    pub takes_condition_overload: bool,
    /// See [`crate::weapons_data::RadialSpec::takes_multishot`].
    pub takes_multishot: bool,
    /// THE ORIGINAL BASE of this explosion — the radial's own [`WeaponBase::co_base`],
    /// and it needs its own because an evolution can raise what the explosion
    /// DEALS without raising what its CO term reads.
    pub co_base: f64,
}

impl RadialBase {
    /// See [`WeaponBase::co_base_fraction`] — derived, never stored.
    pub fn co_base_fraction(&self) -> f64 {
        let total = self.base_vector.total();
        if total <= 0.0 {
            return 1.0;
        }
        self.co_base / total
    }
}

/// The radial part after mod resolution.
#[derive(Debug, Clone, Copy, Default)]
pub struct ResolvedRadial {
    pub damage: DamageVector,
    pub modified_base: f64,
    pub crit_chance: f64,
    pub crit_damage: f64,
    /// The explosion's UNMODDED crit stats — the bases a RELATIVE live crit
    /// buff multiplies. A weapon may give its explosion different crit stats
    /// from its direct hit (Laetum Incarnon happens to use 22%/2.2x for both),
    /// which is why those bonuses stay relative until the sim knows which
    /// stage it is resolving.
    pub base_crit_chance: f64,
    pub base_crit_damage: f64,
    pub status_chance: f64,
    /// The explosion's UNMODDED status chance — the base a RELATIVE live
    /// status-chance buff (Primary Crux) multiplies. It differs from the
    /// direct hit's, which is why the arcane grant stays relative until the
    /// sim knows which attack part it is resolving.
    pub base_status_chance: f64,
    /// Blast geometry, carried through unmodded — the sim's single target
    /// stands at the epicentre, but the PANEL states it: a reader needs the
    /// radius to know what the explosion is worth beyond one enemy.
    pub radius_m: f64,
    pub falloff_start_m: f64,
    pub falloff_reduction: f64,
    /// See [`RadialBase::forced_procs`].
    pub forced_procs: crate::damage::ForcedProcs,
    /// See [`RadialBase::takes_condition_overload`] — CO on an explosion is the
    /// exception, not the default.
    pub takes_condition_overload: bool,
    /// See [`RadialBase::takes_multishot`].
    pub takes_multishot: bool,
    /// See [`RadialBase::co_base_fraction`].
    pub co_base_fraction: f64,
}

impl ResolvedRadial {
    /// What a body `d` metres from the EPICENTRE takes, as a fraction.
    ///
    /// Full inside `falloff_start_m`, decaying linearly to
    /// `1 − falloff_reduction` at the rim, and NOTHING past the radius — the
    /// blast radius IS the falloff's end distance, so the two are one number.
    ///
    /// `falloff_reduction` is the amount REMOVED (the Laetum's 0.2 leaves 80%
    /// at the rim), which reads the opposite way to [`Falloff::keep`]; both are
    /// kept as their sources state them rather than normalised into a spelling
    /// that would make one of them a lie about its source.
    pub fn falloff_at(&self, d: f64) -> f64 {
        if d >= self.radius_m {
            return 0.0;
        }
        if d <= self.falloff_start_m {
            return 1.0;
        }
        let span = self.radius_m - self.falloff_start_m;
        if span <= 0.0 {
            return 1.0 - self.falloff_reduction;
        }
        1.0 - self.falloff_reduction * ((d - self.falloff_start_m) / span)
    }
}

/// THE CONE, resolved: degrees from the reticle, accuracy mods applied.
///
/// `min` is the first shot's and `max` is where sustained fire takes it — see
/// [`crate::weapons_data::SpreadSpec`], which is also where the bloom between
/// them is written down as unmodelled.
#[derive(Debug, Clone, Copy)]
pub struct Spread {
    pub min_deg: f64,
    pub max_deg: f64,
}

impl Spread {
    /// Can this attack miss at all? A weapon whose AIMED cone is zero cannot —
    /// the Torid's grenade is `0 / 0` and its page says "Pinpoint accuracy" in
    /// words, and every sniper in the roster is `0 / 15`: the first shot goes
    /// exactly where the reticle is, which is what a sniper IS.
    pub fn is_pinpoint(&self) -> bool {
        self.min_deg <= 0.0
    }

    /// The deviation ONE pellet drew, in degrees, from a uniform `u` in [0,1).
    ///
    /// UNIFORM INSIDE THE AIMED CONE, i.e. `[0, min_deg)`. Two readings of the
    /// wiki decide that and both are quoted at [`crate::weapons_data::
    /// SpreadSpec`]: spread is *"an angle in degrees from the reticle"*, so the
    /// stat is the cone's RADIUS and a shot lands somewhere inside it rather
    /// than on its rim; and `min` is named *"Deviation With Aim"*, which is the
    /// state this arena is permanently in (the rulers pin `aiming: true`).
    ///
    /// SO `max_deg` IS CARRIED AND NOT CONSUMED. It is where SUSTAINED FIRE
    /// takes the cone — *"the faster a weapon fires, the larger the size of the
    /// 'cone'"* — and the ramp between the two is published nowhere, so
    /// modelling it would mean inventing a bloom rate for 224 entries. What
    /// that costs is stated rather than hidden: a weapon held on the trigger
    /// is more accurate here than in game, most visibly on the ones whose
    /// window is widest (every sniper is `0 / 15`). docs/UNMODELLED.md §2.
    ///
    /// Drawing across `[min, max]` instead was tried first and is refutable
    /// from the data: it makes a Rubico — pinpoint on its first shot, in a
    /// weapon class defined by that — miss about half of them (2026-08-15).
    pub fn draw(&self, u: f64) -> f64 {
        self.min_deg * u
    }
}

/// DIRECT-hit damage falloff, resolved: full damage inside `start_m`, decaying
/// linearly to `keep` of it at `end_m` and flat beyond.
///
/// `keep` is DE's own `reduction` and it is the fraction KEPT — the Boar keeps
/// 0.5 past 25 m. It reads the opposite way to [`RadialBase::falloff_reduction`]
/// (the amount REMOVED), and both are kept as their sources state them; see
/// [`crate::weapons_data::FalloffSpec`].
///
/// THE WINDOW IS SCALED BY PROJECTILE SPEED, which is the first thing that
/// bucket has ever been worth. Wiki (`Projectile Speed`), verbatim: *"Mods
/// including Rivens that have positive or negative Projectile speeds will
/// affect a weapon's entire Damage Falloff range accordingly"* — and, from the
/// other side, *"Hitscan weapons that do not list Damage Falloff values in
/// their UI are completely unaffected by Projectile Speed modifications"*. So a
/// weapon without a falloff takes nothing from the stat, which is exactly this
/// struct being `None`.
#[derive(Debug, Clone, Copy)]
pub struct Falloff {
    pub start_m: f64,
    pub end_m: f64,
    /// Fraction of damage KEPT at `end_m` and beyond.
    pub keep: f64,
}

impl Falloff {
    /// The multiplier on a direct hit that travelled `d` metres.
    pub fn factor(&self, d: f64) -> f64 {
        if d <= self.start_m {
            return 1.0;
        }
        if d >= self.end_m || self.end_m <= self.start_m {
            return self.keep;
        }
        let t = (d - self.start_m) / (self.end_m - self.start_m);
        1.0 - (1.0 - self.keep) * t
    }
}

/// A continuous beam's GEOMETRY — shape, not a damage part. Carried so
/// Firestorm has a radius to scale and the multi-target model has its inputs;
/// the single-target arena reads none of it.
#[derive(Debug, Clone, Copy)]
pub struct BeamGeometry {
    pub range_m: f64,
    /// The impact sphere. Firestorm (Primed) enlarges it.
    pub damage_radius_m: f64,
    /// The sphere does NOT take multishot; only the direct target does.
    pub radius_takes_multishot: bool,
    pub chain_hops: u32,
    pub chain_range_m: f64,
    /// Each hop deals this fraction of the hop before it — or of the MAIN beam
    /// when `chain_compounds` is false, which is the Kuva Nukor's shape.
    pub chain_damage_per_hop: f64,
    pub chain_compounds: bool,
    pub chain_takes_multishot: bool,
    /// Does every chain NODE carry a sphere too? UNVERIFIED (MEASUREMENTS
    /// M15) — one line of weapon data so a measurement flips it.
    pub chain_nodes_have_radius: bool,
}

/// The Incarnon form's charge economy, for the panel's stat display (see
/// [`WeaponBase::incarnon`]). All times are UNMODDED bases.
#[derive(Debug, Clone, Copy)]
pub struct GaugeForm {
    /// Fixed charge capacity ("Max Charges") — magazine mods are inert.
    pub max_charges: f64,
    /// WHAT fills the gauge. Not cosmetic: the Zariman pistols count weak-point
    /// hits, the Torid counts plain direct hits — "Angstrum Incarnon Genesis
    /// and Torid Incarnon Genesis are instead charged through direct hits"
    /// (wiki Incarnon). Either way it is PER PELLET: "Individual Multishot
    /// bullets can build charges."
    pub charge_on: ChargeOn,
    /// Hits of `charge_on` needed to fill the gauge, UNMODIFIED by evolutions
    /// (Dual Toxocyst 9, Laetum 12, Torid 5). `charge_rate` below shortens it.
    pub charges_to_fill: f64,
    /// Transmute IN (enter the form) = the base form's reload time.
    pub transmute_in: f64,
    /// Transmute OUT (revert to the base form; officially unnamed) — an
    /// estimate, also shortened by reload-speed bonuses.
    pub transmute_out: f64,
    /// Extra gauge fill rate from evolutions (Incarnon Efficiency: +0.5).
    /// Weakpoint hits build `1 + charge_rate` times the charge, so the
    /// hits needed to fill the gauge divide by that factor.
    pub charge_rate: f64,
}

/// Which hits build an Incarnon gauge (weapon data, never assumed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChargeOn {
    /// Weak-point hits only — the Zariman weapons (Dual Toxocyst, Laetum).
    /// A radial/field instance can never contribute: it has no hit location.
    #[default]
    WeakpointHits,
    /// Any direct hit on an enemy (Torid: 5 fill it). Its Toxin cloud is NOT a
    /// direct hit and does not charge — the wiki says so outright ("Torid's
    /// poison cloud does not build charges") and a 37.0 patch note fixed the
    /// case where it did.
    DirectHits,
    /// Kills, not hits — the Mausolon and the Cortege, whose alt-fire is a
    /// thing you EARN rather than hold: "Getting 5 kills with the Mausolon's
    /// primary fire will unlock an Alternate Fire", and after firing it
    /// "additional kills are needed to recharge the laser" (wiki Mausolon;
    /// the Cortege page carries the same sentence).
    ///
    /// This is the first gauge whose source is not a hit, and it is why the
    /// count is a MARK rather than a per-shot delta: a hit lands inside the
    /// shot that caused it, a kill can land on a status tick between two.
    Kills,
}

/// The Evolution II choice — a SEARCH DIMENSION (user, 2026-07-25). A
impl WeaponBase {
    /// Apply an arbitrary equipped-evolution set (data ids) from
    /// data/evolutions/*.yaml onto the raw base. An EMPTY list = nothing
    /// installed at any choosable tier.
    fn apply_evolution_ids(mut self, evo_ids: &[&str]) -> Self {
        let evos: Vec<_> = evo_ids
            .iter()
            .map(|id| {
                crate::evolutions_data::get(id)
                    .unwrap_or_else(|| panic!("missing evolution yaml: {id}"))
            })
            .collect();
        crate::evolutions_data::apply(&mut self, &evos);
        self
    }

    /// Build a weapon FORM from its data entry: the raw yaml panel
    /// (`weapons_data::base_panel`) with an arbitrary equipped-evolution
    /// selection applied (data ids; empty = bare weapon). The engine knows
    /// no specific weapon — `id` is purely a data key.
    /// A FLAT BASE-DAMAGE ADD, folded the way an evolution's is.
    ///
    /// The base damage TOTAL rises by `flat` and the vector scales pro-rata, so
    /// the composition is untouched and every downstream reading of the base —
    /// status payloads included — follows. The EXPLOSION takes it too, and keeps
    /// multiplying its UNEVOLVED base for Condition Overload: the CO catalog's
    /// Burston radial row is what settles both halves ("Attack Damage 55 | CO
    /// Damage Bonus at +100% 13 | 24%", where 55 = 13 + the Genesis's only +42
    /// and 13/55 is the printed 24%).
    ///
    /// ONE IMPLEMENTATION, two callers: `evolutions_data::apply` for a plain
    /// flat perk and `resolve_for` for one the player's state gates. A gated
    /// "+40 with overshields" and an ungated "+40" are the same statement about
    /// the weapon, so they must not be able to come out as different panels —
    /// `a_gated_flat_base_damage_folds_exactly_as_an_ungated_one` is that
    /// assertion.
    /// WHAT FRACTION OF THE PANEL THE CO TERM READS — derived from
    /// [`Self::co_base`], never stored. The damage math wants a fraction of the
    /// evolved base; the FACT is the absolute, and deriving one from the other
    /// means they cannot disagree.
    pub fn co_base_fraction(&self) -> f64 {
        let total = self.base_vector.total();
        if total <= 0.0 {
            return 1.0;
        }
        self.co_base / total
    }

    /// Add flat base damage, and say how much of it the GunCO term's base
    /// grows by.
    ///
    /// `into_co` is USUALLY 0 or `flat` and is passed as an amount rather than
    /// a bool on purpose: a build carrying two flat-damage perks that disagree
    /// contributes part of its total, and a bool cannot say that.
    pub fn add_flat_base_damage(&mut self, flat: f64, into_co: f64) {
        let original_total = self.base_vector.total();
        if flat <= 0.0 || original_total <= 0.0 {
            return;
        }
        let evolved = original_total + flat;
        self.base_vector = self.base_vector.scale(evolved / original_total);
        self.co_base += into_co;
        if let Some(r) = self.radial.as_mut() {
            let rad_original = r.base_vector.total();
            if rad_original > 0.0 {
                // THE EXPLOSION TAKES THE SAME ABSOLUTE ADD, not a pro-rata
                // share of it.
                r.base_vector = r.base_vector.scale((rad_original + flat) / rad_original);
                // …AND ITS CO BASE DOES NOT GROW, EVER. That is the behaviour
                // this refactor preserved rather than chose: the old code set
                // the radial's fraction to `original / evolved` unconditionally
                // while the direct hit's followed the perk's flag, so the two
                // parts of one weapon could disagree about the same +42. They
                // agree for every `Adding` entry now that its default excludes,
                // and differ only on a `Multiplying` entry with an explosion,
                // where nothing has been measured either way. Written as
                // `+= 0.0` so the day a measurement arrives there is one line
                // to change and it is this one.
                r.co_base += 0.0;
            }
        }
    }

    pub fn from_data(id: &str, frenzy_active: bool, evo_ids: &[&str]) -> Self {
        crate::weapons_data::base_panel(id, frenzy_active).apply_evolution_ids(evo_ids)
    }

}

/// The resolved panel: everything the dummy sim needs from layers [1]+[2].
#[derive(Debug, Clone)]
pub struct ResolvedPanel {
    /// Post-hierarchy damage vector (physical × (1+bd) + combined elements).
    pub damage: DamageVector,
    /// The resolved radial (AoE) part, when the weapon has one.
    pub radial: Option<ResolvedRadial>,
    /// THE CONE, accuracy mods applied. A zero-width one lands on the reticle;
    /// `None` = this entry's spread is not transcribed, so no shot of it is
    /// allowed to miss and the entry admits that.
    pub spread: Option<Spread>,
    /// DIRECT-hit damage falloff, when this attack lists one — the shotgun's,
    /// and the range the Arsenal prints. `None` = full damage at any distance.
    pub falloff: Option<Falloff>,
    /// The resolved lingering FIELD, when the weapon leaves one.
    pub lingering: Option<ResolvedLingering>,
    /// CONTINUOUS (beam) weapon — see [`WeaponBase::continuous`].
    pub continuous: bool,
    /// Renewed Horror's field-duration multiplier on the shot after an empty
    /// reload (1.0 = none).
    pub field_duration_on_empty_reload: f64,
    /// Lone Enforcer, carried rather than folded: `(fraction of base multishot,
    /// metres)`. `resolve` cannot settle it because it never sees the arena —
    /// `DummyParams::from_panel` does, and that is where it is paid.
    pub multishot_beyond_range: Option<(f64, f64)>,
    /// Final Fusillade's flat multishot add on the magazine's last round
    /// (0.0 = none). NOT folded into `multishot`: it is conditional on the
    /// magazine position, which only the sim can evaluate.
    pub multishot_on_last_round: f64,
    /// The same window, in the OTHER BRACKET: "+5 **Base** Multishot on final
    /// magazine burst", which the wiki notes is "added before mods, and is
    /// thus multiplied by multishot bonuses". So it raises what the weapon's
    /// base pellet count IS for that burst, and every relative bonus — the mod
    /// bucket, a Galvanized stack, an arcane's grant — reads the raised number.
    pub base_multishot_on_last_round: f64,
    /// Plentiful Mayhem's damage bonus on multishot-GENERATED projectiles
    /// (0.0 = none), which also makes multishot spend ammo. Not folded into any
    /// damage bucket: it is an independent multiplier on part of the pellets.
    pub multishot_ammo_bonus: f64,
    /// The Incarnon transformation economy of THIS form, carried through
    /// so the cycle model reads it from data instead of hardcoding one
    /// weapon's numbers.
    pub gauge_form: Option<GaugeForm>,
    /// Beam geometry with `damage_radius_m` already scaled by Blast Range mods.
    /// Firestorm (Primed) enlarges the impact sphere — the one thing a
    /// single-target panel can honestly report about it, since the sphere adds
    /// no damage to a target the beam already struck.
    pub beam: Option<BeamGeometry>,
    /// Additive headshot-damage bonus from evolutions (Caput Mortuum).
    pub headshot_damage_bonus: f64,
    /// Devouring Attrition's (chance, bonus) on non-crit instances.
    pub noncrit_bonus: Option<(f64, f64)>,
    /// Overwhelming Attrition's stacking damage buff.
/// Every stacking buff, with [`BuffGrant::FireRate`] already converted from
    /// a fraction to an absolute rate. See [`StackingBuff`].
    pub stacking_buffs: Vec<StackingBuff>,
    /// ModifiedBase = unmodded total × (1 + Σ base damage) — the base of
    /// every status-payload formula (elemental portions excluded).
    pub modified_base: f64,
    pub crit_chance: f64,
    pub crit_damage: f64,
    /// PRELUDE OF MIGHT, unresolved on purpose: `(how much of `crit_damage`
    /// this perk is, the crit-chance threshold it has to stay under)`. A panel
    /// is the OPTIMISTIC half of the condition — the wiki's note says the
    /// threshold is read against a crit chance the panel cannot see — so the
    /// perk is granted here and the SIM takes it back on any hit that has
    /// climbed over the line. `None` when the perk is not installed, or when
    /// the panel alone already fails the condition and there is nothing to
    /// take back. See [`WeaponBase::crit_mult_below_cc`].
    pub crit_mult_below_cc: Option<(f64, f64)>,
    pub status_chance: f64,
    /// UNMODDED crit and status stats of the DIRECT part — the bases a
    /// RELATIVE live buff multiplies, the counterpart of `base_multishot`.
    pub base_crit_chance: f64,
    pub base_crit_damage: f64,
    pub base_status_chance: f64,
    pub fire_rate: f64,
    /// MODDED charge time (bows) — `base / (1 + fire-rate bonuses)`, the same
    /// factor `fire_rate` is multiplied by, so the two never disagree about
    /// what a fire-rate mod did. `Some` means the sim paces on this instead of
    /// `1 / fire_rate`.
    pub charge_seconds: Option<f64>,
    /// See [`crate::weapons_data::AttackSpec::charge_ammo_per_second`]. Set,
    /// the charge spends the magazine and the damage rides on it.
    pub charge_ammo_per_second: Option<f64>,
    /// See [`crate::weapons_data::SustainedFireRate`]. It rides through the mod
    /// layer UNTOUCHED — it is a fraction of whatever rate the build ends up
    /// with, so a fire-rate mod raises the ceiling and the floor together.
    pub sustained_fire_rate: Option<crate::weapons_data::SustainedFireRate>,
    /// See [`crate::weapons_data::Battery`]. Untouched by the mod layer too:
    /// the regen rate is the weapon's and a magazine mod changes only how many
    /// rounds it has to refill.
    pub battery: Option<crate::weapons_data::Battery>,
    /// Ammo per shot — a WEAPON constant, so no mod bucket touches it.
    pub ammo_cost: f64,
    /// See `weapons_data::WeaponSpec::headshot_bonus_multiplicative`.
    pub headshot_bonus_multiplicative: bool,
    pub charge_cadence: crate::weapons_data::ChargeCadence,
    /// The burst shape with its DELAY already modded — the same treatment
    /// `charge_seconds` gets, and for the same reason: a fire-rate bonus is
    /// spent here rather than re-derived in the sim.
    pub burst: Option<crate::weapons_data::BurstSpec>,
    pub multishot: f64,
    /// The weapon's UNMODDED pellet count — the base a relative multishot
    /// buff (Conjunction Voltage) multiplies when it joins the bucket live.
    pub base_multishot: f64,
    pub magazine_size: f64,
    /// Reserve rounds after mods (Ammo Chain, a riven's Ammo Maximum…), and
    /// whether the sim is allowed to spend them. Both travel together: a
    /// number without the flag is a panel figure, not a limit.
    pub ammo_reserve: f64,
    pub has_reserve: bool,
    pub no_resupply: bool,
    /// Untouched by mods — the passive's numbers are the weapon's own.
    pub super_crit_on_status: Option<crate::weapons_data::SuperCritSpec>,
    /// See `weapons_data::WeaponSpec::beam_ramp_floor`. No mod moves it.
    pub beam_ramp_floor: f64,
    /// Does this weapon apply MICROWAVE? See `dummy::DebuffState::microwave`.
    pub applies_microwave: bool,
    /// See `weapons_data::WeaponSpec::independent_procs`.
    pub independent_procs: &'static [&'static str],
    /// Forced procs, carried through unmodded — no mod grants or removes one.
    pub forced_procs: Vec<DamageType>,
    /// ONE RESOLVED VECTOR PER PROJECTILE, in firing order — `(direct, radial)`
    /// — for a weapon whose missiles carry different innate elements. EMPTY on
    /// every other weapon, and the fight reads `damage` as it always did.
    ///
    /// Resolved by running the whole panel once per element rather than by
    /// retyping a finished vector, because an innate element enters the
    /// elemental hierarchy and a finished vector has already forgotten where
    /// its Blast came from. It costs six resolves at BUILD time and nothing in
    /// the fight.
    pub pellet_damage: Vec<(DamageVector, DamageVector)>,
    /// See `weapons_data::AttackSpec::multishot_adds_damage`.
    pub multishot_adds_damage: bool,
    /// The field the attack plants, unmodded for the same reason: no mod in
    /// the roster lengthens it. See `weapons_data::AttackSpec`.
    pub attractor_seconds: Option<f64>,
    /// Untouched by mods: the tendril cap is the weapon's.
    pub tendril_max: u32,
    /// How far a tendril reaches and how far off the reticle it will take a
    /// body — see [`crate::weapons_data::TendrilSpec`]. Both zero on every
    /// weapon that has no tendrils.
    pub tendril_range_m: f64,
    pub tendril_acquire_deg: f64,
    /// THE SHOT COMBO COUNTER, or `None` — and `None` is what a sniper fired
    /// from the hip resolves to, because *"building combo and benefiting from
    /// its multiplier requires being scoped in"* (wiki `Sniper Rifle`). That is
    /// the whole gate: it is answered once, here, so the simulator, the
    /// optimizer and the board's no-aim ruler all get the same answer without
    /// any of them knowing what a sniper is.
    pub sniper_combo: Option<crate::weapons_data::SniperCombo>,
    /// Sentient Surge: crit chance added PER ACTIVE TENDRIL, relative to the
    /// unmodded base — "Additive to other crit chance and status chance mods"
    /// (wiki), so it joins the same bucket Pistol Gambit does rather than
    /// forming one of its own.
    pub cc_per_tendril: f64,
    /// Its status half, same bucket rule.
    pub sc_per_tendril: f64,
    /// HATA-SATYA under Emergent: relative crit chance per hit and the cap,
    /// spent in the sim because the pile's size is a fact about the fight.
    /// `None` under the other policies — AssumedMax has already folded it into
    /// `crit_chance`, and BaseOnly refuses conditionals.
    pub cc_per_hit: Option<(f64, u32)>,
    /// Fraction of the magazine returned on each kill, from the reserve.
    pub mag_refill_on_kill: f64,
    /// The syndicate radial this build's augment grants, if any.
    pub syndicate_radial: Option<crate::syndicates_data::SyndicateDef>,
    pub reload_seconds: f64,
    /// Σ reload-speed bonuses — transitions (Incarnon transmute/revert)
    /// scale by the same formula: time = base / (1 + this).
    pub reload_bonus: f64,
    /// Σ base-damage bonuses (needed live when CO joins this bucket).
    pub base_damage_bonus: f64,
    /// See [`WeaponBase::bd_below_half_health`] — carried through unchanged,
    /// already corrected for the granting perk's own flat base damage.
    pub bd_below_half_health: f64,
    /// See [`WeaponBase::cc_on_undamaged`] — converted to the post-mod number.
    pub cc_on_undamaged: f64,
    /// See [`WeaponBase::cd_on_undamaged`] — converted to the post-mod number.
    pub cd_on_undamaged: f64,
    /// Σ (CO per_stack × stacks) under `AssumedMax` (0 under
    /// `Emergent` — see `co_stack`) — applied per this weapon's
    /// [`CoBehavior`] × `co_base_fraction`, DIRECT HITS ONLY.
    pub co_per_type: f64,
    pub co_behavior: CoBehavior,
    /// WHAT PRIMARY COMPRESSION HAS TO WORK WITH on this build — the metres of
    /// blast radius it gives up while aiming, and which bracket it pays into.
    /// `None` = the weapon has no row (nothing to compress) or the fight's
    /// Tenno is not aiming, which is the same answer: the arcane is worth
    /// nothing.
    ///
    /// The arcane's own two ramps are NOT spent here. They are per METRE and
    /// this is the metres, so the multiplication happens where a build meets an
    /// arcane — `DummyParams::from_panel` — and that is one place rather than
    /// three. It cannot be this one: the optimizer resolves a panel ONCE and
    /// pairs it with every arcane in the search, so a panel that had already
    /// spent an arcane would have to be re-resolved per job.
    ///
    /// PER FORM. The Torid's cloud pays +240% and its Incarnon beam pays
    /// nothing, so one arcane has two answers inside one cycle.
    pub compression: Option<Compression>,
    pub co_base_fraction: f64,
    /// Live on-kill CO stacks (Emergent policy).
    pub co_stack: Option<StackSpec>,
    /// Live on-kill multishot stacks (Emergent policy); per_stack is
    /// already × base pellets.
    pub ms_stack: Option<StackSpec>,
    /// Crosshairs' on-headshot buff (Emergent): ABSOLUTE crit chance
    /// (base_cc × bonus) as a timed buff (starts active).
    pub cc_on_headshot: Option<TimedBuff>,
    /// Crosshairs' on-headshot-kill stacks (Emergent): per_stack is
    /// ABSOLUTE crit chance; per-stack expiry semantics.
    pub cc_stack: Option<StackSpec>,
    /// (1 + Σ status damage) — multiplies status payload values.
    pub status_damage_mult: f64,
    /// (1 + Σ status duration) — scales status-effect DoT durations.
    pub status_duration_mult: f64,
    /// Σ chance for a CRITICAL hit to apply a Slash status (Hunter
    /// Munitions), rolled per pellet, independent of status chance.
    pub slash_on_crit: f64,
    /// MOD SET bonus: chance for a hit that ALREADY crit to move up one
    /// critical tier (Vigilante). Scales per equipped member with no
    /// threshold — see [`crate::mod_sets_data`]. 0.0 = no set equipped.
    pub crit_tier_upgrade_chance: f64,
    /// Summed INDIRECT buckets (recoil, accuracy, ammo…): outside the
    /// theoretical-DPS math, stated on the panel; a future shooter model
    /// (2D recoil/aim, travel time, ammo sustain) consumes them.
    pub indirect: Vec<(IndirectStat, f64)>,
    /// (element, 1 + Σ that element's bonuses) — the elemental bracket of
    /// DoT tick formulas (only literal same-element mods count).
    pub elem_dot_bonus: Vec<(DamageType, f64)>,
    /// (faction, Σ bonus) — faction-damage bucket (Bane/Expel), ADDITIVE
    /// within a faction. Applied at sim time only vs a matching-faction
    /// target (×2 on DoT ticks); shown on the panel as a conditional row.
    pub faction_damage: Vec<(Faction, f64)>,
    /// Σ LISTED Weak Point damage (Acuity). Sim: +1.5× this on the part
    /// multiplier of true weak points, before the headshot bracket.
    pub weakpoint_damage: f64,
    /// ABSOLUTE crit chance added on weak-point hits only (base_cc × Σ
    /// relative weak-point CC bonuses); part-conditional, all policies.
    pub weakpoint_cc_rel: f64,
    /// King's Gambit's other half: a MULTIPLIER on a non-weak-point pellet's
    /// crit chance, applied after everything else. 1.0 = ordinary.
    pub bodyshot_cc_mult: f64,
    /// WISEMAN'S REGARD, AS A LIVE SPEC: `(rate, cap, what the panel already
    /// folded in)`, the first two ALREADY multiplied by the status-chance mods
    /// because the card grants BASE status chance.
    ///
    /// "30% of CURRENT Critical Chance" is current at the moment of the shot,
    /// not at the arsenal: the row names Secondary Outburst, Cascadia
    /// Overcharge, Secondary Enervate and Galvanized Crosshairs among the
    /// sources that feed it, and all four are live. The panel still shows the
    /// static answer — that is what a panel can say — so the sim subtracts the
    /// third number and adds what the shot actually earns.
    pub derived_status_from_crit: Option<(f64, f64, f64)>,
    /// The mirror (the Dera's High Ground), same shape: `(rate, cap, folded)`
    /// with the first two multiplied by the CRIT-chance mods.
    pub derived_crit_from_status: Option<(f64, f64, f64)>,
    /// Galvanic Reload: `(status, chance, rounds)`, rolled ONCE PER SHOT.
    /// Double Tap: `(per stack, max stacks, seconds)` — its OWN multiplier,
    /// counted per trigger pull. See [`ModEffect::ConsecutiveHitDamage`].
    pub consecutive_hit_damage: Option<(f64, u32, f64)>,
    /// SYNTH CHARGE's multiplier for the magazine's LAST round — see
    /// [`ModEffect::LastRoundDamage`]. Zero on a continuous weapon and on an
    /// Incarnon form, resolved here because only this layer knows the form.
    pub last_round_damage: f64,
    pub round_restore_on_status: Option<(crate::damage::DamageType, f64, f64)>,
    /// Exact Penance: the chance a KILL — from anywhere — reloads instantly.
    pub instant_reload_on_kill: Option<f64>,
    /// Resonant Restore: `(per stack, max stacks)`, the per-stack value ALREADY
    /// scaled by the magazine mods — the card says "Base Magazine Capacity", so
    /// a Magazine Warp build gets more out of every stack. Same units
    /// conversion `BuffGrant::FlatBaseDamage` and `FireRate` take, and for the
    /// same reason: the sim adds it to a number the mods are already inside.
    pub mag_growth_on_empty_reload: Option<(f64, u32)>,
    /// Sharpened Bullets under Emergent: ABSOLUTE crit-damage add as a timed
    /// buff (starts inactive), granted/refreshed on every kill.
    pub cd_on_kill: Option<TimedBuff>,
    /// Pressurized Magazine under Emergent: ABSOLUTE fire-rate add as a timed
    /// buff (starts inactive), granted on every reload.
    pub fr_on_reload: Option<TimedBuff>,
    /// Deadly Efficiency's window — see [`ModEffect::OnReloadDamage`]. Its
    /// `value` is the RELATIVE bonus, because it joins the base-damage bucket
    /// rather than replacing a rate.
    pub bd_on_reload: Option<TimedBuff>,
    /// EXIMUS ADVANTAGE's window — see [`ModEffect::OnEximusWeakpointDamage`].
    /// Its `value` is RELATIVE, joining the base-damage bucket beside Hornet
    /// Strike's, which is what the card's "Stacks additively with base damage
    /// bonuses" says it should do.
    pub bd_on_eximus_weakpoint: Option<TimedBuff>,
    /// READY RETALIATION's window — see
    /// [`crate::evolutions_data::EvoEffect::ReloadSpeedOnEmptyReload`]. It joins
    /// the reload bucket the mods and `evo_reload_bonus` feed, but only while
    /// open, and only a reload FROM EMPTY opens it.
    /// READY RETALIATION as the sim holds it: a buff with NO DURATION that is
    /// simply up or down (0.0 = the weapon does not have the perk).
    ///
    /// The magazine running out puts it up; a reload completing, or either
    /// Incarnon transform completing, takes it down — because all three refill
    /// the magazine. Nothing ASKS whether the moment is a reload: the value is
    /// summed into the live reload-speed total wherever that total is needed,
    /// and only the removal events are reasoned about (owner, 2026-08-11).
    pub rs_on_reload: f64,
    /// Flensing Spikes' rate — see [`WeaponBase::armor_strip_per_puncture`].
    pub armor_strip_per_puncture: f64,
    /// EXECUTIONER'S FORTUNE — see [`InstantReload`]. Carried straight to the
    /// sim under every policy: it is an EVENT, and there is no panel stat an
    /// assumed-max reading could spend it into (a magazine that refills itself
    /// is not a bigger magazine).
    pub instant_reload: Option<InstantReload>,
    /// LINGERING JUDGEMENT — see [`HeadshotStreak`]. Carried to the sim under
    /// every policy: whether the streak ever arms is a property of the FIGHT
    /// (a body-shot engagement never does), not something a panel can assume.
    pub headshot_streak: Option<HeadshotStreak>,
    /// SPITEFUL DEFILEMENT — see [`WeaponBase::cd_below_status_count`]. Also
    /// carried: its condition is the TARGET's live status count.
    pub cd_below_status_count: Option<(u32, f64)>,
    /// Hemorrhage's status-conversion roll (an event mechanic — active under
    /// every policy; contributes no static panel stat).
    pub proc_conversion: Option<ProcConv>,
    /// The evolution's PERMANENT stacked multishot (Fevered Frenzy), if any:
    /// its FULL contribution is already inside `multishot`; the per-buff
    /// config rescales via this spec (no in-sim trigger, no decay — the
    /// stack count is a static choice, full by default).
    pub evo_ms: Option<EvoMsBuff>,
    /// The evolution's PERMANENT flat base damage (Reified Bane), if any.
    pub evo_bd: Option<EvoBdBuff>,
    /// Stats an equipped mod has LOCKED at the weapon's default (`disables`):
    /// `multishot` for the Acuity pair, `fire_rate` for the Cannonades.
    ///
    /// Stated on the panel because the panel is not the last word on either.
    /// A lock is absolute — "set to its default ignoring other bonuses, even
    /// negative effects" — and the sim owns the live sources that never reach
    /// this struct's arithmetic: an arcane's multishot stacks, the weapon's
    /// Frenzy passive. They read this rather than each re-deriving it.
    pub locked: Vec<&'static str>,
}

impl ResolvedPanel {
    /// Is the reserve effectively bottomless for this weapon, under this
    /// scenario's Infinite-ammo setting?
    ///
    /// THE ONE PLACE THE RULE IS WRITTEN. It lives on the panel rather than in
    /// a caller because there were TWO callers writing it — the web api and the
    /// optimizer — and they wrote the same wrong version of it
    /// (`infinite_ammo || !finite_reserve`). The simulator is the truth and the
    /// optimizer obeys it, so the optimizer must CALL this, not restate it.
    ///
    /// Three facts meet here, and two of them were one field until 2026-08-04:
    ///
    /// - `has_reserve` — is there a pool behind the magazine at all? A sentinel
    ///   weapon has none, so nothing can make it run out.
    /// - `no_resupply` — can the game refill it mid-fight? False for everything
    ///   but a ground Arch-Gun, which is REMOVED when empty and cannot be
    ///   called back down for five minutes.
    /// - `infinite_ammo` — the scenario's setting, which is how the sim stands
    ///   in for ammo PICKUPS, since it models none of them.
    ///
    /// The setting is therefore about PICKUPS, and a weapon that cannot receive
    /// one is not covered by it. That is what lets a single benchmark term be
    /// right for the whole roster: reserves ignored where the game would refill
    /// them, real where it cannot (owner, 2026-08-04).
    pub fn reserve_is_infinite(&self, infinite_ammo: bool) -> bool {
        !self.has_reserve || (infinite_ammo && !self.no_resupply)
    }
}


/// A permanent flat-BASE-DAMAGE buff on the resolved panel, sibling to
/// [`EvoMsBuff`] and rescaled the same way: the panel already carries the full
/// contribution, and the buff card scales it back out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EvoBdBuff {
    /// The flat base damage the buff contributes at full stacks (+14).
    pub full: f64,
    /// The base TOTAL without it — the denominator for scaling back. The
    /// bonus rides the whole mod chain multiplicatively (flat base damage is
    /// added pro-rata BEFORE mods), so removing it is one ratio on the
    /// resolved vector rather than a re-resolve.
    pub without: f64,
    pub max_stacks: u32,
    pub stacks: u32,
}

/// A permanent stacked multishot buff on the resolved panel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EvoMsBuff {
    /// FINAL multishot contributed at full stacks (base pellets × Σ bonus).
    pub full: f64,
    pub max_stacks: u32,
    /// The count actually in play. PERMANENT stacks never move during a run,
    /// so this is a static choice — but the replay still has to be able to say
    /// what it was, and `multishot` has already absorbed it by then.
    pub stacks: u32,
}

/// A resolved status-conversion roll (Hemorrhage).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProcConv {
    pub from: DamageType,
    pub to: DamageType,
    pub chance: f64,
    /// Chance ×`low_rate_mult` while LIVE fire rate < this (strictly).
    pub low_rate_threshold: f64,
    pub low_rate_mult: f64,
}

/// Resolve a mod set in slot order against a weapon base.
/// Resolve a build for the NEUTRAL Tenno (`data/tenno/default.yaml`): aiming,
/// no frame chosen, no ability running. The panel's and the optimizer's
/// default view, and the historical behaviour of every caller.
pub fn resolve(base: &WeaponBase, mods: &[&ModDef], policy: StackPolicy) -> ResolvedPanel {
    resolve_for(base, mods, policy, crate::tenno_data::default_tenno())
}

/// Resolve a build for a GIVEN Tenno — the fight's second actor. Every
/// `condition:` a mod card states is a question about this player: aiming,
/// invisible, airborne. A gated effect whose condition is false is absent from
/// the static buckets AND from the emergent specs handed to the sim, so the
/// buff never arms rather than arming and contributing zero.
pub fn resolve_for(
    base: &WeaponBase,
    mods: &[&ModDef],
    policy: StackPolicy,
    tenno: &crate::tenno_data::Tenno,
) -> ResolvedPanel {
    // A GATED FLAT BASE-DAMAGE ADD IS A CHANGE TO THE WEAPON, so it is folded
    // BEFORE anything reads the panel — Haven Foray's "With Overshields:
    // Increase Base Damage by +40" makes the same weapon a plain "+40" would,
    // and `add_flat_base_damage` is the one place that decides what that means.
    //
    // It is the only gated grant that cannot be a term added later: the other
    // four join a bucket, and this one moves the number every bucket multiplies.
    // Hence the clone, and hence only when a gate is actually open — the neutral
    // Tenno opens none, so the ordinary path allocates nothing.
    // A FORM THAT CANNOT ZOOM CANNOT BE AIMING. The wiki's word for aiming IS
    // "Zoom" (its page opens "Zoom (or aiming, aiming down sights (ADS))", and
    // the Galvanized mods link it as `[[Zoom|aiming]]`), and DE settled the
    // consequence in a patch note about Mesa's Regulators: the buffs "never
    // actually applied due to the 'on aim' criteria not being fulfilled".
    //
    // Answered HERE rather than in `webapi`, for two reasons. The optimizer and
    // every other caller get it for free — and it is per FORM, which a single
    // request-level Tenno cannot express: the Vasto aims and its Incarnon form
    // does not, and a cycle resolves both.
    let aimless;
    let tenno = if base.cannot_zoom && tenno.state.aiming {
        let mut t = tenno.clone();
        t.state.aiming = false;
        aimless = t;
        &aimless
    } else {
        tenno
    };
    let gated_flat: f64 = base
        .gated
        .iter()
        .filter(|(c, k, _)| *k == GatedGrant::FlatBaseDamage && c.open(tenno))
        .map(|(_, _, v)| v)
        .sum();
    // …AND THE SAME QUESTION FOR THE MAGAZINE. Folded into the BASE here rather
    // than into the modded size below, so a gated +14 and a plain +14 are the
    // same weapon everywhere the magazine is read — the mods multiply it, the
    // by-shell reload counts it, and a charged form's ammo ratio has it in both
    // halves of its fraction.
    //
    // "Increased Base Magazine Capacity does not affect Incarnon Form" (Lone
    // Gun, Extended Volley): the same `incarnon.is_none()` guard `apply` puts
    // on the ungated spelling, kept here because this add cannot happen there —
    // `apply` never sees a Tenno.
    let gated_mag: f64 = if base.gauge_form.is_none() {
        base.gated
            .iter()
            .filter(|(c, k, _)| *k == GatedGrant::FlatBaseMagazine && c.open(tenno))
            .map(|(_, _, v)| v)
            .sum()
    } else {
        0.0
    };
    let owned;
    let base = if gated_flat > 0.0 || gated_mag > 0.0 {
        let mut b = base.clone();
        if gated_flat > 0.0 {
            // A GATED FLAT ADD FEEDS THE CO BASE, which is what it did
            // before this became a choice: the old code left the fraction
            // alone and grew the panel, so the absolute the term read grew
            // with it. Preserved rather than decided — no gated perk is on
            // the CO catalog and none has been measured.
            b.add_flat_base_damage(gated_flat, gated_flat);
        }
        b.magazine_size += gated_mag;
        owned = b;
        &owned
    } else {
        base
    };
    // THE FIGHT'S OWN BONUSES SEED THE BUCKETS, before a single mod is read —
    // which is the whole of what "the effect equals stuffing in another mod"
    // means (owner, 2026-08-13). They are ADDITIVE with the mods by
    // construction, because they are in the same variable, so nothing
    // downstream had to learn the concept: every bucket's arithmetic, every
    // lock, every panel row and the optimizer's own scoring treat them as one
    // more card in the build.
    //
    // A LOCK STILL WINS. `locks("multishot")` zeroes the bucket further down,
    // and a fight bonus is in it — which is right: "set to its default ignoring
    // other bonuses" does not make an exception for where the bonus came from.
    let fb = &tenno.bonuses;
    let (mut bd, mut ms, mut cc, mut cd, mut sc, mut fr, mut sd) = (
        fb.base_damage,
        fb.multishot,
        fb.crit_chance,
        fb.crit_damage,
        fb.status_chance,
        // NOT doubled by `fire_rate_mod_multiplier`: the bow x2 is printed on
        // the CARD of a fire-rate mod, and a fight bonus has no card.
        fb.fire_rate,
        fb.status_damage,
    );
    // THE SCOPE'S OWN CRIT, and it joins the ordinary buckets. Most snipers'
    // zoom grants a critical CHANCE or MULTIPLIER rather than headshot damage
    // (the Lanka +50% chance at 8x, the Rubico family +50% multiplier), and
    // "these zoom buffs ... generally stack additively with similar buffs from
    // mods" (wiki `Sniper Rifle`) — so they are bucket terms, not their own
    // factor. Added HERE, above the lock site, so a mod that pins crit chance
    // wipes this too: "set to its default ignoring other bonuses" does not make
    // an exception for where the bonus came from.
    if tenno.state.aiming {
        cc += base.scope_crit_chance;
        cd += base.scope_crit_multiplier;
    }
    // …and the Lanka's, which is NOT a bucket term. Its bonus is "a flat
    // +20/30/50 critical chance, applied after mods", so it lands on the
    // post-mod layer beside the other flat grants rather than being multiplied
    // by the weapon's unmodded 25%.
    let scope_post_cc = if tenno.state.aiming { base.scope_crit_chance_post_mod } else { 0.0 };
    // RELOAD STARTS AT THE EVOLUTION'S BONUS, not at zero. Rapid Reinforcement
    // and its family feed the SAME additive bucket the mods do — one bucket, so
    // an evolution's +60% and Primed Fast Hands' +55% sum rather than
    // multiplying, which is the shape every other shared stat here has.
    let mut rl = base.evo_reload_bonus + fb.reload_speed;
    // Magazine-capacity and status-duration additive buckets.
    let (mut mag, mut sdur) = (fb.magazine, 0.0);
    // Sentient Surge's three, carried to the sim rather than spent here: all
    // three depend on fight state (how many tendrils are up, whether anything
    // died) that the panel cannot know.
    let (mut per_tendril_cc, mut per_tendril_sc, mut mag_refill) = (0.0, 0.0, 0.0);
    // A syndicate augment's radial, resolved from the six-effect table.
    let mut syndicate_radial: Option<crate::syndicates_data::SyndicateDef> = None;
    // Blast RANGE bucket (Firestorm / Fulmination): + the sum, of base radius.
    let mut br = 0.0;
    // Hunter Munitions: its own bucket, because its roll is its own.
    let mut slash_on_crit = 0.0;
    // Unconditional weapon-level CO (Carnage Reign) seeds the static rate.
    // …AND THE HALF THAT ASKS ABOUT THE PLAYER. "With Sprint Speed 1.2 or
    // Higher" is a question about who is carrying the gun, so it is answered
    // here rather than in `apply`, which never sees a Tenno. The neutral player
    // sprints at 0.9 — the slowest frame — so a perk gated on speed pays
    // nothing until someone says which frame is holding it.
    // …AND THE HALF THAT ASKS ABOUT THE PLAYER, summed once for every bracket.
    // The neutral Tenno opens none of these gates, so a build that does not say
    // which frame is holding the gun pays nothing for them — which is the
    // honest default and the same rule every other Tenno field here follows.
    let gate = |g: GatedGrant| -> f64 {
        base.gated
            .iter()
            .filter(|(c, k, _)| *k == g && c.open(tenno))
            .map(|(_, _, v)| v)
            .sum()
    };
    let mut co = base.innate_co_per_type + gate(GatedGrant::ConditionOverload);
    let (mut co_stack, mut ms_stack): (Option<StackSpec>, Option<StackSpec>) = (None, None);
    let mut cc_on_headshot: Option<TimedBuff> = None;
    let mut cc_stack: Option<StackSpec> = None;
    // …and the weak-point crit bucket starts at the EVOLUTION's, not at zero,
    // because the card says it is additive with the mods that write here.
    let mut consecutive_hit: Option<(f64, u32, f64)> = None;
    // SYNTH CHARGE. Summed, though only one such mod exists — a bucket of one
    // is still a bucket, and a second card would otherwise silently replace the
    // first.
    let mut last_round_damage = 0.0f64;
    let (mut wp_dmg, mut wp_cc) = (0.0, base.evo_weakpoint_cc_rel);
    let mut cd_on_kill: Option<TimedBuff> = None;
    let mut fr_on_reload: Option<TimedBuff> = None;
    let mut bd_on_reload: Option<TimedBuff> = None;
    let mut bd_on_eximus_weakpoint: Option<TimedBuff> = None;
    let mut cc_per_hit: Option<(f64, u32)> = None;
    // READY RETALIATION arrives on the BASE (an evolution wrote it there),
    // unlike the two above which arrive from mods — so the policy split is
    // here rather than in the mod loop.
    //
    // AssumedMax spends it into the RELOAD BUCKET, which is what the panel and
    // the optimizer's ranking read — a panel is a statement about a reload, and
    // in this arena every reload is from empty. Emergent hands it to the sim,
    // which applies it to each reload and to nothing else: a transmute
    // animation is scaled by the same bucket and is NOT a reload, so folding it
    // into `rl` there would have sped up an animation this perk never touches.
    // A sentinel's conditional never fires.
    let mut rs_on_reload = match policy {
        StackPolicy::Emergent => base.rs_on_empty_reload,
        StackPolicy::AssumedMax => {
            rl += base.rs_on_empty_reload;
            0.0
        }
        _ => 0.0,
    };
    let mut proc_conv: Option<ProcConv> = None;
    let mut elem_bonus: Vec<(DamageType, f64)> = Vec::new();
    // SEEDED from the weapon, not empty: an evolution's indirect stat is a
    // property of the weapon by the time `resolve` runs (evolutions are folded
    // into `WeaponBase` first), and it shares its bucket with the mods'.
    let mut indirect: Vec<(IndirectStat, f64)> = base.indirect.clone();
    // …plus any the PLAYER's state opens. Same bucket the mods and the
    // unconditional evolutions feed, so it sums rather than multiplying.
    {
        let ps = gate(GatedGrant::ProjectileSpeed);
        if ps != 0.0 {
            match indirect.iter_mut().find(|(s, _)| *s == IndirectStat::ProjectileSpeed) {
                Some((_, v)) => *v += ps,
                None => indirect.push((IndirectStat::ProjectileSpeed, ps)),
            }
        }
        let acc = gate(GatedGrant::Accuracy);
        if acc != 0.0 {
            match indirect.iter_mut().find(|(s, _)| *s == IndirectStat::Accuracy) {
                Some((_, v)) => *v += acc,
                None => indirect.push((IndirectStat::Accuracy, acc)),
            }
        }
    }
    // Charge-rate bonuses, summed apart from fire rate: both shorten the draw,
    // only fire rate also raises an uncharged form's cadence.
    let mut cr = 0.0f64;
    let mut faction_bonus: Vec<(Faction, f64)> = Vec::new();
    // Physical (IPS) bonuses, per type — scale the base of that physical type.
    let mut phys_bonus: Vec<(DamageType, f64)> = Vec::new();
    // Stats LOCKED from modding by an equipped mod (Pistol Acuity → multishot,
    // Semi-Pistol Cannonade → fire_rate). Their bucket is zeroed after the loop.
    let mut disabled: Vec<&str> = Vec::new();

    for m in mods {
        // `requires`: a mod whose required weapon trait is absent is INERT here
        // (calc-layer, not an equip block) — skip all of its effects/locks.
        if let Some(req) = m.requires {
            if !base.traits.contains(&req) {
                continue;
            }
        }
        for &d in &m.disables {
            if !disabled.contains(&d) {
                disabled.push(d);
            }
        }
        for e in &m.effects {
            // Unwrap the player gates HERE so no arm below has to know about
            // them: a gated effect either becomes its inner effect or vanishes.
            // `aiming` is the older, bare form of the same idea; the Tenno one
            // asks the player's state instead of a threaded bool.
            let e: &ModEffect = match e {
                ModEffect::WhileTenno(c, inner) if c.holds(tenno) => inner,
                ModEffect::WhileTenno(..) => continue,
                other => other,
            };
            match *e {
                ModEffect::WhileTenno(..) => {
                    unreachable!("unwrapped above")
                }
                // DOUBLE TAP does not join a bucket — it carries its own
                // multiplier to the sim, which is the whole point of the card's
                // "multiplicatively stacks with damage bonuses like Serration".
                ModEffect::LastRoundDamage(v) => last_round_damage += v,
                ModEffect::ConsecutiveHitDamage { per_stack, max_stacks, duration } => {
                    consecutive_hit = Some((per_stack, max_stacks, duration));
                }
                ModEffect::BaseDamage(v) => bd += v,
                ModEffect::Multishot(v) => ms += v,
                ModEffect::CritChance(v) => cc += v,
                // The per-tendril halves do NOT join `cc`/`sc` here: their
                // size depends on how many tendrils are up, which is a fact
                // about the fight and not about the build. They travel to the
                // sim as rates and are spent there, the same way an on-reload
                // buff is.
                ModEffect::PerTendril { crit_chance, status_chance } => {
                    per_tendril_cc += crit_chance;
                    per_tendril_sc += status_chance;
                }
                // HATA-SATYA. Same split as the tendrils above and for the
                // same reason — how many hits are in the pile is a fact about
                // the fight — except that the panel HAS an honest maximum to
                // show, because the card publishes one (500%).
                ModEffect::CritChancePerHit { per_stack, max_stacks } => match policy {
                    StackPolicy::AssumedMax => cc += per_stack * f64::from(max_stacks),
                    StackPolicy::Emergent => cc_per_hit = Some((per_stack, max_stacks)),
                    StackPolicy::BaseOnly => {} // sentinel: conditional never fires
                },
                ModEffect::MagazineRefillOnKill(v) => mag_refill += v,
                // The card names one of six; the payload is the syndicate's.
                ModEffect::SyndicateRadial { syndicate, .. } => {
                    syndicate_radial = crate::syndicates_data::get(syndicate).copied();
                }
                ModEffect::CritDamage(v) => cd += v,
                ModEffect::StatusChance(v) => sc += v,
                // "(x2 for Bows)" is on the CARD of every fire-rate mod, so it
                // is the mod's own bonus that doubles — penalties included
                // (Critical Delay reads −40% on a bow). Buff-granted fire rate
                // joins the bucket further down UNDOUBLED: no such card says it.
                ModEffect::FireRate(v) => fr += v * base.fire_rate_mod_multiplier,
                // The draw's own bucket. `fire_rate_mod_multiplier` is the
                // bow x2, which belongs to fire-rate mods and not to this.
                ModEffect::ChargeRate(v) => cr += v,
                ModEffect::ReloadSpeed(v) => rl += v,
                ModEffect::StatusDamage(v) => sd += v,
                // Independent of everything else: it is its own roll on a
                // crit, so it is its own bucket rather than joining status.
                ModEffect::SlashOnCrit(v) => slash_on_crit += v,
                // Physical (IPS) bonus: accumulate per type; applied to the
                // BASE physical component below (NOT the elemental hierarchy).
                ModEffect::Physical(t, v) => {
                    if let Some(x) = phys_bonus.iter_mut().find(|(a, _)| *a == t) {
                        x.1 += v;
                    } else {
                        phys_bonus.push((t, v));
                    }
                }
                ModEffect::Element(t, v) | ModEffect::CombinedElement(t, v) => {
                    if let Some(x) = elem_bonus.iter_mut().find(|(a, _)| *a == t) {
                        x.1 += v;
                    } else {
                        elem_bonus.push((t, v));
                    }
                }
                ModEffect::OnKillMultishot {
                    per_stack,
                    max_stacks,
                    duration,
                } => match policy {
                    StackPolicy::AssumedMax => ms += per_stack * max_stacks as f64,
                    StackPolicy::Emergent => {
                        ms_stack = Some(StackSpec {
                            per_stack: base.base_multishot * per_stack,
                            max_stacks,
                            duration,
                            initial_stacks: 0, // EARNED — docs/BUFFS.md §Activation policy
                        })
                    }
                    StackPolicy::BaseOnly => {} // sentinel: conditional never fires
                },
                ModEffect::ConditionOverload {
                    per_stack,
                    max_stacks,
                    duration,
                } => match policy {
                    StackPolicy::AssumedMax => co += per_stack * max_stacks as f64,
                    StackPolicy::Emergent => {
                        co_stack = Some(StackSpec {
                            per_stack,
                            max_stacks,
                            duration,
                            initial_stacks: 0, // EARNED — docs/BUFFS.md §Activation policy
                        })
                    }
                    StackPolicy::BaseOnly => {} // sentinel: conditional never fires
                },
                ModEffect::OnHeadshotCritChance { bonus, duration } => match policy {
                    StackPolicy::AssumedMax => cc += bonus,
                    StackPolicy::Emergent => {
                        cc_on_headshot = Some(TimedBuff {
                            // RELATIVE, deliberately: `cc += bonus` is what
                            // the AssumedMax arm does, and that bonus reaches
                            // EVERY attack part through the bucket. Resolving
                            // it against the direct part's base here made the
                            // same mod skip the explosion under Emergent.
                            value: bonus,
                            duration,
                            initial_active: false, // EARNED, like every timed buff
                        })
                    }
                    StackPolicy::BaseOnly => {} // sentinel: conditional never fires
                },
                ModEffect::OnHeadshotKillCritChance {
                    per_stack,
                    max_stacks,
                    duration,
                } => match policy {
                    StackPolicy::AssumedMax => cc += per_stack * max_stacks as f64,
                    StackPolicy::Emergent => {
                        cc_stack = Some(StackSpec {
                            per_stack, // RELATIVE — see cc_on_headshot above
                            max_stacks,
                            duration,
                            initial_stacks: 0, // EARNED — docs/BUFFS.md §Activation policy
                        })
                    }
                    StackPolicy::BaseOnly => {} // sentinel: conditional never fires
                },
                ModEffect::Indirect(stat, v) => {
                    if let Some(x) = indirect.iter_mut().find(|(s, _)| *s == stat) {
                        x.1 += v;
                    } else {
                        indirect.push((stat, v));
                    }
                }
                // Conditional handling buff (Reflex Draw): handling-only and
                // temporary — listed from the card, never a static panel stat.
                ModEffect::OnEquipHandling { .. } => {}
                // Faction bonus: additive within a faction (Bane + Roar share
                // one bracket); the matching-faction multiply happens at sim
                // time. Merge by faction here.
                ModEffect::FactionDamage(fac, v) => {
                    if let Some(x) = faction_bonus.iter_mut().find(|(f, _)| *f == fac) {
                        x.1 += v;
                    } else {
                        faction_bonus.push((fac, v));
                    }
                }
                ModEffect::MagazineCapacity(v) => mag += v,
                ModEffect::BlastRadius(v) => br += v,
                ModEffect::StatusDuration(v) => sdur += v,
                // Weak-point effects: conditional on the PART HIT, not on an
                // uptime — so they are active under EVERY policy and the sim
                // gates them on `is_head`. AssumedMax is about a buff's stack
                // count, not about where a bullet lands: no policy can make a
                // body shot into a head shot.
                //
                // The CC half used to fold into the plain bucket under
                // AssumedMax, which split ONE mod down the middle — Acuity's
                // Weak Point Damage stayed conditional while its Weak Point
                // Crit Chance became unconditional. On the panel (always
                // AssumedMax) that read the Burston Incarnon's 28% as 126%,
                // and handed the same 126% to the RADIAL, which can never
                // weak-point-hit at all. The arcane source of the same effect
                // (Cascadia Accuracy) was already in this bucket, so the mod
                // was the only thing in the engine claiming otherwise.
                ModEffect::WeakpointDamage(v) => wp_dmg += v,
                ModEffect::WeakpointCritChance(v) => wp_cc += v,
                ModEffect::OnKillCritDamage { bonus, duration } => match policy {
                    StackPolicy::AssumedMax => cd += bonus,
                    StackPolicy::Emergent => {
                        cd_on_kill = Some(TimedBuff {
                            value: bonus, // RELATIVE — see cc_on_headshot above
                            duration,
                            initial_active: false, // Sharpened Bullets seeds inactive
                        })
                    }
                    StackPolicy::BaseOnly => {} // sentinel: conditional never fires
                },
                ModEffect::OnReloadDamage { bonus, duration } => match policy {
                    StackPolicy::AssumedMax => bd += bonus,
                    StackPolicy::Emergent => {
                        bd_on_reload = Some(TimedBuff {
                            // RELATIVE: the sim adds it into the base-damage
                            // bucket alongside Serration, which is where the
                            // card's "+X% Damage" belongs.
                            value: bonus,
                            duration,
                            initial_active: false, // no reload has happened yet
                        })
                    }
                    StackPolicy::BaseOnly => {} // sentinel: conditional never fires
                },
                // EXIMUS ADVANTAGE, the same three-policy shape as Deadly
                // Efficiency above: the panel folds it in at its maximum, the
                // sim earns it. What the sim adds that the panel cannot is the
                // TARGET's half — a fight against a non-Eximus never opens the
                // window at all, and the panel has no target to ask.
                ModEffect::OnEximusWeakpointDamage { bonus, duration } => match policy {
                    StackPolicy::AssumedMax => bd += bonus,
                    StackPolicy::Emergent => {
                        bd_on_eximus_weakpoint = Some(TimedBuff {
                            value: bonus,
                            duration,
                            initial_active: false, // no weak point has been hit yet
                        })
                    }
                    StackPolicy::BaseOnly => {} // sentinel: conditional never fires
                },
                ModEffect::OnReloadFireRate { bonus, duration } => match policy {
                    StackPolicy::AssumedMax => fr += bonus,
                    StackPolicy::Emergent => {
                        fr_on_reload = Some(TimedBuff {
                            value: base.base_fire_rate * bonus,
                            duration,
                            initial_active: false, // Pressurized Magazine seeds inactive
                        })
                    }
                    StackPolicy::BaseOnly => {} // sentinel: conditional never fires
                },
                // Event mechanic — carried to the sim under every policy;
                // contributes no static panel stat.
                ModEffect::ProcConversion { from, to, chance, low_rate_threshold, low_rate_mult } => {
                    proc_conv = Some(ProcConv { from, to, chance, low_rate_threshold, low_rate_mult });
                }
                // Conditional buff at its assumed-max total — applied only under
                // AssumedMax (panel/optimizer); emergent leaves it to the sim.
                ModEffect::CondBuff(bucket, v) => {
                    if policy == StackPolicy::AssumedMax {
                        match bucket {
                            CondBucket::BaseDamage => bd += v,
                            CondBucket::Multishot => ms += v,
                            CondBucket::CritChance => cc += v,
                            CondBucket::CritDamage => cd += v,
                            CondBucket::StatusChance => sc += v,
                            CondBucket::StatusDamage => sd += v,
                            CondBucket::FireRate => fr += v,
                            CondBucket::ReloadSpeed => rl += v,
                        }
                    }
                }
            }
        }
    }

    // Apply `disables`: a locked stat cannot be modified — zero its mod bucket
    // (and any conditional stacks feeding it); the weapon's base value stays.
    //
    // A LOCK IS ABSOLUTE, AND THE MOD BUCKET IS NOT THE WHOLE OF IT. Both
    // families that carry one say the same sentence: "Equipping this mod will
    // set weapon's Fire Rate to its default ignoring other bonuses, EVEN
    // NEGATIVE EFFECTS" (wiki, Semi-Rifle/Shotgun/Pistol Cannonade) and
    // "...will set weapon's Multishot to its default ignoring other bonuses,
    // even negative effects" (Primary/Pistol Acuity). "Its default" is the
    // WEAPON'S value, so a source that never passed through the mod bucket —
    // an evolution's permanent bonus, an arcane's live stacks, the weapon's own
    // Frenzy passive — is not exempt just because the loop above cannot see it
    // (user, 2026-08-04). The out-of-bucket layers are shadowed here, and
    // `locked` carries the fact to the SIM, which owns the live ones.
    let locked_stat = |s: &str| disabled.contains(&s);
    let evo_ms_bonus = if locked_stat("multishot") {
        0.0
    } else {
        base.buff_multishot_bonus + gate(GatedGrant::Multishot)
    };
    let evo_ms_stacks = if locked_stat("multishot") { 0 } else { base.buff_ms_max_stacks };
    let ms_last_round = if locked_stat("multishot") { 0.0 } else { base.multishot_on_last_round };
    let evo_fr_bonus = if locked_stat("fire_rate") {
        0.0
    } else {
        // …plus the half that asks about the PLAYER. Answered here, where the
        // Tenno is; the neutral player sprints at 0.9 — the slowest frame — so
        // a perk gated on speed pays nothing until someone says which frame is
        // holding the gun.
        base.evo_fire_rate_bonus + gate(GatedGrant::FireRate)
    };
    // PRELUDE OF MIGHT, resolved here because it is the one evolution whose
    // condition is the BUILD's own output: "with Critical Chance below 40%".
    // Computed against the same expression the panel publishes, so the tile and
    // the number can never disagree about whether it is on.
    //
    // It joins the BASE multiplier, so crit-damage mods multiply it — the raw
    // wikitext says "Increase Base Critical Damage Multiplier by +3x", the same
    // wording `flat_base_crit_multiplier` already models for Critical Parallel.
    // It shipped for one commit added AFTER the mods instead, which on a
    // Primed Target Cracker build is 10.14x against the correct 13.44x. The
    // difference only appears once a crit-damage mod is on, which is why
    // reading the rendered page rather than the wikitext missed it: the word
    // that decides it is "Base".
    // ONE STAT DERIVED FROM THE OTHER, both grants computed from the PRE-GRANT
    // modded values. No weapon carries both — the Dera has one and the
    // Cestra/Sicarus/Vectis the other — so the order cannot matter, and
    // computing them this way means it never will.
    let modded_cc_pre = (base.base_crit_chance * (1.0 + cc) + (base.post_mod_crit_chance + scope_post_cc)).max(0.0);
    let modded_sc_pre =
        (base.base_status_chance * (1.0 + sc) + base.post_mod_status_chance).max(0.0);
    let derived = |spec: Option<(f64, f64)>, from: f64| -> f64 {
        spec.map_or(0.0, |(rate, cap)| (rate * from).min(cap))
    };
    let cc_from_sc = derived(base.base_crit_from_status, modded_sc_pre);
    let sc_from_cc = derived(base.base_status_from_crit, modded_cc_pre);

    let resolved_cc =
        ((base.base_crit_chance + cc_from_sc) * (1.0 + cc) + (base.post_mod_crit_chance + scope_post_cc)).max(0.0);
    let prelude_cd = match base.crit_mult_below_cc {
        Some((bonus, below)) if resolved_cc < below => bonus,
        _ => 0.0,
    };
    for &d in &disabled {
        match d {
            "multishot" => {
                ms = 0.0;
                ms_stack = None;
            }
            "fire_rate" => {
                fr = 0.0;
                fr_on_reload = None;
            }
            "crit_chance" => {
                cc = 0.0;
                cc_on_headshot = None;
                cc_stack = None;
                cc_per_hit = None;
                wp_cc = 0.0;
            }
            "crit_damage" => {
                cd = 0.0;
                cd_on_kill = None;
            }
            "status_chance" => sc = 0.0,
            "base_damage" => {
                bd = 0.0;
                bd_on_reload = None;
                bd_on_eximus_weakpoint = None;
            }
            // A LOCK TAKES THE WINDOW TOO. "Set to its default ignoring other
            // bonuses" cannot mean the static half only — that was the bug the
            // Cannonades taught (MEASUREMENTS M30), and this is the third
            // on-reload buff to need the same line.
            "reload_speed" => {
                rl = 0.0;
                rs_on_reload = 0.0;
            }
            _ => {}
        }
    }

    // The vector build, shared by the direct hit and the radial part: both
    // run the SAME mod math on their OWN base vector (elemental mods are a
    // percentage of THAT part's base damage — MECHANICS §7 "radial attack
    // parts"). Returns (resolved vector, that part's ModifiedBase).
    //
    // Split the (base-damage-scaled) innate vector. Physical IPS stays a fixed
    // component; innate PRIMARY elements (Torid's Toxin, Verglas Prime's Cold)
    // go into their own bucket, which the hierarchy places LAST — the mods
    // combine among themselves and the innate takes what is left over
    // (MECHANICS §3 rule 2; the code used to place them FIRST, which is the
    // superseded draft the doc calls out). They still scale with base-damage
    // mods like the rest of the base. (A physical-innate weapon leaves `input`
    // empty here, so this is a no-op for Dual Toxocyst.)
    // THE MODDED MAGAZINE, computed HERE because on one weapon it is a damage
    // stat and the damage is built below. A charge-backed Incarnon magazine is
    // a fixed resource outside the ammo system, so magazine mods never scale it.
    let mag_size = if base.gauge_form.is_some() {
        base.magazine_size
    } else {
        (base.magazine_size * (1.0 + mag)).floor()
    };
    // …AND WHAT A FULL CHARGE IS WORTH. The Phantasma's alt fire spends the
    // magazine to buy damage — "directly proportional to the amount of ammo
    // consumed" — so a bigger magazine is a longer charge and a bigger bomb.
    // The listed numbers are a full charge of the UNMODDED magazine, which is
    // what the arsenal shows, so this is a ratio against that.
    //
    // Applied to the BASE VECTOR, before the elemental hierarchy: the whole
    // attack is bigger, so ModifiedBase, the elements and every status payload
    // ride along without a second rule. See
    // `weapons_data::AttackSpec::charge_ammo_per_second`.
    let charge_scale = match base.charge_ammo_per_second {
        Some(_) if base.magazine_size > 0.0 => mag_size / base.magazine_size,
        _ => 1.0,
    };
    // ---- PRIMARY COMPRESSION ------------------------------------------
    // The arcane shrinks the explosion to a fifth while aiming and pays for
    // every metre given up. What it is worth is a property of the WEAPON, so
    // the two halves meet HERE and nowhere else: the arcane brings two ramps
    // per METRE, the weapon brings the radius those metres come off and its own
    // row in the published table (docs/CATALOGS.md §2).
    //
    //   radius_lost  = radius_considered × (1 − 0.2)     # continuous
    //   damage_bonus = damage_per_metre(rank) × radius_lost
    //
    // MODDED, not base: the table's Primed Firestorm column is exactly 1.44×
    // its base column on every row that can take the mod, which is the same
    // 1 + br this build already spent on the radius below.
    //
    // AND IT IS AIM-GATED, which is not a footnote on a weapon like this: the
    // whole card reads "on aim", so a scenario whose Tenno is not aiming gets
    // nothing at all rather than a reduced share. Same treatment every
    // `while_aiming` mod gets — the condition is a question about the player.
    let mut compression = None;
    if let Some(c) = base.compression.as_ref().filter(|_| tenno.state.aiming) {
        // WHICH radius. The attack's own, modded — unless the row names one
        // this weapon's data does not carry (the Vectis pair read a 0.1 m embed
        // radial instead of their 6.7 m headshot explosion), in which case the
        // row's metres ARE the answer and `effectiveness` is the transcribed
        // account of how far off that is rather than a second multiplication.
        let attack_radius = base
            .radial
            .as_ref()
            .map(|r| r.radius_m * if r.takes_blast_radius_mods { 1.0 + br } else { 1.0 })
            .or_else(|| base.lingering.as_ref().map(|f| f.radius_m * (1.0 + br)))
            .unwrap_or(0.0);
        let considered = c
            .reads_radius_m
            .unwrap_or(attack_radius * c.effectiveness);
        compression = Some(Compression {
            radius_lost: considered * (1.0 - COMPRESSION_RADIUS_KEPT),
            // THE ROW'S OTHER COLUMN, and the same split Condition Overload
            // has: `adds` joins the base-damage bucket and is diluted by
            // Serration, `multiplies` stands beside it. Most weapons multiply;
            // Ambassador, Battacor, Ferrox, Opticor, Trumna and every
            // Braton/Burston Incarnon add.
            adds: c.stacking == "adds",
        });
    }

    let build = |base_vector: &DamageVector,
                     elem_bonus: Option<&mut Vec<(DamageType, f64)>>|
     -> (DamageVector, f64) {
        let modified_base = base_vector.total() * (1.0 + bd);
        let scale = 1.0 + bd;
        let mut physical = DamageVector::new();
        let mut input = ElementalInput::default();
        for (t, v) in base_vector.iter_nonzero() {
            if t.is_primary_element() {
                input.innate.push((t, v * scale));
            } else {
                // Physical (IPS): base_t × (1 + Σ physical mods) × (1 + base dmg).
                // `scale` carries the base-damage multiplier; the physical bucket is
                // multiplicative with it (wiki Damage/Calculation).
                let pb = phys_bonus.iter().find(|(a, _)| *a == t).map_or(0.0, |(_, x)| *x);
                physical.add(t, v * scale * (1.0 + pb));
            }
        }

        // Mod-added elements append AFTER the innate ones, in mod order (first
        // placement establishes an element's position; later same-element mods
        // merge there).
        for m in mods {
            // ONE MOD, TWO ELEMENTS: THE LAST ONE LISTED GOES FIRST.
            //
            // Wiki, Damage §combining: "the hierarchy priority will be given to
            // the LAST elemental stat listed on the Riven mod" — its worked
            // example is a riven with "+100% Electricity first and +90% Toxin
            // last", where the TOXIN combines with a mod higher up and the
            // Electricity with one lower down. So a mod's own elements enter
            // the hierarchy in REVERSE of how the card prints them.
            //
            // Only a riven can carry two (no mod in `data/mods/` has more than
            // one elemental bonus), so this reverses nothing else — a single
            // element reversed is itself. It is written for MODS rather than
            // for rivens because the rule is about a mod's stat list, and a
            // riven is a mod here by construction.
            //
            // Reported as wrong output (owner, 2026-08-07): a Phantasma Prime
            // with Magnetic / Cold / riven(Toxin, Electricity) / Electricity
            // reads Magnetic + Toxin in game; listed-order pairing gave
            // Viral + Electricity instead, because Cold met the riven's Toxin
            // where the game has it meet the riven's Electricity.
            //
            // The wiki's other half needs no code: "if no other elemental
            // damage mods are present, the elements on the Riven mod will
            // combine with itself" — reversed or not they stay adjacent, so
            // they pair with each other exactly as they did.
            let mut own: Vec<(DamageType, f64)> = Vec::new();
            for e in &m.effects {
                match *e {
                    ModEffect::Element(t, v) => own.push((t, modified_base * v)),
                    ModEffect::CombinedElement(t, v) => {
                        input.direct_secondary.push((t, modified_base * v))
                    }
                    _ => {}
                }
            }
            for (t, v) in own.into_iter().rev() {
                input.push(t, v);
            }
        }
        let mut elem_bonus = elem_bonus;
        for &(t, bonus) in &base.injected_elements {
            input.injected.push((t, modified_base * bonus));
            // The injection "behaves like a Toxin mod, additive with
            // elemental mods" (frenzy.yaml) — so it ALSO raises that
            // element's DoT tick bracket (1 + element bonuses). Recorded
            // once, from the direct part's pass.
            if let Some(eb) = elem_bonus.as_deref_mut() {
                if let Some(x) = eb.iter_mut().find(|(a, _)| *a == t) {
                    x.1 += bonus;
                } else {
                    eb.push((t, bonus));
                }
            }
        }
        (elements::combine(&physical, &input), modified_base)
    };

    let (damage, modified_base) =
        build(&base.base_vector.scale(charge_scale), Some(&mut elem_bonus));
    // The radial part (Laetum Incarnon's 300 Radiation explosion): its own
    // base vector, crit and status stats, modded by the same buckets.
    let radial = base.radial.as_ref().map(|r| {
        // THE EXPLOSION RIDES THE CHARGE TOO. "Damage dealt by the plasma bomb
        // is directly proportional to the amount of ammo consumed" — the bomb
        // IS the explosion on this weapon, and the direct hit is the smaller
        // half of it.
        let (rd, rmb) = build(&r.base_vector.scale(charge_scale), None);
        ResolvedRadial {
            damage: rd,
            modified_base: rmb,
            // The post-mod flat layer (Elemental Excess) is a WEAPON stat
            // change, so the explosion takes it too.
            crit_chance: (r.base_crit_chance * (1.0 + cc) + (base.post_mod_crit_chance + scope_post_cc)).max(0.0),
            crit_damage: r.base_crit_damage * (1.0 + cd),
            base_crit_chance: r.base_crit_chance,
            base_crit_damage: r.base_crit_damage,
            status_chance: (r.base_status_chance * (1.0 + sc) + base.post_mod_status_chance)
                .max(0.0),
            base_status_chance: r.base_status_chance,
            forced_procs: r.forced_procs,
            // Blast RANGE mods scale the radius; the falloff FLOOR is
            // unchanged ("Only mods that increase the explosion radius change
            // how far the falloff reaches; they do not change the floor").
            // THE BUCKET REACHES MOST EXPLOSIONS AND NOT ALL OF THEM. The
            // Shedu's "cannot benefit from Firestorm (Primed) despite being
            // area of effect" is the roster's first exception, and it is worth
            // a branch rather than a comment because Primary Compression pays
            // per metre of this number.
            radius_m: r.radius_m * if r.takes_blast_radius_mods { 1.0 + br } else { 1.0 },
            falloff_start_m: r.falloff_start_m
                * if r.takes_blast_radius_mods { 1.0 + br } else { 1.0 },
            falloff_reduction: r.falloff_reduction,
            takes_condition_overload: r.takes_condition_overload,
            takes_multishot: r.takes_multishot,
            co_base_fraction: r.co_base_fraction(),
        }
    });

    // The lingering FIELD (Torid's Toxin cloud): its own base vector, crit and
    // status stats, through the SAME mod buckets — three patch notes settle
    // that ("Fixed Torid gas clouds not receiving damage buffs from mods";
    // "…the Torid's gas cloud not allowing for criticals"). Tick rate and
    // duration are NOT mod-scaled: fire-rate mods change shots per second, not
    // the cloud's own clock, and the cloud is not a status effect so status
    // duration does not reach it either.
    let lingering = base.lingering.as_ref().map(|f| {
        let (fd, fmb) = build(&f.base_vector, None);
        ResolvedLingering {
            damage: fd,
            modified_base: fmb,
            crit_chance: (f.base_crit_chance * (1.0 + cc) + (base.post_mod_crit_chance + scope_post_cc)).max(0.0),
            crit_damage: f.base_crit_damage * (1.0 + cd),
            status_chance: (f.base_status_chance * (1.0 + sc) + base.post_mod_status_chance)
                .max(0.0),
            base_crit_chance: f.base_crit_chance,
            base_crit_damage: f.base_crit_damage,
            base_status_chance: f.base_status_chance,
            tick_rate: f.tick_rate,
            duration_s: f.duration_s,
            radius_m: f.radius_m * (1.0 + br),
            falloff_start_m: f.falloff_start_m * (1.0 + br),
            falloff_reduction: f.falloff_reduction,
            stacking: f.stacking,
            takes_condition_overload: f.takes_condition_overload,
        }
    });

    // MOD SET bonuses. Every equipped member adds its own share — the set
    // does not have to be complete to be worth carrying (wiki: 5% per
    // Vigilante mod, 30% at six). A mod cannot be equipped twice, so counting
    // members is just counting the mods that name the set.
    let crit_tier_upgrade_chance: f64 = mods
        .iter()
        .filter_map(|m| m.set)
        .filter_map(crate::mod_sets_data::set_def)
        .filter(|s| s.kind == crate::mod_sets_data::SetBonusKind::CritTierUpgrade)
        .map(|s| s.per_mod)
        .sum();

    // DAMAGE FALLOFF, with Projectile Speed moving the whole window — see
    // [`Falloff`] for the two wiki lines that make this the one bucket
    // Projectile Speed pays into. A negative roll can only shorten the window,
    // never invert it, so the scale is floored at zero.
    // THE CONE, with the accuracy mods narrowing it. Wiki (`Accuracy`),
    // verbatim: *"Bonuses that increase accuracy decrease the deviation
    // (spread) of a shot"* — and accuracy is `100 / spread`, so a +30% accuracy
    // card divides the angle by 1.3 rather than subtracting from it.
    //
    // A NEGATIVE roll widens the cone and cannot invert it: the divisor is
    // floored just above zero, and a pinpoint attack (0 / 0) stays pinpoint
    // under any bonus, which is the arithmetic agreeing with the weapon.
    let spread = base.spread.map(|s| {
        let bonus = indirect
            .iter()
            .find(|(st, _)| *st == IndirectStat::Accuracy)
            .map_or(0.0, |(_, v)| *v);
        let k = (1.0 + bonus).max(0.05);
        Spread { min_deg: s.min_deg / k, max_deg: s.max_deg / k }
    });

    let falloff = base.falloff.as_ref().map(|f| {
        let ps = 1.0
            + indirect
                .iter()
                .find(|(s, _)| *s == IndirectStat::ProjectileSpeed)
                .map_or(0.0, |(_, v)| *v);
        let ps = ps.max(0.0);
        Falloff { start_m: f.start_m * ps, end_m: f.end_m * ps, keep: f.reduction }
    });

    ResolvedPanel {
        damage,
        radial,
        spread,
        falloff,
        lingering,
        slash_on_crit,
        crit_tier_upgrade_chance,
        continuous: base.continuous,
        field_duration_on_empty_reload: base.field_duration_on_empty_reload,
        multishot_beyond_range: base.multishot_beyond_range,
        multishot_on_last_round: ms_last_round,
        // Locked the same way: an Acuity says "set to its default ignoring
        // other bonuses", and a bigger base for one burst is a bonus.
        base_multishot_on_last_round: if locked_stat("multishot") {
            0.0
        } else {
            base.base_multishot_on_last_round
        },
        multishot_ammo_bonus: base.multishot_ammo_bonus,
        gauge_form: base.gauge_form,
        // Blast Range reaches the beam's sphere too — wiki: "The 2.3 meter
        // damage radius from the point of impact CAN benefit from Firestorm
        // (Primed)." Same `br` bucket the radial and the field use.
        beam: base.beam.map(|b| BeamGeometry {
            damage_radius_m: b.damage_radius_m * (1.0 + br),
            ..b
        }),
        modified_base,
        // Elemental Excess adds its crit/status FLAT, after the mod
        // multiply (wiki) — a different layer from the base-stat one.
        crit_chance: resolved_cc,
        // A GATED "+Nx Base Critical Damage Multiplier" joins the BASE, so the
        // crit-damage mods multiply it — which is what "Base" earns on the card.
        crit_damage: (base.base_crit_damage + prelude_cd + gate(GatedGrant::BaseCritDamage))
            * (1.0 + cd),
        // What the line above added, in the same post-mod units, so the sim
        // subtracts exactly what was granted — including through a crit-damage
        // LOCK, which zeroes `cd` for both expressions at once.
        crit_mult_below_cc: base
            .crit_mult_below_cc
            .filter(|_| prelude_cd > 0.0)
            .map(|(_, below)| (prelude_cd * (1.0 + cd), below)),
        // No upper clamp: status chance ABOVE 100% is meaningful (a
        // guaranteed proc plus an extra roll) — DT resolves to 129%.
        status_chance: ((base.base_status_chance + sc_from_cc) * (1.0 + sc)
            + base.post_mod_status_chance)
            .max(0.0),
        base_crit_chance: base.base_crit_chance,
        base_crit_damage: base.base_crit_damage,
        base_status_chance: base.base_status_chance,
        fire_rate: base.base_fire_rate * (1.0 + fr + evo_fr_bonus),
        // A charged weapon's fire-rate bonuses DIVIDE the draw instead of
        // multiplying a rate (wiki Fire Rate: on charge weapons the bonus
        // "decreases the charge time"). Same bucket, reciprocal application —
        // and on a bow the bucket already carries the x2.
        // The DRAW's divisor. Charge-rate bonuses always count; fire-rate
        // ones count only where the weapon lets them (an Arch-Gun does not).
        charge_ammo_per_second: base.charge_ammo_per_second,
        sustained_fire_rate: base.sustained_fire_rate,
        battery: base.battery,
        // A MAGAZINE-EATING CHARGE STATES ITS OWN TIME. `magazine / rate`
        // seconds, because that is how long the magazine takes to be spent —
        // so a magazine mod lengthens the charge as well as paying for the
        // damage it buys. Charge-speed bonuses still divide it: they raise the
        // RATE, which is the same thing as shortening the draw.
        charge_seconds: match (base.charge_ammo_per_second, base.charge_seconds) {
            (Some(rate), _) if rate > 0.0 => {
                let from_rate = if base.fire_rate_shortens_draw {
                    fr + evo_fr_bonus
                } else {
                    0.0
                };
                Some(mag_size / rate / (1.0 + cr + from_rate).max(1e-9))
            }
            _ => base.charge_seconds.map(|c| {
            let from_rate = if base.fire_rate_shortens_draw {
                fr + evo_fr_bonus
            } else {
                0.0
            };
            c / (1.0 + cr + from_rate).max(1e-9)
            }),
        },
        // …AND IT COSTS THE WHOLE MAGAZINE. "Charging consumes ammo, up to a
        // full magazine on full charge" — so the shot's price is the magazine
        // it just spent, which is also why a bigger magazine is not free.
        ammo_cost: match base.charge_ammo_per_second {
            Some(rate) if rate > 0.0 => mag_size,
            _ => base.ammo_cost,
        },
        headshot_bonus_multiplicative: base.headshot_bonus_multiplicative,
        charge_cadence: base.charge_cadence,
        // A fire-rate bonus shortens the gap WITHIN a burst as well as the gap
        // between bursts (wiki: it "affect[s] both the speed of the burst as
        // well as the time between bursts"), which is what makes a burst
        // weapon scale linearly like every other gun.
        //
        // The `.max(1.0)` is the wiki's one exception, stated outright: "Burst
        // Delay is not affected by net negative Fire Rate bonuses." So Critical
        // Delay stretches the gap between bursts and leaves the burst itself
        // alone — the weapon keeps more of its rate than the card's number
        // suggests, and only a burst weapon does that.
        burst: base.burst.map(|b| crate::weapons_data::BurstSpec {
            count: b.count,
            delay_seconds: b.delay_seconds / (1.0 + fr + evo_fr_bonus).max(1.0),
        }),
        // THE SCOPE'S OWN HEADSHOT BONUS joins this bracket rather than
        // multiplying it — *"These zoom buffs ... generally stack additively
        // with similar buffs from mods"* (wiki `Sniper Rifle` §Zoom Buffs) —
        // and it is paid only while aiming, for the same reason the combo is.
        // `tenno` here is already the aim-corrected one, so an Incarnon form
        // that cannot zoom pays nothing without knowing why.
        headshot_damage_bonus: base.headshot_damage_bonus
            + if tenno.state.aiming { base.scope_headshot_damage } else { 0.0 },
        noncrit_bonus: base.noncrit_bonus,
        stacking_buffs: base
            .stacking_buffs
            .iter()
            .filter(|b| !locked_stat(b.grant.locked_stat()))
            // A LOCKED STAT TAKES ITS BUFFS WITH IT, and they go rather than
            // going quiet: a card that opens, stacks and grants nothing is a
            // measurement a player cannot make (the rule `check_equip_rules`
            // asserts — "a buff whose only grant is that stat is not offered").
            // Every one of these grants exactly one stat, so the filter is the
            // whole rule.
            .map(|b| StackingBuff {
                per_stack: match b.grant {
                    // ONE conversion: a FireRate buff arrives as a fraction of
                    // the base rate and leaves as the absolute rate it is worth,
                    // because the sim adds it inside the bracket fire-rate mods
                    // live in.
                    BuffGrant::FireRate => base.base_fire_rate * b.per_stack,
                    // …and the same trick for a FLAT base-damage add, which
                    // arrives as the number on the card and leaves as the share
                    // of the base-damage bucket worth the same thing. The mods
                    // are in by now, so `(1 + bd) / base` is a constant — and it
                    // is what preserves the one difference that matters: this
                    // is not diluted by Serration, because the equivalent share
                    // grows with `bd` exactly as fast as the bucket does.
                    BuffGrant::FlatBaseDamage => {
                        let unmodded = base.base_vector.total();
                        if unmodded > 0.0 {
                            b.per_stack * (1.0 + bd) / unmodded
                        } else {
                            0.0
                        }
                    }
                    // …and once more for a BASE crit-damage add, which arrives
                    // as the "+1x" on the card and leaves as the post-mod
                    // multiplier it is worth. Same reason as the two above: the
                    // sim adds it to a total the mods are already inside.
                    BuffGrant::BaseCritDamage => b.per_stack * (1.0 + cd),
                    _ => b.per_stack,
                },
                // …and the same conversion for HOW MANY a trigger grants: 0
                // means "one per shell loaded", which is the modded magazine.
                // It cannot be resolved earlier — the magazine does not exist
                // until the mods are in — and it is what makes a magazine mod
                // a trade rather than a free stat on a by-round reloader.
                stacks_per_trigger: if b.stacks_per_trigger == 0 {
                    mag_size.max(1.0) as u32
                } else {
                    b.stacks_per_trigger
                },
                per_shell: b.stacks_per_trigger == 0,
                // NO OPENING STACKS. It briefly seeded one reload's worth,
                // which was wrong for the same reason Secondary Enervate opens
                // at zero: this is a pile you can be caught without, and the
                // fight is what earns it. An empty magazine takes the whole
                // thing, so "how many you walk in with" is not a state the
                // weapon has — it is a state the last few seconds decided
                // (owner, 2026-08-08).
                initial_stacks: b.initial_stacks,
                ..*b
            })
            .collect(),
        multishot: base.base_multishot * (1.0 + evo_ms_bonus + ms),
        base_multishot: base.base_multishot,
        // Magazine capacity: +% of base, floored to whole rounds (in-game).
        // A charge-backed Incarnon magazine is a fixed resource OUTSIDE the
        // ammo system — magazine mods are inert, so it never scales.
        magazine_size: mag_size,
        // Reserve: +% of base, the same shape as the magazine, and floored
        // for the same reason — a fraction of a round is not a round. The
        // bonus is the Ammo Reserve bucket the panel already shows, which is
        // where Ammo Chain and a riven's Ammo Maximum land.
        ammo_reserve: (base.ammo_reserve
            * (1.0
                + indirect
                    .iter()
                    .find(|(s, _)| *s == IndirectStat::AmmoMax)
                    .map_or(0.0, |(_, v)| *v)))
        .floor(),
        has_reserve: base.has_reserve,
        no_resupply: base.no_resupply,
        super_crit_on_status: base.super_crit_on_status,
        beam_ramp_floor: base.beam_ramp_floor,
        applies_microwave: base.applies_microwave,
        independent_procs: base.independent_procs,
        forced_procs: base.forced_procs.clone(),
        multishot_adds_damage: base.multishot_adds_damage,
        // ONE RESOLVE PER ELEMENT. The recursion terminates because the clone
        // clears `pellet_elements` — and it has to be a re-resolve rather than
        // a retyped result, because an innate element enters the elemental
        // HIERARCHY (MECHANICS §3 rule 2) and a finished vector cannot say
        // which of its Blast the mods put there.
        //
        // The whole base vector becomes `total` of the named element, which is
        // what "each projectile deals one of six elements" means: the missile
        // is that element, not a blend with it.
        pellet_damage: base
            .pellet_elements
            .iter()
            .map(|e| {
                let mut one = base.clone();
                one.pellet_elements = Vec::new();
                one.base_vector = crate::damage::DamageVector::new()
                    .with(*e, base.base_vector.total());
                if let Some(r) = one.radial.as_mut() {
                    r.base_vector =
                        crate::damage::DamageVector::new().with(*e, r.base_vector.total());
                }
                let p = resolve_for(&one, mods, policy, tenno);
                (p.damage, p.radial.map_or_else(DamageVector::new, |r| r.damage))
            })
            .collect(),
        attractor_seconds: base.attractor_seconds,
        tendril_max: base.tendril_max,
        tendril_range_m: base.tendril_range_m,
        tendril_acquire_deg: base.tendril_acquire_deg,
        sniper_combo: if tenno.state.aiming { base.sniper_combo } else { None },
        cc_per_tendril: per_tendril_cc,
        cc_per_hit,
        sc_per_tendril: per_tendril_sc,
        mag_refill_on_kill: mag_refill,
        syndicate_radial,
        // A BY-ROUND RELOAD IS PAID PER SHELL, so it grows with the modded
        // magazine. `mag_size` is the same number the magazine field above
        // reports, which is what keeps the two from disagreeing.
        reload_seconds: match base.by_round_reload {
            Some((start, per, end)) => (start + per * mag_size + end) / (1.0 + rl),
            None => base.base_reload / (1.0 + rl),
        },
        reload_bonus: rl,
        base_damage_bonus: bd,
        // A LOCKED base damage takes this with it, the same rule the live
        // buffs follow: a lock is "set to its default ignoring other bonuses".
        bd_below_half_health: if locked_stat("base_damage") { 0.0 } else { base.bd_below_half_health },
        // CONVERTED, and each by its OWN bucket: "+40% Base Critical Chance" is
        // multiplied by the crit-chance mods and "+2x Base Critical Damage
        // Multiplier" by the crit-damage ones, exactly as the unconditional
        // `flat_base_crit_*` grants beside them are.
        cc_on_undamaged: if locked_stat("critical_chance") { 0.0 } else { base.cc_on_undamaged * (1.0 + cc) },
        cd_on_undamaged: if locked_stat("critical_damage") { 0.0 } else { base.cd_on_undamaged * (1.0 + cd) },
        co_behavior: base.co_behavior,
        compression,
        co_base_fraction: base.co_base_fraction(),
        co_per_type: co,
        co_stack,
        ms_stack,
        cc_on_headshot,
        cc_stack,
        status_damage_mult: 1.0 + sd,
        status_duration_mult: 1.0 + sdur,
        elem_dot_bonus: elem_bonus.into_iter().map(|(t, v)| (t, 1.0 + v)).collect(),
        indirect,
        faction_damage: faction_bonus,
        weakpoint_damage: wp_dmg,
        // RELATIVE; direct-head only, so the sim uses the direct base.
        weakpoint_cc_rel: wp_cc,
        bodyshot_cc_mult: base.bodyshot_cc_mult,
        consecutive_hit_damage: consecutive_hit.or(base.consecutive_hit_damage),
        // OFF ON A CONTINUOUS WEAPON AND ON AN INCARNON FORM, both the mod's
        // own words. `base.form` is the entry being resolved, so an Incarnon
        // half of a cycle drops it while the base half keeps it — which is
        // exactly what "does not have an effect on any Incarnon fire modes,
        // whether on the last shot in their magazine or if activated with one
        // bullet left in the primary mode's magazine" describes.
        last_round_damage: if base.continuous
            || base.form == crate::weapons_data::FormKind::Incarnon
        {
            0.0
        } else {
            last_round_damage
        },
        // The card's numbers converted into POST-MOD units, so the sim adds one
        // term and never has to know about the status bucket. Same units trick
        // `BuffGrant::FlatBaseDamage` and `BaseCritDamage` take.
        derived_status_from_crit: base
            .base_status_from_crit
            .map(|(rate, cap)| (rate * (1.0 + sc), cap * (1.0 + sc), sc_from_cc * (1.0 + sc))),
        derived_crit_from_status: base
            .base_crit_from_status
            .map(|(rate, cap)| (rate * (1.0 + cc), cap * (1.0 + cc), cc_from_sc * (1.0 + cc))),
        round_restore_on_status: base.round_restore_on_status,
        instant_reload_on_kill: base.instant_reload_on_kill,
        mag_growth_on_empty_reload: base.mag_growth_on_empty_reload.map(|(per, max)| {
            // A charge-backed Incarnon magazine is outside the ammo system and
            // the mods never scale it — the same exception `mag_size` makes two
            // hundred lines up, so the grant follows the magazine it grows.
            let scaled = if base.gauge_form.is_some() { per } else { per * (1.0 + mag) };
            (scaled, max)
        }),
        cd_on_kill,
        fr_on_reload,
        rs_on_reload,
        armor_strip_per_puncture: base.armor_strip_per_puncture,
        instant_reload: base.instant_reload_on_headshot,
        headshot_streak: base.headshot_streak,
        cd_below_status_count: base.cd_below_status_count,
        bd_on_reload,
        bd_on_eximus_weakpoint,
        proc_conversion: proc_conv,
        // Reified Bane: the vector already carries the +14 (evolutions apply
        // before mods), so the buff opens FULL and the card scales it back.
        evo_bd: (base.reload_damage_buff > 0.0).then_some(EvoBdBuff {
            full: base.reload_damage_buff,
            without: (base.base_vector.total() - base.reload_damage_buff).max(0.0),
            max_stacks: 1,
            stacks: 1,
        }),
        evo_ms: (evo_ms_bonus > 0.0 && evo_ms_stacks > 0).then_some(
            EvoMsBuff {
                full: base.base_multishot * evo_ms_bonus,
                max_stacks: evo_ms_stacks,
                // Full until a buff card says otherwise — it is permanent, so
                // "the count in play" starts at the count the panel resolved.
                stacks: evo_ms_stacks,
            },
        ),
        locked: disabled.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use DamageType::*;

    /// Verglas Prime, from `data/weapons/sentinel/verglas_prime.yaml` — the
    /// engine's reference elemental-innate weapon (100% Cold(32)), which is
    /// what exercises the innate-element combination branch and the BaseOnly
    /// policy a sentinel forces.
    ///
    /// It was a hand-built struct here while the weapon was ONLY a test
    /// fixture, so its numbers had two homes and only one of them shipped.
    fn verglas_prime() -> WeaponBase {
        WeaponBase::from_data("verglas_prime", true, &[])
    }

    fn m(id: &'static str, effects: Vec<ModEffect>) -> ModDef {
        ModDef {
            exclusive_to: &[],
            unmodeled: false,
            out_of_scope: false,   // a hand-built test mod discloses nothing
            id,
            name: id,
            base_drain: 10,
            max_rank: 10,
            polarity: Polarity::Madurai,
            rarity: Rarity::Common,
            exilus: false,
            family: None,
            requires_weapon: None,
            excludes_weapon: Vec::new(),
            set: None,
            requires: None,
            disables: Vec::new(),
            effects,
        }
    }

    fn m_req(id: &'static str, requires: Option<&'static str>, disables: Vec<&'static str>, effects: Vec<ModEffect>) -> ModDef {
        ModDef { requires, disables, ..m(id, effects) }
    }

    /// The neutral Tenno with one state flipped — how a fight says "hip fire",
    /// "invisible", "airborne". There is one knob shape for all of them now.
    fn tenno_who(f: impl FnOnce(&mut crate::tenno_data::TennoState)) -> crate::tenno_data::Tenno {
        let mut t = crate::tenno_data::default_tenno().clone();
        f(&mut t.state);
        t
    }

    /// Spectral Serration reads "+330% Damage while Invisible", and used to be
    /// a flat `base_damage_bonus` — every shot of every build collected it.
    /// The condition is now a TENNO question: the neutral player in
    /// `data/tenno/` is visible, so the mod contributes nothing, and the same
    /// mod on an invisible Tenno pays in full through the same code path.
    ///
    /// That last half is the point of the seam. Nothing in the engine, the
    /// data, or the UI has to learn about invisibility again when a real frame
    /// arrives — it arrives as a `Tenno` (user, 2026-08-02).
    #[test]
    fn a_player_state_condition_is_asked_of_the_tenno() {
        let base = verglas_prime();
        let ss = crate::mods_data::load_class("rifle")
            .into_iter()
            .find(|d| d.id == "spectral_serration")
            .expect("spectral_serration is in the rifle pool");
        // It loads WRAPPED. Asserting the bare bonus would pass on a build
        // where the gate had been dropped, which is the bug this exists to stop.
        assert!(
            ss.effects.iter().any(|e| matches!(e,
                ModEffect::WhileTenno(TennoCondition::Invisible, inner)
                    if matches!(**inner, ModEffect::BaseDamage(v) if (v - 3.3).abs() < 1e-9))),
            "spectral_serration is gated on invisibility, got {:?}",
            ss.effects
        );

        let plain = resolve(&base, &[], StackPolicy::AssumedMax).modified_base;
        let neutral = crate::tenno_data::default_tenno();
        assert!(!neutral.state.invisible, "the default Tenno is visible");
        assert!(
            (resolve_for(&base, &[&ss], StackPolicy::AssumedMax, neutral).modified_base
                - plain)
                .abs()
                < 1e-9,
            "a visible Tenno collects nothing from a while-Invisible mod"
        );

        let mut hidden = neutral.clone();
        hidden.state.invisible = true;
        let paid =
            resolve_for(&base, &[&ss], StackPolicy::AssumedMax, &hidden).modified_base;
        assert!(
            (paid - plain * 4.3).abs() < 1e-6,
            "an invisible Tenno collects +330%: {paid} vs {}",
            plain * 4.3
        );
    }

    /// ONE STAT DERIVED FROM THE OTHER, and the wiki hands over the arithmetic
    /// to check it with. High Ground reads "Increase Base Critical Chance by 25%
    /// of current Status Chance, up to 35%", and the card's own notes say how
    /// much status chance maxing it takes — "+366.7%" for the non-Incarnon Dera
    /// Vandal, "+536.4%" for the Incarnon Vandal and the non-Incarnon Dera.
    ///
    /// Those two numbers test THREE things at once. 0.35/0.25 = 1.40 is the
    /// current status chance the cap needs, so 0.30 x 4.667 and 0.22 x 6.364
    /// both landing on 1.40 says (a) "CURRENT" is the MODDED value, (b) the rate
    /// and cap are 0.25/0.35, and (c) this roster's base status chances are
    /// 0.30 / 0.22 — which `data/weapons/` carries independently of the note.
    #[test]
    fn a_derived_stat_reads_the_modded_value_and_lands_on_the_base_one() {
        let hg = "dera_vandal_high_ground";
        let base = WeaponBase::from_data("dera_vandal", false, &[hg]);
        assert!(
            (base.base_status_chance - 0.30).abs() < 1e-9,
            "the wiki groups the non-Incarnon Vandal at 30% status chance, got {}",
            base.base_status_chance
        );
        let cc = |sc_bonus: f64| {
            let sm = m("t_sc", vec![ModEffect::StatusChance(sc_bonus)]);
            resolve(&base, &[&sm], StackPolicy::AssumedMax).crit_chance
        };
        // Just under the wiki's threshold the bonus is still climbing; AT it the
        // cap is exactly reached, and past it nothing more is bought.
        let (under, at, over) = (cc(3.60), cc(3.667), cc(6.0));
        assert!(under < at - 1e-9, "below +366.7% the bonus still climbs: {under} vs {at}");
        assert!((at - over).abs() < 1e-9, "+366.7% maxes it out: {at} vs {over}");

        // …and it lands on BASE crit chance, so the crit mods multiply it. The
        // two readings of "Base" only differ once a crit mod is on, which is
        // what this half pins: a 35% base grant through +150% is worth 87.5%.
        let bare = WeaponBase::from_data("dera_vandal", false, &[]);
        let sm = m("t_sc", vec![ModEffect::StatusChance(6.0)]);
        let cm = m("t_cc", vec![ModEffect::CritChance(1.5)]);
        let a = resolve(&bare, &[&sm, &cm], StackPolicy::AssumedMax).crit_chance;
        let b = resolve(&base, &[&sm, &cm], StackPolicy::AssumedMax).crit_chance;
        assert!(
            ((b - a) - 0.35 * 2.5).abs() < 1e-9,
            "a 35% BASE grant is worth 87.5% through a +150% crit mod, got {}",
            b - a
        );

        // THE CARD IS ONE SENTENCE. It was split into a flat "+25% base crit
        // chance" and an inert "of current Status Chance", so modelling the real
        // clause would have paid the perk twice (2026-08-12).
        assert!(
            (a - resolve(&base, &[&cm], StackPolicy::AssumedMax).crit_chance).abs() > 1e-9,
            "sanity: the perk must do something"
        );
        let no_status = m("t_zero", vec![ModEffect::StatusChance(-1.0)]);
        assert!(
            (resolve(&base, &[&no_status, &cm], StackPolicy::AssumedMax).crit_chance
                - resolve(&bare, &[&no_status, &cm], StackPolicy::AssumedMax).crit_chance)
                .abs()
                < 1e-9,
            "at zero status chance the perk grants nothing — there is no flat half"
        );
    }

    /// WITH OVERSHIELDS — the eighth card to ask about the player, and the first
    /// whose grant is not a term added later.
    ///
    /// VERBATIM (Paris_Incarnon_Genesis, Guardian's Might):
    ///   *Increase Base Damage by '''+X'''.
    ///   *With Overshields: Increase Base Damage by '''+Y'''.
    ///   | X = 40<br>Y = 52  | X = 50<br>Y = 40  | X = 20<br>Y = 74
    /// (columns: Paris | Mk1-Paris | Paris Prime, from the table header.)
    ///
    /// The assertion that matters is that the gate changes NOTHING about what
    /// the number means: a Paris Prime holding overshields must be exactly the
    /// weapon a plain "+74" perk would make, down to the base vector's
    /// composition and the explosion's Condition Overload fraction. Both routes
    /// THE ORIGINAL BASE IS AN ABSOLUTE, and this is the case that made it one
    /// (owner, 2026-08-16): TWO FLAT-DAMAGE SOURCES THAT DISAGREE.
    ///
    /// The engine held `co_base_fraction`, a ratio recomputed as
    /// `original / evolved` wherever something raised the panel. One ratio can
    /// describe one verdict — everything feeds, or nothing does — so a build
    /// carrying a perk that feeds the CO term and a perk that does not had no
    /// value it could take. It was never wrong in practice only because no such
    /// build could be assembled: the two flat-damage perks on a weapon are
    /// tier-mates and you pick one. The catalog says the Despair is exactly
    /// that pair (Stalker's Vendetta excluded, Fatal Affliction not), so the
    /// arrangement is one game update away from existing.
    ///
    /// Here it is built by hand, because no roster weapon can express it yet.
    /// A base of 100, one source of +50 that feeds and one of +30 that does
    /// not: the panel reads 180 and the CO term reads 150.
    #[test]
    fn two_flat_sources_that_disagree_each_land_where_they_should() {
        let mut b = WeaponBase::from_data("braton", false, &[]);
        let mut v = DamageVector::new();
        v.add(DamageType::Impact, 100.0);
        b.base_vector = v;
        b.co_base = 100.0;
        b.radial = None;

        b.add_flat_base_damage(50.0, 50.0); // feeds
        b.add_flat_base_damage(30.0, 0.0); // does not

        assert_eq!(b.base_vector.total(), 180.0);
        assert_eq!(b.co_base, 150.0);
        assert!((b.co_base_fraction() - 150.0 / 180.0).abs() < 1e-12);

        // …AND NEITHER RATIO ALONE DESCRIBES IT. `original/evolved` over the
        // whole build is 100/180 and over the feeding source is 150/180; the
        // truth is the second, and the old code could only have reached it by
        // knowing which sources to leave out of a division it performed once.
        assert!((b.co_base_fraction() - 100.0 / 180.0).abs() > 0.2);
    }

    /// …AND THE ORDER THEY ARRIVE IN DOES NOT MATTER, which is what makes the
    /// absolute safe to accumulate. The panel folds pro-rata either way and the
    /// CO base is a sum.
    #[test]
    fn the_original_base_does_not_depend_on_the_order_of_the_sources() {
        let build = |a: (f64, f64), c: (f64, f64)| {
            let mut b = WeaponBase::from_data("braton", false, &[]);
            let mut v = DamageVector::new();
        v.add(DamageType::Impact, 100.0);
        b.base_vector = v;
            b.co_base = 100.0;
            b.radial = None;
            b.add_flat_base_damage(a.0, a.1);
            b.add_flat_base_damage(c.0, c.1);
            (b.base_vector.total(), b.co_base)
        };
        assert_eq!(build((50.0, 50.0), (30.0, 0.0)), build((30.0, 0.0), (50.0, 50.0)));
    }

    /// call `WeaponBase::add_flat_base_damage`, and this is what says so.
    #[test]
    fn a_gated_flat_base_damage_folds_exactly_as_an_ungated_one() {
        let neutral = crate::tenno_data::default_tenno();
        assert!(!neutral.state.overshields, "the default player has none");
        let shielded = tenno_who(|s| s.overshields = true);

        let base = WeaponBase::from_data("paris_prime", true, &["paris_prime_guardians_might"]);
        let off = resolve_for(&base, &[], StackPolicy::AssumedMax, neutral);
        let on = resolve_for(&base, &[], StackPolicy::AssumedMax, &shielded);
        assert!(on.modified_base > off.modified_base,
            "overshields are worth +74 base: {} vs {}", on.modified_base, off.modified_base);

        // THE REFERENCE: the same weapon with the whole +94 as one plain perk.
        // 20 + 74 is what the card pays a player who has them, so a base panel
        // carrying that flat outright must resolve to the same numbers.
        let mut plain = WeaponBase::from_data("paris_prime", true, &[]);
        plain.add_flat_base_damage(20.0 + 74.0, 20.0 + 74.0);
        let want = resolve_for(&plain, &[], StackPolicy::AssumedMax, neutral);
        assert!((on.modified_base - want.modified_base).abs() < 1e-9,
            "gated {} vs plain {}", on.modified_base, want.modified_base);
        // …and the COMPOSITION, not just the total: a pro-rata scale leaves the
        // shares untouched, and getting that wrong moves every status payload
        // while the damage total still reads right.
        assert!((on.damage.total() - want.damage.total()).abs() < 1e-9);
        for ty in crate::damage::DamageType::ALL {
            assert!((on.damage.get(ty) - want.damage.get(ty)).abs() < 1e-9,
                "{ty:?}: gated {} vs plain {}", on.damage.get(ty), want.damage.get(ty));
        }

        // A GATE THAT IS SHUT COSTS NOTHING. The unshielded panel is the weapon
        // with only the perk's unconditional +20 — which is what the card says,
        // and what a build that never picks up an overshield actually gets.
        let mut just_x = WeaponBase::from_data("paris_prime", true, &[]);
        just_x.add_flat_base_damage(20.0, 20.0);
        let x_only = resolve_for(&just_x, &[], StackPolicy::AssumedMax, neutral);
        assert!((off.modified_base - x_only.modified_base).abs() < 1e-9,
            "shut: {} vs {}", off.modified_base, x_only.modified_base);
    }

    /// EVERY CARD THAT ASKS ABOUT OVERSHIELDS, and what each one pays.
    ///
    /// Ten cards across four Genesis families, and the numbers are transcribed
    /// per VARIANT because the wiki prints them per variant — the column order
    /// comes from each page's own table header, which is the part that is easy
    /// to get backwards and impossible to notice afterwards.
    ///
    ///   Angstrum | Prisma Angstrum          one colspan="2" cell, both +50
    ///   Lato | Lato Vandal | Lato Prime      +40 / +40 / +34
    ///   Paris | Mk1-Paris | Paris Prime      +52 / +40 / +74
    ///   Furis | Mk1-Furis                    +30 written into the bullet, both
    ///
    /// Asserted as the DIFFERENCE the state makes, so it reads as the card
    /// does: tick overshields, gain exactly this much base damage.
    #[test]
    fn every_overshield_card_pays_the_number_on_its_own_variant() {
        let roster: &[(&str, &str, f64)] = &[
            ("angstrum", "angstrum_haven_foray", 50.0),
            ("prisma_angstrum", "prisma_angstrum_haven_foray", 50.0),
            ("lato", "lato_haven_foray", 40.0),
            ("lato_vandal", "lato_vandal_haven_foray", 40.0),
            ("lato_prime", "lato_prime_haven_foray", 34.0),
            ("paris", "paris_guardians_might", 52.0),
            ("mk1_paris", "mk1_paris_guardians_might", 40.0),
            ("paris_prime", "paris_prime_guardians_might", 74.0),
            ("furis", "furis_haven_foray", 30.0),
            ("mk1_furis", "mk1_furis_haven_foray", 30.0),
        ];
        let neutral = crate::tenno_data::default_tenno();
        let shielded = tenno_who(|s| s.overshields = true);
        for (weapon, evo, want) in roster {
            let base = WeaponBase::from_data(weapon, true, &[evo]);
            let off = resolve_for(&base, &[], StackPolicy::AssumedMax, neutral);
            let on = resolve_for(&base, &[], StackPolicy::AssumedMax, &shielded);
            // The panel's modified base is the weapon's total through the
            // damage bucket, and with no mods that bucket is 1 — so the
            // difference IS the card's number.
            let got = on.modified_base - off.modified_base;
            assert!((got - want).abs() < 1e-6,
                "{evo}: overshields are worth {want} on the card, the panel moved by {got}");
        }
        // …and the roster is CLOSED: exactly these ten ask, so a new card that
        // spells the condition some other way fails here rather than paying
        // nothing in silence.
        let asking: Vec<&str> = crate::evolutions_data::pool()
            .iter()
            .filter(|e| {
                WeaponBase::from_data(&e.weapon, true, &[e.id.as_str()])
                    .gated
                    .iter()
                    .any(|(g, _, _)| *g == TennoGate::HasOvershields)
            })
            .map(|e| e.id.as_str())
            .collect();
        assert_eq!(asking.len(), roster.len(),
            "cards asking about overshields: {asking:?}");
    }

    /// LONE GUN — the first perk that asks about the LOADOUT, and the first
    /// gated grant that is not damage.
    ///
    /// VERBATIM (Vasto_Incarnon_Genesis, EVO2 Perk 1):
    ///   `* Increase Base Damage by '''+X'''.`
    ///   `* With No Primary Equipped:`
    ///   `** Increase Base Damage by '''+40'''`
    ///   `** Increase Base Magazine Capacity by '''+14'''.`
    ///   `| X = 66   | X = 24`        (Vasto | Vasto Prime)
    ///   `* Increased Base Magazine Capacity does not affect Incarnon Form.`
    ///
    /// So the conditional half is NOT per variant — only X is — which is the
    /// part a per-variant transcription gets wrong by habit.
    ///
    /// The clause used to be `out_of_scope`, on the ruling that the Tenno walks
    /// in with a full loadout. That ruling is still the DEFAULT and now only the
    /// default (owner, 2026-08-13): the scenario says which, and a shut gate
    /// costs exactly nothing, which is what keeps every board row meaning what
    /// it meant.
    #[test]
    fn lone_gun_pays_its_two_halves_only_with_no_other_weapon() {
        let solo = tenno_who(|s| s.solo_weapon = true);
        let neutral = crate::tenno_data::default_tenno();
        for (weapon, evo) in [("vasto", "vasto_lone_gun"), ("vasto_prime", "vasto_prime_lone_gun")]
        {
            let base = WeaponBase::from_data(weapon, true, &[evo]);
            let off = resolve_for(&base, &[], StackPolicy::AssumedMax, neutral);
            let on = resolve_for(&base, &[], StackPolicy::AssumedMax, &solo);
            // With no mods the damage bucket is 1, so the difference IS the
            // card's number — the same reading the overshield roster uses.
            let dmg = on.modified_base - off.modified_base;
            assert!((dmg - 40.0).abs() < 1e-6, "{evo}: base damage moved by {dmg}, card says +40");
            let mag = on.magazine_size - off.magazine_size;
            assert!((mag - 14.0).abs() < 1e-9, "{evo}: magazine moved by {mag}, card says +14");

            // A SHUT GATE COSTS NOTHING: the weapon is its plain +X and nothing
            // else, which is the fight the board is scored under.
            let x = crate::evolutions_data::get(evo).expect(evo).flat_base_damage();
            let mut just_x = WeaponBase::from_data(weapon, true, &[]);
            just_x.add_flat_base_damage(x, x);
            let x_only = resolve_for(&just_x, &[], StackPolicy::AssumedMax, neutral);
            assert!((off.modified_base - x_only.modified_base).abs() < 1e-9,
                "{evo} shut: {} vs {}", off.modified_base, x_only.modified_base);
            assert!((off.magazine_size - x_only.magazine_size).abs() < 1e-9,
                "{evo} shut: magazine {} vs {}", off.magazine_size, x_only.magazine_size);
        }

        // "Increased Base Magazine Capacity does not affect Incarnon Form" —
        // and the DAMAGE half still does, which is why this is asserted on the
        // same panel rather than as "the perk is off in Incarnon Form".
        for (form, evo) in
            [("vasto_incarnon", "vasto_lone_gun"), ("vasto_prime_incarnon", "vasto_prime_lone_gun")]
        {
            let base = WeaponBase::from_data(form, true, &[evo]);
            let off = resolve_for(&base, &[], StackPolicy::AssumedMax, neutral);
            let on = resolve_for(&base, &[], StackPolicy::AssumedMax, &solo);
            assert!((on.magazine_size - off.magazine_size).abs() < 1e-9,
                "{form}: the Incarnon magazine must not move, it went {} -> {}",
                off.magazine_size, on.magazine_size);
            assert!(on.modified_base > off.modified_base,
                "{form}: the damage half still pays in Incarnon Form");
        }

        // …and the roster is CLOSED. Exactly these two cards ask about the
        // loadout, so a third that spells it some other way fails here rather
        // than paying nothing in silence — the same guard the overshield roster
        // carries, and the reason it is worth carrying: the option exists to
        // make these clauses reachable, so one that quietly is not is the whole
        // failure.
        let asking: Vec<&str> = crate::evolutions_data::pool()
            .iter()
            .filter(|e| {
                WeaponBase::from_data(&e.weapon, true, &[e.id.as_str()])
                    .gated
                    .iter()
                    .any(|(g, _, _)| *g == TennoGate::SoloWeapon)
            })
            .map(|e| e.id.as_str())
            .collect();
        assert_eq!(asking, ["vasto_lone_gun", "vasto_prime_lone_gun"],
            "cards asking about the loadout: {asking:?}");
    }

    /// A FIGHT BONUS IS ONE MORE MOD, and that is the whole claim (owner,
    /// 2026-08-13).
    ///
    /// Asserted as an EQUALITY against the real card rather than as a
    /// direction: a scenario's +165% base damage has to resolve to the same
    /// panel Serration does, or "like a mod" is a description of the UI and not
    /// of the arithmetic. Nine buckets, each checked against the mod that owns
    /// it, so a bonus wired to the wrong local fails on the stat it landed in.
    #[test]
    fn a_fight_bonus_resolves_exactly_like_the_mod_of_that_stat() {
        let base = WeaponBase::from_data("torid", true, &[]);
        let pool = crate::mods_data::class_pool("rifle");
        let by = |id: &str| {
            pool.iter().find(|m| m.id == id).unwrap_or_else(|| panic!("{id} missing"))
        };
        let neutral = crate::tenno_data::default_tenno();
        let with_bonus = |f: fn(&mut crate::tenno_data::StatBonuses)| {
            let mut t = neutral.clone();
            f(&mut t.bonuses);
            resolve_for(&base, &[], StackPolicy::Emergent, &t)
        };
        let with_mod = |id: &str| {
            resolve_for(&base, &[by(id)], StackPolicy::Emergent, neutral)
        };
        // (the mod, the bucket setter, and what to read off it)
        type Case = (&'static str, fn(&mut crate::tenno_data::StatBonuses), fn(&ResolvedPanel) -> f64);
        let cases: &[Case] = &[
            ("serration", |b| b.base_damage = 1.65, |p| p.modified_base),
            ("split_chamber", |b| b.multishot = 0.90, |p| p.multishot),
            ("point_strike", |b| b.crit_chance = 1.50, |p| p.crit_chance),
            ("vital_sense", |b| b.crit_damage = 1.20, |p| p.crit_damage),
            ("rifle_aptitude", |b| b.status_chance = 0.90, |p| p.status_chance),
            ("speed_trigger", |b| b.fire_rate = 0.60, |p| p.fire_rate),
            ("magazine_warp", |b| b.magazine = 0.30, |p| p.magazine_size),
        ];
        for (id, set, read) in cases {
            let a = read(&with_mod(id));
            let b = read(&with_bonus(*set));
            assert!((a - b).abs() < 1e-9,
                "{id}: the mod resolves to {a}, the same number as a fight bonus resolves to {b}");
        }
        // RELOAD IS THE OTHER DIRECTION — a bigger bucket is a SHORTER time —
        // so it is read separately rather than being one more row above.
        let m = with_mod("fast_hands").reload_seconds;
        let f = with_bonus(|b| b.reload_speed = 0.30).reload_seconds;
        assert!((m - f).abs() < 1e-9, "fast hands {m}s vs a fight bonus {f}s");

        // …AND THEY ADD, which is what "one more mod" means when there is
        // already one: Serration + a +165% fight bonus is one bucket at +330%,
        // never two multipliers.
        let mut both = neutral.clone();
        both.bonuses.base_damage = 1.65;
        let stacked = resolve_for(&base, &[by("serration")], StackPolicy::Emergent, &both);
        let plain = resolve_for(&base, &[], StackPolicy::Emergent, neutral);
        assert!((stacked.modified_base / plain.modified_base - 4.30).abs() < 1e-9,
            "1 + 1.65 + 1.65 = 4.30, got x{}", stacked.modified_base / plain.modified_base);

        // …AND A LOCK STILL WINS. "Set to its default ignoring other bonuses"
        // makes no exception for where a bonus came from, and the fight is not
        // a loophole in a rule the mods obey.
        let mut ms = neutral.clone();
        ms.bonuses.multishot = 5.0;
        let locked = resolve_for(&base, &[by("primary_acuity")], StackPolicy::Emergent, &ms);
        let unlocked = resolve_for(&base, &[], StackPolicy::Emergent, neutral);
        assert!((locked.multishot - unlocked.multishot).abs() < 1e-9,
            "a locked multishot ignores a fight bonus too: {} vs {}",
            locked.multishot, unlocked.multishot);
    }

    /// WITH A CHANNELED ABILITY ACTIVE — the second player-declared state, and
    /// the family where the conditional half is the BIGGER one.
    ///
    /// VERBATIM (Braton_Incarnon_Genesis, Daring Reverie):
    ///   * Increase Base Damage by '''+X'''.
    ///   * With [[Channeled Abilities|Channeled Ability]] active: Increase Base
    ///     Damage by '''+Y'''. '''+50%''' Ammo Efficiency
    ///     | X = 24<br>Y = 30 | X = 28<br>Y = 22 | X = 12<br>Y = 34 | X = 4<br>Y = 38
    ///
    /// THE COLUMNS COME FROM THE TABLE HEADER — Braton | Mk1-Braton | Braton
    /// Vandal | Braton Prime — and NOT from the page's opening sentence, which
    /// lists the same four in a different order. Reading the sentence makes
    /// three of the four look wrong, so the mapping is pinned here.
    #[test]
    fn every_channeled_ability_card_pays_the_number_on_its_own_variant() {
        let roster: &[(&str, &str, f64, f64)] = &[
            ("braton", "braton_daring_reverie", 24.0, 30.0),
            ("mk1_braton", "mk1_braton_daring_reverie", 28.0, 22.0),
            ("braton_vandal", "braton_vandal_daring_reverie", 12.0, 34.0),
            ("braton_prime", "braton_prime_daring_reverie", 4.0, 38.0),
        ];
        let neutral = crate::tenno_data::default_tenno();
        assert!(!neutral.state.channeling, "the default player is casting nothing");
        let channeling = tenno_who(|s| s.channeling = true);
        for (weapon, evo, x, y) in roster {
            let bare = WeaponBase::from_data(weapon, true, &[]);
            let with = WeaponBase::from_data(weapon, true, &[evo]);
            let off = resolve_for(&with, &[], StackPolicy::AssumedMax, neutral);
            let on = resolve_for(&with, &[], StackPolicy::AssumedMax, &channeling);
            let plain = resolve_for(&bare, &[], StackPolicy::AssumedMax, neutral);
            // The unconditional half is X…
            assert!((off.modified_base - plain.modified_base - x).abs() < 1e-6,
                "{evo}: the unconditional half is +{x}, panel moved by {}",
                off.modified_base - plain.modified_base);
            // …and ticking the state adds Y on top of it.
            assert!((on.modified_base - off.modified_base - y).abs() < 1e-6,
                "{evo}: a channeled ability is worth +{y}, panel moved by {}",
                on.modified_base - off.modified_base);
        }
        // THE CONDITIONAL HALF IS THE BIGGER ONE for three of the four, which is
        // the fact a player needs before reading an unticked Braton as its
        // ceiling — and a sign-flipped transcription would break it.
        assert_eq!(roster.iter().filter(|(_, _, x, y)| y > x).count(), 3);
    }

    /// A FORM THAT CANNOT ZOOM CANNOT BE AIMING, so every aim-gated bonus pays
    /// nothing in it — and the SAME WEAPON's base form is unaffected.
    ///
    /// VERBATIM (Vasto_Incarnon_Genesis): "Incarnon Form transforms into a
    /// 6-round burst with '''6''' base [[multishot]] … has significantly higher
    /// [[Recoil]], and cannot [[Zoom]]."
    ///
    /// "Zoom" is the wiki's word for the aim STATE — its page opens "Zoom (or
    /// aiming, aiming down sights (ADS))" and the Galvanized mods write the
    /// condition as `[[Zoom|aiming]]` — and DE settled the consequence in a
    /// patch note about Mesa's Regulators: the buffs "never actually applied
    /// due to the 'on aim' criteria not being fulfilled".
    ///
    /// The scenario is left ALONE. A player who ticks "aiming" is not corrected
    /// and their other weapons still aim; the form answers the question for
    /// itself, which is why this is on the weapon and not on the Tenno.
    #[test]
    fn a_form_that_cannot_zoom_pays_no_aim_gated_bonus() {
        use crate::mods_data::class_pool;
        let pool = class_pool("pistol");
        let gc = pool.iter().find(|m| m.id == "galvanized_crosshairs")
            .expect("galvanized_crosshairs is in the pistol pool");
        let aiming = crate::tenno_data::default_tenno();
        assert!(aiming.state.aiming, "the default player aims");
        let hipfire = tenno_who(|s| s.aiming = false);

        let cc = |id: &str, t: &crate::tenno_data::Tenno| {
            let base = WeaponBase::from_data(id, false, &[]);
            resolve_for(&base, &[gc], StackPolicy::AssumedMax, t).crit_chance
        };

        // THE BASE FORM is an ordinary weapon: aiming is worth something and
        // hipfiring is not.
        let (base_aim, base_hip) = (cc("vasto_prime", aiming), cc("vasto_prime", &hipfire));
        assert!(base_aim > base_hip,
            "the base form pays the aim mod: {base_aim} vs {base_hip}");

        // THE INCARNON FORM cannot zoom, so the two are the same number — the
        // mod is equipped, resolves, and grants nothing.
        let (inc_aim, inc_hip) = (cc("vasto_prime_incarnon", aiming),
                                  cc("vasto_prime_incarnon", &hipfire));
        assert!((inc_aim - inc_hip).abs() < 1e-9,
            "cannot Zoom means the aim mod pays nothing: aiming {inc_aim}, hipfire {inc_hip}");

        // …and it is the FLAG doing it, not the weapon happening to ignore crit
        // mods: a weapon whose form CAN zoom still pays.
        let (lex_aim, lex_hip) = (cc("lex_prime_incarnon", aiming),
                                  cc("lex_prime_incarnon", &hipfire));
        assert!(lex_aim > lex_hip,
            "an Incarnon form that CAN zoom still pays it: {lex_aim} vs {lex_hip}");

        // THE ROSTER IS CLOSED at the two Vastos — the only "cannot Zoom" in the
        // whole Incarnon Evolutions page. The four "-30% Zoom" perks there cut
        // magnification and leave the aim state alone, so they must not appear.
        let flagged: Vec<&str> = crate::weapons_data::all().iter()
            .filter(|w| w.cannot_zoom).map(|w| w.id.as_str()).collect();
        assert_eq!(flagged, vec!["vasto_incarnon", "vasto_prime_incarnon"], "{flagged:?}");
    }

    /// A SCOPE GRANTS ONE OF THREE THINGS, AND THEY LAND IN THREE PLACES.
    ///
    /// The Vectis family's zoom gives headshot damage, the Rubico's a critical
    /// MULTIPLIER, the Lanka's a critical CHANCE — and the Lanka's is the one
    /// the mechanic page calls an exception: *"The zoom bonus adds a flat
    /// +20/30/50 critical chance, applied after mods"*. That is a different
    /// layer from the other two. A relative +50% on the Lanka's 25% base is
    /// five points; the flat one is fifty, and putting it in the ordinary
    /// bucket would have understated the weapon by an order of magnitude.
    #[test]
    fn a_scopes_three_kinds_land_in_three_buckets() {
        let refs: Vec<&ModDef> = Vec::new();
        let mut hip = crate::tenno_data::default_tenno().clone();
        hip.state.aiming = false;
        let pair = |id: &str| {
            let b = WeaponBase::from_data(id, false, &[]);
            (
                resolve(&b, &refs, StackPolicy::Emergent),
                resolve_for(&b, &refs, StackPolicy::Emergent, &hip),
            )
        };

        // THE RUBICO: +50% critical multiplier, relative, so 3.0x -> 4.5x.
        let (aim, no) = pair("rubico");
        assert!((no.crit_damage - 3.0).abs() < 1e-9, "hip: {}", no.crit_damage);
        assert!((aim.crit_damage - 4.5).abs() < 1e-9, "scoped: {}", aim.crit_damage);

        // THE LANKA: fifty POINTS of crit chance, not half of its 25%.
        let (aim, no) = pair("lanka");
        assert!((no.crit_chance - 0.25).abs() < 1e-9, "hip: {}", no.crit_chance);
        assert!(
            (aim.crit_chance - 0.75).abs() < 1e-9,
            "the Lanka's scope is a FLAT +50 applied after mods, so 25% becomes 75%              and not 37.5%: {}",
            aim.crit_chance
        );

        // THE VULKAR: +70% headshot damage, the Vectis family's kind.
        let (aim, no) = pair("vulkar");
        assert!((aim.headshot_damage_bonus - no.headshot_damage_bonus - 0.7).abs() < 1e-9);
        // ...and none of the three touches a weapon without a scope.
        let (aim, no) = pair("torid");
        assert!((aim.crit_chance - no.crit_chance).abs() < 1e-9);
        assert!((aim.crit_damage - no.crit_damage).abs() < 1e-9);
    }

    /// BOTH SNIPER MECHANICS ARE THE SCOPE'S. *"Building combo and benefiting
    /// from its multiplier requires being scoped in"* (wiki `Sniper Rifle`),
    /// and the zoom buff is a property of a zoom level — so a hip-fired
    /// scenario gets neither, and it is `resolve` that says so, once, for the
    /// simulator and the optimizer and the board's no-aim ruler alike.
    #[test]
    fn a_sniper_fired_from_the_hip_has_no_combo_and_no_scope() {
        let base = WeaponBase::from_data("vectis_prime", false, &[]);
        let refs: Vec<&ModDef> = Vec::new();
        let mut hip = crate::tenno_data::default_tenno().clone();
        hip.state.aiming = false;

        let aimed = resolve(&base, &refs, StackPolicy::Emergent);
        let from_hip = resolve_for(&base, &refs, StackPolicy::Emergent, &hip);

        assert_eq!(aimed.sniper_combo.map(|c| c.min), Some(5), "scoped in, it has one");
        assert!(from_hip.sniper_combo.is_none(), "from the hip it has none");
        // ...and the scope's headshot bonus travels with it, in the additive
        // bracket the wiki puts it in.
        assert!(
            (aimed.headshot_damage_bonus - from_hip.headshot_damage_bonus - 0.6).abs() < 1e-12,
            "the scope is worth +60% headshot damage and only while aiming: {} vs {}",
            aimed.headshot_damage_bonus,
            from_hip.headshot_damage_bonus
        );
    }


    /// A DERIVED STAT READS THE FORM IT IS ON — which is the in-mission
    /// behaviour, and NOT the one the Arsenal shows.
    ///
    /// VERBATIM (Sicarus_Incarnon_Genesis, Wiseman's Regard):
    ///   * Incarnon Form Status Chance is displayed in the [[Arsenal]] screen as
    ///     if using the normal form's Critical Chance, but will properly use
    ///     its' own Critical chance while in-mission.
    ///
    /// So the panel must NOT reproduce the Arsenal's bug: each form's status
    /// chance comes from THAT form's crit chance. Free here, because a form is
    /// its own weapon entry and resolves its own panel — which is exactly why
    /// it is worth pinning, since nothing else would notice if it stopped.
    ///
    /// FOUR NUMBERS OUT OF ONE SENTENCE. The same row states what it takes to
    /// max the conversion: "achievable with '''+734%''' (Sicarus) / '''+434%''' (Prime)
    /// modded Critical Chance, or '''+567%''' (Sicarus) / '''+345%''' (Prime) in
    /// Incarnon Form". The cap needs 0.40/0.30 = 1.3333 current crit, so each
    /// threshold implies that form's BASE crit chance — 1.3333/8.34 = 0.160,
    /// /6.67 = 0.200, /5.34 = 0.250, /4.45 = 0.300 — and `data/weapons/` was
    /// written from the weapon pages, so the two sources meet here.
    #[test]
    fn wisemans_regard_reads_each_forms_own_crit_chance() {
        let rows: &[(&str, &str, f64, f64)] = &[
            // (entry, perk, that form's base crit, the wiki's "+X%" threshold)
            ("sicarus", "sicarus_wisemans_regard", 0.16, 7.34),
            ("sicarus_incarnon", "sicarus_wisemans_regard", 0.20, 5.67),
            ("sicarus_prime", "sicarus_prime_wisemans_regard", 0.25, 4.34),
            ("sicarus_prime_incarnon", "sicarus_prime_wisemans_regard", 0.30, 3.45),
        ];
        for (id, perk, base_cc, threshold) in rows {
            let bare = WeaponBase::from_data(id, false, &[]);
            let with = WeaponBase::from_data(id, false, &[perk]);
            let b = resolve(&bare, &[], StackPolicy::AssumedMax);
            let w = resolve(&with, &[], StackPolicy::AssumedMax);

            assert!((w.crit_chance - base_cc).abs() < 1e-9,
                "{id}: the wiki's +{:.0}% threshold implies a base crit of {base_cc}, ours is {}",
                threshold * 100.0, w.crit_chance);
            // THE CONVERSION IS OFF THIS FORM'S OWN NUMBER, not the base form's.
            let got = w.status_chance - b.status_chance;
            assert!((got - 0.30 * base_cc).abs() < 1e-9,
                "{id}: 30% of {base_cc} is {}, the panel moved by {got}", 0.30 * base_cc);
            // …and the threshold reproduces the cap, which is the other half of
            // the sentence: at +X% modded crit the conversion is exactly 40%.
            let capped = (0.30 * base_cc * (1.0 + threshold)).min(0.40);
            assert!((capped - 0.40).abs() < 0.002,
                "{id}: at +{:.0}% the conversion should sit on the 40% cap, got {capped}",
                threshold * 100.0);
        }

        // THE ARSENAL'S BUG, as a negative control: the Incarnon form's answer
        // must NOT equal what the base form's crit would give it. On the Prime
        // that is 0.090 against 0.075, so a regression to the base form's
        // number is a visible 1.5-point difference and not a rounding one.
        let inc = resolve(
            &WeaponBase::from_data("sicarus_prime_incarnon", false,
                &["sicarus_prime_wisemans_regard"]), &[], StackPolicy::AssumedMax);
        let inc_bare = resolve(
            &WeaponBase::from_data("sicarus_prime_incarnon", false, &[]),
            &[], StackPolicy::AssumedMax);
        let base_form_would_give = 0.30 * 0.25;
        assert!(((inc.status_chance - inc_bare.status_chance) - base_form_would_give).abs() > 1e-6,
            "the Incarnon form is using the BASE form's crit chance — the Arsenal's bug");
    }

    /// The sim used to satisfy `while_aiming` silently, so every aim-gated
    /// buff fired whether or not the scenario implied aiming (user,
    /// 2026-07-30). `resolve_with(.., aiming)` is the knob; `resolve` keeps
    /// assuming aim.
    #[test]
    fn aiming_gates_the_while_aiming_effects_and_only_those() {
        use crate::mods_data::class_pool;
        let pool = class_pool("pistol");
        let by = |id: &str| pool.iter().find(|m| m.id == id).expect(id);
        let base = WeaponBase::from_data("dual_toxocyst", true, &[]);
        let hipfire = tenno_who(|s| s.aiming = false);

        // Galvanized Crosshairs is entirely aim-gated: dropping aim must
        // remove its whole crit-chance contribution.
        let gc = vec![by("galvanized_crosshairs")];
        let aimed = resolve(&base, &gc, StackPolicy::AssumedMax);
        let hip = resolve_for(&base, &gc, StackPolicy::AssumedMax, &hipfire);
        assert!(
            aimed.crit_chance > hip.crit_chance,
            "aim-gated crit must vanish: {} vs {}",
            aimed.crit_chance,
            hip.crit_chance
        );
        let bare = resolve_for(&base, &[], StackPolicy::AssumedMax, &hipfire);
        assert!(
            (hip.crit_chance - bare.crit_chance).abs() < 1e-12,
            "with no aim the mod contributes NOTHING, not something reduced"
        );

        // An UNGATED mod is untouched by the flag - the wrapper must not leak.
        let hs = vec![by("hornet_strike")];
        let a2 = resolve(&base, &hs, StackPolicy::AssumedMax);
        let h2 = resolve_for(&base, &hs, StackPolicy::AssumedMax, &hipfire);
        assert!(
            (a2.modified_base - h2.modified_base).abs() < 1e-12,
            "Hornet Strike does not care about aiming"
        );

        // And the plain entry point still assumes aim (30 callers rely on it).
        let legacy = resolve(&base, &gc, StackPolicy::AssumedMax);
        assert!((legacy.crit_chance - aimed.crit_chance).abs() < 1e-12);
    }

    #[test]
    fn requires_gate_disables_and_cond_buff() {
        let base = WeaponBase::from_data("dual_toxocyst", true, &["dual_toxocyst_commodores_fortune", "dual_toxocyst_evolved_autoloader", "dual_toxocyst_fevered_frenzy"]); // traits: semi_auto
        let p0 = resolve(&base, &[], StackPolicy::AssumedMax);
        // requires: a mod needing `beam` is INERT on Dual Toxocyst (no beam);
        // a mod needing `semi_auto` applies.
        let beam_mod = m_req("beam", Some("beam"), vec![], vec![ModEffect::BaseDamage(3.0)]);
        assert!((resolve(&base, &[&beam_mod], StackPolicy::AssumedMax).modified_base - p0.modified_base).abs() < 1e-9);
        let semi = m_req("semi", Some("semi_auto"), vec![], vec![ModEffect::BaseDamage(3.0)]);
        assert!(resolve(&base, &[&semi], StackPolicy::AssumedMax).modified_base > p0.modified_base);
        // disables: a mod locking `multishot` sets it to the weapon's DEFAULT.
        //
        // Not "voids other multishot MODS" — the card says "set weapon's
        // Multishot to its default ignoring other bonuses, even negative
        // effects" (wiki, Primary/Pistol Acuity), and this base carries Fevered
        // Frenzy, whose permanent stacked multishot never passed through the mod
        // bucket. It used to survive the lock, so an Acuity build on Dual
        // Toxocyst kept the evolution's pellets (user, 2026-08-04).
        let lock = m_req("acuity", None, vec!["multishot"], vec![]);
        let ms_mod = m("ms", vec![ModEffect::Multishot(1.0)]);
        let locked = resolve(&base, &[&ms_mod, &lock], StackPolicy::AssumedMax);
        assert!((locked.multishot - base.base_multishot).abs() < 1e-9, "the weapon's default");
        assert!(locked.evo_ms.is_none(), "and the evolution's buff card goes with it");
        // ...and the evolution IS worth something without the lock, so the line
        // above is not passing because it contributes nothing.
        assert!(p0.multishot > base.base_multishot + 1e-9, "Fevered Frenzy pays unlocked");
        assert_eq!(locked.locked, vec!["multishot"], "the panel states the lock");
        // CondBuff: contributes under AssumedMax, nothing under Emergent.
        let cb = m("cond", vec![ModEffect::CondBuff(CondBucket::StatusChance, 0.90)]);
        let amax = resolve(&base, &[&cb], StackPolicy::AssumedMax);
        let emerg = resolve(&base, &[&cb], StackPolicy::Emergent);
        assert!((amax.status_chance - base.base_status_chance * 1.90).abs() < 1e-9);
        assert!((emerg.status_chance - base.base_status_chance).abs() < 1e-9);
    }

    /// A LOCK REACHES A LIVE BUFF, not just the static buckets.
    ///
    /// Acuity "set[s] weapon's Multishot to its default ignoring other bonuses,
    /// even negative effects" — and a buff earned DURING the fight is a bonus
    /// like any other, so Stormburst's "+0.4 Multishot for 2s, stacks 3x" is
    /// worth nothing under one (owner, 2026-08-11). It was worth +1.2: the
    /// resolver knew about locks in exactly one arm, `BuffGrant::FireRate`, and
    /// a buff that fed any other stat walked straight past it.
    ///
    /// The pair below is the whole point — the same build, once locked and once
    /// not — because a filter that removed the buff unconditionally would pass
    /// the first half on its own.
    #[test]
    fn a_multishot_lock_removes_a_live_multishot_buff_too() {
        let base = WeaponBase::from_data("furis", true, &["furis_stormburst"]);
        let ms_of = |b: &ResolvedPanel| {
            b.stacking_buffs
                .iter()
                .filter(|s| s.grant == BuffGrant::Multishot)
                .map(|s| s.per_stack * s.max_stacks as f64)
                .sum::<f64>()
        };
        let free = resolve(&base, &[], StackPolicy::Emergent);
        assert!(
            (ms_of(&free) - 1.2).abs() < 1e-9,
            "unlocked, Stormburst is worth +1.2 multishot at three stacks: {}",
            ms_of(&free)
        );

        let lock = m_req("acuity", None, vec!["multishot"], vec![]);
        let locked = resolve(&base, &[&lock], StackPolicy::Emergent);
        assert_eq!(ms_of(&locked), 0.0, "and nothing at all under the lock");
        assert!(
            !locked.stacking_buffs.iter().any(|s| s.grant == BuffGrant::Multishot),
            "the buff is not offered — a card that stacks and grants nothing is              a measurement nobody can make"
        );
        // The lock is about ONE stat: this evolution's OTHER half, a flat +28
        // base damage, is untouched.
        let bare = WeaponBase::from_data("furis", true, &[]);
        assert!(
            locked.modified_base > resolve(&bare, &[&lock], StackPolicy::Emergent).modified_base,
            "the +28 survives — a lock is not a way to switch an evolution off"
        );
    }

    /// A FIRE-RATE PENALTY DOES NOT STRETCH THE BURST ITSELF — the wiki's one
    /// exception, stated outright: *"Burst Delay is not affected by net
    /// negative Fire Rate bonuses."*
    ///
    /// So Critical Delay costs a Burston Prime less than its card says. The
    /// gap between bursts stretches in full, the two 0.04 s gaps inside the
    /// burst do not, and the weapon keeps rate the number does not account
    /// for. This is the ONLY place a burst weapon is more than an auto weapon
    /// relabelled at its effective rate, which is why it gets its own test.
    #[test]
    fn a_fire_rate_penalty_leaves_a_burst_weapon_s_own_delay_alone() {
        let base = WeaponBase::from_data("burston_prime", false, &[]);
        let listed = base.burst.expect("burston prime declares a burst").delay_seconds;
        assert!((listed - 0.04).abs() < 1e-9, "the module's BurstDelay");

        // Critical Delay: -36% fire rate, and nothing else that matters here.
        let slow = [m("critical_delay", vec![ModEffect::FireRate(-0.36)])];
        let refs: Vec<&ModDef> = slow.iter().collect();
        let p = resolve(&base, &refs, StackPolicy::AssumedMax);
        assert!(
            (p.burst.unwrap().delay_seconds - listed).abs() < 1e-9,
            "a NET NEGATIVE fire-rate bonus must leave the burst delay alone"
        );
        // ...while the rate itself takes the penalty in full.
        assert!((p.fire_rate - 5.0 * 0.64).abs() < 1e-9);

        // A POSITIVE bonus shortens BOTH — "Fire Rate bonuses affect both the
        // speed of the burst as well as the time between bursts" — which is
        // what makes a burst weapon scale linearly like every other gun. Shred
        // is +30%.
        let fast = [m("shred", vec![ModEffect::FireRate(0.30)])];
        let refs: Vec<&ModDef> = fast.iter().collect();
        let q = resolve(&base, &refs, StackPolicy::AssumedMax);
        assert!((q.burst.unwrap().delay_seconds - 0.04 / 1.30).abs() < 1e-9);
        assert!((q.fire_rate - 5.0 * 1.30).abs() < 1e-9);
    }

    #[test]
    fn dual_toxocyst_panel_resolves_by_hand() {
        // Hornet +220%, Frostbite (cold 60/SC 60), Jolt (elec 60/SC 60),
        // PPG +187% cc, PTC +110% cd, Lethal Torrent (FR 60/MS 60),
        // Galvanized Shot (+80% SC, CO 0.4×3), Galvanized Diffusion
        // (+110% MS, +30%×4 on kill).
        let mods = [
            m("hornet", vec![ModEffect::BaseDamage(2.20)]),
            m(
                "frostbite",
                vec![
                    ModEffect::Element(Cold, 0.60),
                    ModEffect::StatusChance(0.60),
                ],
            ),
            m(
                "jolt",
                vec![
                    ModEffect::Element(Electricity, 0.60),
                    ModEffect::StatusChance(0.60),
                ],
            ),
            m("ppg", vec![ModEffect::CritChance(1.87)]),
            m("ptc", vec![ModEffect::CritDamage(1.10)]),
            m(
                "lt",
                vec![ModEffect::FireRate(0.60), ModEffect::Multishot(0.60)],
            ),
            m(
                "gshot",
                vec![
                    ModEffect::StatusChance(0.80),
                    ModEffect::ConditionOverload {
                        per_stack: 0.40,
                        max_stacks: 3,
                        duration: 14.0,
                    },
                ],
            ),
            m(
                "gdiff",
                vec![
                    ModEffect::Multishot(1.10),
                    ModEffect::OnKillMultishot {
                        per_stack: 0.30,
                        max_stacks: 4,
                        duration: 20.0,
                    },
                ],
            ),
        ];
        let refs: Vec<&ModDef> = mods.iter().collect();
        let p = resolve(
            &WeaponBase::from_data("dual_toxocyst_incarnon", false, &["dual_toxocyst_commodores_fortune", "dual_toxocyst_evolved_autoloader", "dual_toxocyst_fevered_frenzy"]),
            &refs,
            StackPolicy::AssumedMax,
        );

        // ModifiedBase: 125 × 3.2 = 400.
        assert!((p.modified_base - 400.0).abs() < 1e-9);
        // Physical keeps its 3.2×; Cold+Electricity (adjacent) -> Magnetic
        // of 2 × 0.6 × 400 = 480; total = 400 + 480 = 880.
        assert!((p.damage.get(Magnetic) - 480.0).abs() < 1e-9);
        assert!((p.damage.get(Puncture) - 200.0).abs() < 1e-9);
        assert!((p.damage.total() - 880.0).abs() < 1e-9);
        // cc 0.31 × 2.87; cd 3 × 2.1; sc 0.43 × 3.0; fr 4.5 × 1.6.
        assert!((p.crit_chance - 0.8897).abs() < 1e-9);
        assert!((p.crit_damage - 6.3).abs() < 1e-9);
        assert!((p.status_chance - 1.29).abs() < 1e-9);
        assert!((p.fire_rate - 7.2).abs() < 1e-9);
        // MS: 1 × (1 + 1.0 Fevered + 0.6 + 1.1 + 1.2 stacks) = 4.9.
        assert!((p.multishot - 4.9).abs() < 1e-9);
        // CO assumed max: 0.4 × 3 = 1.2. No status-damage mods.
        assert!((p.co_per_type - 1.2).abs() < 1e-9);
        assert!((p.status_damage_mult - 1.0).abs() < 1e-9);
        // Elemental DoT brackets: cold 1.6, electricity 1.6.
        assert!(p.elem_dot_bonus.contains(&(Cold, 1.6)));
        assert!(p.elem_dot_bonus.contains(&(Electricity, 1.6)));
    }

    #[test]
    fn faction_bonuses_merge_additively_by_faction() {
        // Two Grineer faction sources (Expel + a hypothetical Bane) add within
        // the Grineer bucket; a Corpus source is its own entry. Wiki: faction
        // bonuses are additive with each other (Bane + Roar share a bracket).
        let mods = [
            m("expel_grineer", vec![ModEffect::FactionDamage(Faction::Grineer, 0.30)]),
            m("bane_grineer", vec![ModEffect::FactionDamage(Faction::Grineer, 0.55)]),
            m("expel_corpus", vec![ModEffect::FactionDamage(Faction::Corpus, 0.30)]),
        ];
        let refs: Vec<&ModDef> = mods.iter().collect();
        let p = resolve(
            &WeaponBase::from_data("dual_toxocyst_incarnon", false, &["dual_toxocyst_commodores_fortune", "dual_toxocyst_evolved_autoloader", "dual_toxocyst_fevered_frenzy"]),
            &refs,
            StackPolicy::AssumedMax,
        );
        let bonus = |f: Faction| p.faction_damage.iter().find(|(x, _)| *x == f).map(|(_, v)| *v);
        assert!((bonus(Faction::Grineer).unwrap() - 0.85).abs() < 1e-9);
        assert!((bonus(Faction::Corpus).unwrap() - 0.30).abs() < 1e-9);
    }

    #[test]
    fn magazine_and_status_duration_buckets_resolve() {
        let base = WeaponBase::from_data("dual_toxocyst", true, &["dual_toxocyst_commodores_fortune", "dual_toxocyst_evolved_autoloader", "dual_toxocyst_fevered_frenzy"]);
        let baseline = resolve(&base, &[], StackPolicy::AssumedMax).magazine_size;
        let mods = [
            m("mag", vec![ModEffect::MagazineCapacity(0.60)]),   // +60% of base
            m("lasting", vec![ModEffect::StatusDuration(0.40)]),
        ];
        let refs: Vec<&ModDef> = mods.iter().collect();
        let p = resolve(&base, &refs, StackPolicy::AssumedMax);
        // Magazine capacity is +% of base, floored to whole rounds.
        assert!((p.magazine_size - (baseline * 1.60).floor()).abs() < 1e-9);
        assert!((p.status_duration_mult - 1.40).abs() < 1e-9);
    }

    #[test]
    fn physical_mod_scales_base_of_its_type_not_the_element_hierarchy() {
        // A +90% Impact physical mod scales the BASE Impact by ×1.9 and does
        // NOT add modified_base as a combined element (the old, wrong behavior).
        // Puncture/Slash are untouched; the total rises only by the impact gain.
        let base = WeaponBase::from_data("dual_toxocyst", true, &["dual_toxocyst_commodores_fortune", "dual_toxocyst_evolved_autoloader", "dual_toxocyst_fevered_frenzy"]);
        let p0 = resolve(&base, &[], StackPolicy::AssumedMax);
        let m_imp = m("phys", vec![ModEffect::Physical(Impact, 0.90)]);
        let p1 = resolve(&base, &[&m_imp], StackPolicy::AssumedMax);
        assert!((p1.damage.get(Impact) / p0.damage.get(Impact) - 1.90).abs() < 1e-9);
        assert!((p1.damage.get(Puncture) - p0.damage.get(Puncture)).abs() < 1e-9);
        assert!((p1.damage.get(Slash) - p0.damage.get(Slash)).abs() < 1e-9);
        // Total delta == exactly the impact increase (no element injected).
        assert!((p1.damage.total() - p0.damage.total() - 0.90 * p0.damage.get(Impact)).abs() < 1e-6);
    }

    #[test]
    fn injected_toxin_raises_the_toxin_dot_bracket_too() {
        // Frenzy's +100% Toxin behaves like a Toxin mod: with Pistol
        // Pestilence (+60%) the Poison tick bracket is 1 + 0.6 + 1.0.
        let pest = m(
            "pestilence",
            vec![
                ModEffect::Element(Toxin, 0.60),
                ModEffect::StatusChance(0.60),
            ],
        );
        let base = WeaponBase::from_data("dual_toxocyst_incarnon", true, &["dual_toxocyst_commodores_fortune", "dual_toxocyst_evolved_autoloader", "dual_toxocyst_fevered_frenzy"]);
        let p = resolve(&base, &[&pest], StackPolicy::AssumedMax);
        assert!(p
            .elem_dot_bonus
            .iter()
            .any(|&(t, v)| t == Toxin && (v - 2.6).abs() < 1e-9));
        // And the injection joined the vector: toxin mod + injection all
        // land as pure Toxin (no partner element): 125 × (0.6 + 1.0).
        assert!((p.damage.get(Toxin) - 200.0).abs() < 1e-9);
    }

    /// Torid is the first weapon whose innate damage is itself an element, so
    /// it is the first build where "innate last" is observable: Heat(1) and
    /// Electricity(2) combine with EACH OTHER into Radiation and the innate
    /// Toxin, last, is left pure. Placing the innate first — the superseded
    /// draft the engine used to implement — would give Gas + Electricity.
    #[test]
    fn torid_innate_toxin_takes_what_the_mods_leave() {
        let heat = m("hellfire", vec![ModEffect::Element(Heat, 0.90)]);
        let elec = m("stormbringer", vec![ModEffect::Element(Electricity, 0.90)]);
        let base = WeaponBase::from_data("torid", true, &[]);
        let p = resolve(&base, &[&heat, &elec], StackPolicy::BaseOnly);
        assert!(p.damage.get(Radiation) > 0.0, "Heat(1)+Electricity(2)");
        assert!(p.damage.get(Toxin) > 0.0, "innate Toxin, last, stays pure");
        assert_eq!(p.damage.get(Gas), 0.0, "innate first would have made Gas");
        assert_eq!(p.damage.get(Corrosive), 0.0);
    }

    #[test]
    fn element_mod_order_changes_the_combination() {
        let heat = m("scorch", vec![ModEffect::Element(Heat, 0.60)]);
        let cold = m("frostbite", vec![ModEffect::Element(Cold, 0.60)]);
        let tox = m("pestilence", vec![ModEffect::Element(Toxin, 0.60)]);
        let base = WeaponBase::from_data("dual_toxocyst_incarnon", false, &["dual_toxocyst_commodores_fortune", "dual_toxocyst_evolved_autoloader", "dual_toxocyst_fevered_frenzy"]);

        // Heat,Cold,Toxin -> Blast + trailing Toxin.
        let p1 = resolve(&base, &[&heat, &cold, &tox], StackPolicy::AssumedMax);
        assert!(p1.damage.get(Blast) > 0.0);
        assert!(p1.damage.get(Toxin) > 0.0);
        // Cold,Toxin,Heat -> Viral + trailing Heat.
        let p2 = resolve(&base, &[&cold, &tox, &heat], StackPolicy::AssumedMax);
        assert!(p2.damage.get(Viral) > 0.0);
        assert!(p2.damage.get(Heat) > 0.0);
        // Totals identical either way.
        assert!((p1.damage.total() - p2.damage.total()).abs() < 1e-9);
    }

    /// A set bonus scales PER EQUIPPED MEMBER with no threshold — one
    /// Vigilante mod already pays its 5%, which is what makes an otherwise
    /// weak member (Supplies contributes nothing on its own) worth a slot.
    #[test]
    fn each_vigilante_member_adds_its_share_of_the_set_bonus() {
        // The Vigilante mods are PRIMARY-tagged, not rifle-tagged, so this has
        // to be the union the Torid actually sees.
        let pool = crate::mods_data::pool_union(&["primary".into(), "rifle".into()]);
        let pick = |id: &str| pool.iter().find(|m| m.id == id).unwrap_or_else(|| panic!("{id}"));
        let base = WeaponBase::from_data("verglas_prime", true, &[]);
        let chance = |mods: &[&ModDef]| resolve(&base, mods, StackPolicy::BaseOnly).crit_tier_upgrade_chance;

        assert_eq!(chance(&[]), 0.0);
        assert!((chance(&[pick("vigilante_armaments")]) - 0.05).abs() < 1e-12);
        assert!(
            (chance(&[
                pick("vigilante_armaments"),
                pick("vigilante_fervor"),
                pick("vigilante_offense"),
                pick("vigilante_supplies"),
            ]) - 0.20)
                .abs()
                < 1e-12,
            "all FOUR primary members = 20%; the other two are Warframe mods"
        );
        // A non-member contributes nothing, set or no set.
        assert!(
            (chance(&[pick("vigilante_armaments"), pick("serration")]) - 0.05).abs() < 1e-12
        );
    }

    #[test]
    fn verglas_innate_cold_combines_with_mod_elements() {
        // Innate Cold(32) sits LAST, so the lone Heat mod is the only thing
        // ahead of it and the two pair into Blast (not pure Cold + pure Heat).
        // One mod element cannot tell first from last — see
        // `innate_elements_go_last_not_first` for the case that can. No
        // base-damage mod -> modified_base 32; Heat mod = 32 × 0.9 = 28.8;
        // Blast = 60.8.
        let heat = m("hellfire", vec![ModEffect::Element(Heat, 0.90)]);
        let p = resolve(&verglas_prime(), &[&heat], StackPolicy::BaseOnly);
        assert!((p.damage.get(Blast) - 60.8).abs() < 1e-9);
        assert_eq!(p.damage.get(Cold), 0.0);
        assert_eq!(p.damage.get(Heat), 0.0);

        // Innate Cold + a Toxin mod -> Viral.
        let tox = m("infected_clip", vec![ModEffect::Element(Toxin, 0.90)]);
        let pv = resolve(&verglas_prime(), &[&tox], StackPolicy::BaseOnly);
        assert!((pv.damage.get(Viral) - 60.8).abs() < 1e-9);
    }

    #[test]
    fn sentinel_base_only_ignores_galvanized_conditional() {
        // Galvanized Chamber: base +55% multishot + on-kill +25%×5. On a
        // sentinel only the BASE applies — no live stacks are generated.
        let gchamber = m(
            "galvanized_chamber",
            vec![
                ModEffect::Multishot(0.55),
                ModEffect::OnKillMultishot { per_stack: 0.25, max_stacks: 5, duration: 20.0 },
            ],
        );
        let p = resolve(&verglas_prime(), &[&gchamber], StackPolicy::BaseOnly);
        assert!((p.multishot - 1.55).abs() < 1e-9);
        assert!(p.ms_stack.is_none());
    }
    /// A BIGGER MAGAZINE COSTS RELOAD TIME on a by-round reloader, and it is
    /// free on everything else.
    ///
    /// The Felarx loads a shell at a time — 0.8 s to start, 0.4 s a shell,
    /// 0.5 s to end — so Ammo Stock buys capacity and pays for it in downtime.
    /// Modelled as one flat block, the mod read as pure profit on exactly the
    /// weapons the game charges for it (owner, 2026-08-08). The other half of
    /// the claim matters as much: an ordinary reloader must NOT pay, or the
    /// fix would have made every magazine mod worse.
    #[test]
    fn a_magazine_mod_lengthens_a_by_round_reload_and_only_that() {
        let mag_mod = crate::mods_data::class_pool("shotgun")
            .into_iter()
            .find(|m| m.id == "ammo_stock")
            .expect("ammo stock");

        let felarx = WeaponBase::from_data("felarx", true, &[]);
        let plain = resolve(&felarx, &[], StackPolicy::AssumedMax);
        let bigger = resolve(&felarx, &[&mag_mod], StackPolicy::AssumedMax);
        assert!(bigger.magazine_size > plain.magazine_size, "the mod does work");
        assert!(
            bigger.reload_seconds > plain.reload_seconds,
            "a by-round reload must grow with the magazine: {} -> {} rounds,              {:.2} -> {:.2} s",
            plain.magazine_size, bigger.magazine_size,
            plain.reload_seconds, bigger.reload_seconds
        );
        // …by EXACTLY the shells added, not by some proportion of the total.
        let added = bigger.magazine_size - plain.magazine_size;
        assert!(
            ((bigger.reload_seconds - plain.reload_seconds) - added * 0.4).abs() < 1e-6,
            "{added} more shells at 0.4 s each"
        );
        // The unmodded number is still the published one.
        assert!((plain.reload_seconds - 3.7).abs() < 1e-9, "{}", plain.reload_seconds);

        // AND THE CONTROL. The Boar is an ordinary shotgun: same mod, same
        // bigger magazine, same reload.
        let boar = WeaponBase::from_data("boar", true, &[]);
        let a = resolve(&boar, &[], StackPolicy::AssumedMax);
        let b = resolve(&boar, &[&mag_mod], StackPolicy::AssumedMax);
        assert!(b.magazine_size > a.magazine_size);
        assert!((b.reload_seconds - a.reload_seconds).abs() < 1e-9,
            "an ordinary reload is one block: {} vs {}", a.reload_seconds, b.reload_seconds);
    }

    /// MOUNTING MOMENTUM IS PAID IN SHELLS, so it is worth what the magazine
    /// is — and the magazine is not free.
    ///
    /// The perk grants +10% fire rate per shell LOADED, which makes its value
    /// a weapon stat rather than a card constant: a magazine mod buys stacks
    /// and pays for them in the by-round reload it lengthens. Implementing
    /// either half alone would have been worse than neither — stacks without
    /// the reload cost is free fire rate for the optimizer to farm, and the
    /// reload cost without the stacks is a mod that only ever hurts.
    ///
    /// It also OPENS at one reload's worth: a per-shell counter at zero
    /// describes a weapon just holstered, which is not a fight anyone measures
    /// (owner, 2026-08-08).
    #[test]
    fn mounting_momentum_is_worth_a_magazine_and_grows_with_it() {
        let evo = ["felarx_mounting_momentum".to_string()];
        let ids: Vec<&str> = evo.iter().map(|s| s.as_str()).collect();
        let base = WeaponBase::from_data("felarx", true, &ids);
        let plain = resolve(&base, &[], StackPolicy::AssumedMax);
        let b = plain
            .stacking_buffs
            .iter()
            .find(|b| b.id == "per_shell_fire_rate")
            .expect("the perk grants a buff");
        assert_eq!(b.trigger, BuffTrigger::ReloadComplete);
        // Six shells: six stacks a reload, and NOTHING at t = 0 — the pile
        // is earned, and an empty magazine takes all of it.
        assert_eq!(b.stacks_per_trigger, 6, "one per shell in the magazine");
        assert_eq!(b.initial_stacks, 0, "it is earned, not granted");
        assert!(b.duration.is_infinite(), "no clock ends it");
        assert_eq!(b.cleared_by, ClearedBy::EmptyMagazine, "an empty magazine does");

        // …and it FOLLOWS the magazine, which is the whole trade.
        let mag_mod = crate::mods_data::class_pool("shotgun")
            .into_iter()
            .find(|m| m.id == "ammo_stock")
            .expect("ammo stock");
        let bigger = resolve(&base, &[&mag_mod], StackPolicy::AssumedMax);
        let bb = bigger
            .stacking_buffs
            .iter()
            .find(|b| b.id == "per_shell_fire_rate")
            .unwrap();
        assert_eq!(bb.stacks_per_trigger, bigger.magazine_size as u32);
        assert!(bb.stacks_per_trigger > b.stacks_per_trigger, "more shells, more stacks");
        // The other side of the trade, so this test fails if the reload ever
        // stops charging for it.
        assert!(
            bigger.reload_seconds > plain.reload_seconds,
            "and the reload pays for them"
        );
    }

    /// FIRING A MAGAZINE DRY EARNS ONE MAGAZINE'S WORTH, AND NEVER MORE.
    ///
    /// This test asserted the opposite for an hour — a ramp to the 99-stack
    /// cap — because the buff was first implemented as "a reload grants stacks
    /// and nothing takes them". The mechanic is stricter: the pile is cleared
    /// the INSTANT the magazine reaches zero, not by the reload and not by a
    /// clock (owner, 2026-08-08). So the loop this sim runs — fire dry,
    /// reload, fire dry — holds a magazine's worth through each magazine and
    /// loses every stack on its last shot.
    ///
    /// The 99 cap therefore belongs to a play pattern this sim does not have:
    /// topping up a magazine that never empties. Asserted as a SHAPE so it
    /// cannot drift back — a longer fight must not pay more, which is what a
    /// buff reset every magazine looks like from outside.
    #[test]
    fn mounting_momentum_is_reset_by_an_empty_magazine_so_it_does_not_ramp() {
        let evo = ["felarx_mounting_momentum".to_string()];
        let ids: Vec<&str> = evo.iter().map(|s| s.as_str()).collect();
        let base = WeaponBase::from_data("felarx", true, &ids);
        let panel = resolve(&base, &[], StackPolicy::AssumedMax);
        let dps = |secs: f64| {
            let arena = crate::arena::Arena::training(secs);
            let p = crate::dummy::DummyParams::from_panel(&panel, &arena, &crate::arcanes_data::ArcaneFx::none());
            let mut rng = crate::rng::Rng::new(7);
            crate::dummy::run_once(&p, &mut rng).total_damage / secs
        };
        let (d30, d300, d600) = (dps(30.0), dps(300.0), dps(600.0));
        // PAST THE FIRST MAGAZINE the rate is flat: every magazine after it
        // earns the same stacks and loses them the same way. A buff that
        // accumulated would still be climbing at ten minutes.
        assert!(
            (d600 - d300).abs() < d300 * 0.03,
            "a magazine-reset buff pays the same however long the fight:              {d300:.0} vs {d600:.0} dps"
        );
        // …and the FIRST magazine is cheaper, because there is nothing to
        // spend yet. That is the honest cost of a buff you have to earn, and
        // it is also the proof the stacks are not being seeded.
        assert!(d30 < d300 * 0.95, "the opening magazine is unbuffed: {d30:.0} vs {d300:.0}");
    }

}
    /// A CHARGE THAT EATS THE MAGAZINE MAKES MAGAZINE CAPACITY A DAMAGE STAT —
    /// the only weapon in the roster where it is, and the reason the mechanic
    /// is worth a field rather than a number.
    ///
    /// Wiki Notes, verbatim: *"Charging consumes ammo, up to a full magazine on
    /// full charge"*, *"Damage dealt by the plasma bomb is directly
    /// proportional to the amount of ammo consumed during the charge"*, and
    /// *"Charge rate consumes a set 11 ammo per second. Modding to increase
    /// magazine capacity will allow a longer total charge, and thus more
    /// damage."* Confirmed in play (owner, 2026-08-09).
    ///
    /// Three things move together and this asserts all three, because any one
    /// of them alone would be a different weapon: the TIME (magazine / 11), the
    /// PRICE (the magazine), and the DAMAGE (x magazine / 11) — on the direct
    /// hit AND on the explosion, which is the larger half of the bomb.
    #[test]
    fn a_magazine_mod_buys_the_phantasmas_charged_shot_more_damage() {
        let pool = crate::mods_data::pool_for_weapon("phantasma_prime_charged");
        let shot = |want: &[&str]| {
            let b = WeaponBase::from_data("phantasma_prime_charged", true, &[]);
            let ms: Vec<_> = want
                .iter()
                .filter_map(|m| pool.iter().find(|d| d.id == *m))
                .collect();
            resolve(&b, &ms, StackPolicy::Emergent)
        };
        // STOCK is the arsenal's own line, and nothing about it moved: 11 in
        // the magazine, one second at eleven a second, 15 + 73.
        let base = shot(&[]);
        assert_eq!(base.magazine_size, 11.0);
        assert!((base.charge_seconds.expect("a charge") - 1.0).abs() < 1e-9);
        assert!((base.ammo_cost - 11.0).abs() < 1e-9, "a full charge costs the magazine");
        assert!((base.damage.total() - 15.0).abs() < 1e-6);
        assert!((base.radial.as_ref().expect("the bomb").damage.total() - 73.0).abs() < 1e-6);

        // …AND A MAGAZINE MOD MOVES ALL THREE, in the same ratio.
        let big = shot(&["burdened_magazine"]);
        let k = big.magazine_size / base.magazine_size;
        assert!(k > 1.0, "the mod has to do something: {k}");
        assert!((big.charge_seconds.unwrap() / base.charge_seconds.unwrap() - k).abs() < 1e-9);
        assert!((big.ammo_cost / base.ammo_cost - k).abs() < 1e-9);
        assert!((big.damage.total() / base.damage.total() - k).abs() < 1e-6, "the direct hit");
        assert!(
            (big.radial.as_ref().unwrap().damage.total()
                / base.radial.as_ref().unwrap().damage.total()
                - k)
                .abs()
                < 1e-6,
            "the explosion"
        );
    }

    /// …AND NO OTHER WEAPON IS TOUCHED BY IT. The field is opt-in, so a charge
    /// weapon that does not declare it keeps stating its own time and paying
    /// its own price — a bow's draw is not a magazine.
    #[test]
    fn a_magazine_mod_does_not_move_an_ordinary_charge_weapon() {
        let pool = crate::mods_data::pool_for_weapon("cernos_prime");
        let shot = |want: &[&str]| {
            let b = WeaponBase::from_data("cernos_prime", true, &[]);
            let ms: Vec<_> = want
                .iter()
                .filter_map(|m| pool.iter().find(|d| d.id == *m))
                .collect();
            resolve(&b, &ms, StackPolicy::Emergent)
        };
        let a = shot(&[]);
        let b = shot(&["primed_fast_hands"]);
        assert_eq!(a.charge_seconds, b.charge_seconds);
        assert!((a.damage.total() - b.damage.total()).abs() < 1e-9);
    }




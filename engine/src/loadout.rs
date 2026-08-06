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
            BaseDamage(v) => format!("{} Base Damage", pct(v)),
            Multishot(v) => format!("{} Multishot", pct(v)),
            CritChance(v) => format!("{} Crit Chance", pct(v)),
            // Both halves in one line, because they are one column on the card
            // and always equal; the refill is a separate sentence because it
            // answers a different question.
            PerTendril { crit_chance, .. } => {
                format!("{} Crit Chance and Status Chance per active tendril", pct(crit_chance))
            }
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

/// A weapon's unmodded panel (fixed evolutions folded in — they alter the
/// weapon's BASE stats before mods).
#[derive(Debug, Clone)]
pub struct WeaponBase {
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
    /// Damage types forced on every DIRECT hit — see
    /// `weapons_data::AttackSpec::forced_procs`.
    pub forced_procs: Vec<DamageType>,
    /// How many tendrils this weapon can hold up (0 = it has none). See
    /// `weapons_data::TendrilSpec` for why the COUNT is modelled and the
    /// tendrils' own damage is not.
    pub tendril_max: u32,
    /// ...and can it NOT be refilled mid-fight? See `WeaponSpec::no_resupply`.
    /// Separate from the above on purpose — most weapons have a reserve AND a
    /// way to top it up.
    pub no_resupply: bool,
    pub base_reload: f64,
    /// Unconditional CO rate baked into the weapon config (Carnage
    /// Reign's +33% per status type) — additive with mod CO sources.
    pub innate_co_per_type: f64,
    /// This weapon's Condition Overload behavior class.
    pub co_behavior: CoBehavior,
    /// CO base effectiveness = `original_base / evolved_base`, i.e. how much of
    /// the CO term the weapon's own evolutions dilute.
    ///
    /// **1.0 on every weapon but Dual Toxocyst.** Including a perk's flat base
    /// damage in the CO term is the NORMAL behaviour (user, 2026-07-30) — the
    /// Torid's catalog rows say 100% for both its parts and stay 100% with
    /// Final Fusillade or Plentiful Mayhem equipped. Dual Toxocyst is the
    /// anomaly the catalog calls out with a "100% or 56%" row, so the exclusion
    /// is DECLARED by that weapon (`co_base_excludes_evolution_damage`) rather
    /// than derived from the presence of a flat-damage evolution.
    pub co_base_fraction: f64,
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
    pub incarnon: Option<IncarnonForm>,
    /// Evolution-granted additive fire rate (Rapid Wrath) — joins the
    /// fire-rate-mod bucket.
    pub evo_fire_rate_bonus: f64,
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
    pub plain_hit_bonus: Option<PlainHitBuff>,
    /// Lethal Rearmament's stacking on-headshot reload speed.
    pub reload_on_headshot: Option<HeadshotReloadBuff>,
    /// A RADIAL (AoE) attack part fired alongside the direct hit — the
    /// Laetum Incarnon's 300 Radiation explosion. Separate damage vector,
    /// crit and status stats; the directly-hit enemy takes both parts.
    /// See MECHANICS §7 "Radial (AoE) attack parts" for the rule set.
    pub radial: Option<RadialBase>,
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
    /// Continuous-beam geometry, when this form is one.
    pub beam: Option<BeamGeometry>,
    /// Final Fusillade: a FLAT multishot add on the LAST round of the magazine
    /// (0.0 = not installed). Base form only — the evolution loader drops it on
    /// a charge-backed form, so this is always 0.0 there.
    pub multishot_on_last_round: f64,
    /// Plentiful Mayhem: multishot spends ammo, and what it GENERATES deals
    /// +v damage (0.0 = not installed). Both forms carry it; the rule differs
    /// by form and the sim reads that off `continuous`.
    pub multishot_ammo_bonus: f64,
}

/// Overwhelming Attrition: a hit that neither crits nor applies a status
/// grants a stack; on timeout ONE stack drops and the timer resets.
#[derive(Debug, Clone, Copy)]
pub struct PlainHitBuff {
    pub per_stack: f64,
    pub max_stacks: u32,
    pub duration: f64,
    /// Stacks at t = 0 — the buff card's other knob, the first being
    /// `duration` ([`NO_TIMEOUT`] when it is locked).
    pub initial_stacks: u32,
}

/// Lethal Rearmament: every HEADSHOT grants a stack of reload speed for
/// `duration`; on timeout ONE stack drops and the timer resets (the
/// Galvanized decay). Reload speed also shortens the Incarnon transmute
/// animations, so the buff reaches the whole cycle, not just reloads.
#[derive(Debug, Clone, Copy)]
pub struct HeadshotReloadBuff {
    pub per_stack: f64,
    pub max_stacks: u32,
    pub duration: f64,
    /// The same two knobs every other stacking buff carries.
    pub initial_stacks: u32,
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

/// A weapon's radial (explosion) attack part, unmodded.
#[derive(Debug, Clone)]
pub struct RadialBase {
    pub base_vector: DamageVector,
    pub base_crit_chance: f64,
    pub base_crit_damage: f64,
    pub base_status_chance: f64,
    /// Blast radius = the falloff `end` distance.
    pub radius_m: f64,
    /// Linear falloff window and the fraction of damage REMOVED at max
    /// distance: `mult(d) = 1 − reduction × clamp((d−start)/(end−start))`.
    /// Only bites once the sim has targets away from the epicentre.
    pub falloff_start_m: f64,
    pub falloff_reduction: f64,
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
    /// What fraction of this explosion's evolved base feeds its CO term — the
    /// radial's own copy of `co_base_fraction`, and it needs one because an
    /// evolution can raise the explosion's DAMAGE without raising the base CO
    /// multiplies. See `evolutions_data::apply`, where it is set.
    pub co_base_fraction: f64,
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
    /// See [`RadialBase::takes_condition_overload`] — CO on an explosion is the
    /// exception, not the default.
    pub takes_condition_overload: bool,
    /// See [`RadialBase::takes_multishot`].
    pub takes_multishot: bool,
    /// See [`RadialBase::co_base_fraction`].
    pub co_base_fraction: f64,
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
    /// Each hop deals this fraction of the hop before it.
    pub chain_damage_per_hop: f64,
    pub chain_takes_multishot: bool,
    /// Does every chain NODE carry a sphere too? UNVERIFIED (MEASUREMENTS
    /// M15) — one line of weapon data so a measurement flips it.
    pub chain_nodes_have_radius: bool,
}

/// The Incarnon form's charge economy, for the panel's stat display (see
/// [`WeaponBase::incarnon`]). All times are UNMODDED bases.
#[derive(Debug, Clone, Copy)]
pub struct IncarnonForm {
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
    /// The resolved lingering FIELD, when the weapon leaves one.
    pub lingering: Option<ResolvedLingering>,
    /// CONTINUOUS (beam) weapon — see [`WeaponBase::continuous`].
    pub continuous: bool,
    /// Renewed Horror's field-duration multiplier on the shot after an empty
    /// reload (1.0 = none).
    pub field_duration_on_empty_reload: f64,
    /// Final Fusillade's flat multishot add on the magazine's last round
    /// (0.0 = none). NOT folded into `multishot`: it is conditional on the
    /// magazine position, which only the sim can evaluate.
    pub multishot_on_last_round: f64,
    /// Plentiful Mayhem's damage bonus on multishot-GENERATED projectiles
    /// (0.0 = none), which also makes multishot spend ammo. Not folded into any
    /// damage bucket: it is an independent multiplier on part of the pellets.
    pub multishot_ammo_bonus: f64,
    /// The Incarnon transformation economy of THIS form, carried through
    /// so the cycle model reads it from data instead of hardcoding one
    /// weapon's numbers.
    pub incarnon: Option<IncarnonForm>,
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
    pub plain_hit_bonus: Option<PlainHitBuff>,
    /// Lethal Rearmament's stacking on-headshot reload speed.
    pub reload_on_headshot: Option<HeadshotReloadBuff>,
    /// ModifiedBase = unmodded total × (1 + Σ base damage) — the base of
    /// every status-payload formula (elemental portions excluded).
    pub modified_base: f64,
    pub crit_chance: f64,
    pub crit_damage: f64,
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
    /// Forced procs, carried through unmodded — no mod grants or removes one.
    pub forced_procs: Vec<DamageType>,
    /// Untouched by mods: the tendril cap is the weapon's.
    pub tendril_max: u32,
    /// Sentient Surge: crit chance added PER ACTIVE TENDRIL, relative to the
    /// unmodded base — "Additive to other crit chance and status chance mods"
    /// (wiki), so it joins the same bucket Pistol Gambit does rather than
    /// forming one of its own.
    pub cc_per_tendril: f64,
    /// Its status half, same bucket rule.
    pub sc_per_tendril: f64,
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
    /// Σ (CO per_stack × stacks) under `AssumedMax` (0 under
    /// `Emergent` — see `co_stack`) — applied per this weapon's
    /// [`CoBehavior`] × `co_base_fraction`, DIRECT HITS ONLY.
    pub co_per_type: f64,
    pub co_behavior: CoBehavior,
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
    let (mut bd, mut ms, mut cc, mut cd, mut sc, mut fr, mut rl, mut sd) =
        (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    // Magazine-capacity and status-duration additive buckets.
    let (mut mag, mut sdur) = (0.0, 0.0);
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
    let mut co = base.innate_co_per_type;
    let (mut co_stack, mut ms_stack): (Option<StackSpec>, Option<StackSpec>) = (None, None);
    let mut cc_on_headshot: Option<TimedBuff> = None;
    let mut cc_stack: Option<StackSpec> = None;
    let (mut wp_dmg, mut wp_cc) = (0.0, 0.0);
    let mut cd_on_kill: Option<TimedBuff> = None;
    let mut fr_on_reload: Option<TimedBuff> = None;
    let mut bd_on_reload: Option<TimedBuff> = None;
    let mut proc_conv: Option<ProcConv> = None;
    let mut elem_bonus: Vec<(DamageType, f64)> = Vec::new();
    // SEEDED from the weapon, not empty: an evolution's indirect stat is a
    // property of the weapon by the time `resolve` runs (evolutions are folded
    // into `WeaponBase` first), and it shares its bucket with the mods'.
    let mut indirect: Vec<(IndirectStat, f64)> = base.indirect.clone();
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
    // (user, 2026-08-04: "应该要锁定的，好像没锁"). The out-of-bucket layers are
    // shadowed here, and `locked` carries the fact to the SIM, which owns the
    // live ones.
    let locked_stat = |s: &str| disabled.contains(&s);
    let evo_ms_bonus = if locked_stat("multishot") { 0.0 } else { base.buff_multishot_bonus };
    let evo_ms_stacks = if locked_stat("multishot") { 0 } else { base.buff_ms_max_stacks };
    let ms_last_round = if locked_stat("multishot") { 0.0 } else { base.multishot_on_last_round };
    let evo_fr_bonus = if locked_stat("fire_rate") { 0.0 } else { base.evo_fire_rate_bonus };
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
            for e in &m.effects {
                match *e {
                    ModEffect::Element(t, v) => input.push(t, modified_base * v),
                    ModEffect::CombinedElement(t, v) => {
                        input.direct_secondary.push((t, modified_base * v))
                    }
                    _ => {}
                }
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

    let (damage, modified_base) = build(&base.base_vector, Some(&mut elem_bonus));
    // The radial part (Laetum Incarnon's 300 Radiation explosion): its own
    // base vector, crit and status stats, modded by the same buckets.
    let radial = base.radial.as_ref().map(|r| {
        let (rd, rmb) = build(&r.base_vector, None);
        ResolvedRadial {
            damage: rd,
            modified_base: rmb,
            // The post-mod flat layer (Elemental Excess) is a WEAPON stat
            // change, so the explosion takes it too.
            crit_chance: (r.base_crit_chance * (1.0 + cc) + base.post_mod_crit_chance).max(0.0),
            crit_damage: r.base_crit_damage * (1.0 + cd),
            base_crit_chance: r.base_crit_chance,
            base_crit_damage: r.base_crit_damage,
            status_chance: (r.base_status_chance * (1.0 + sc) + base.post_mod_status_chance)
                .max(0.0),
            base_status_chance: r.base_status_chance,
            // Blast RANGE mods scale the radius; the falloff FLOOR is
            // unchanged ("Only mods that increase the explosion radius change
            // how far the falloff reaches; they do not change the floor").
            radius_m: r.radius_m * (1.0 + br),
            falloff_start_m: r.falloff_start_m * (1.0 + br),
            falloff_reduction: r.falloff_reduction,
            takes_condition_overload: r.takes_condition_overload,
            takes_multishot: r.takes_multishot,
            co_base_fraction: r.co_base_fraction,
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
            crit_chance: (f.base_crit_chance * (1.0 + cc) + base.post_mod_crit_chance).max(0.0),
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

    ResolvedPanel {
        damage,
        radial,
        lingering,
        slash_on_crit,
        crit_tier_upgrade_chance,
        continuous: base.continuous,
        field_duration_on_empty_reload: base.field_duration_on_empty_reload,
        multishot_on_last_round: ms_last_round,
        multishot_ammo_bonus: base.multishot_ammo_bonus,
        incarnon: base.incarnon,
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
        crit_chance: (base.base_crit_chance * (1.0 + cc) + base.post_mod_crit_chance).max(0.0),
        crit_damage: base.base_crit_damage * (1.0 + cd),
        // No upper clamp: status chance ABOVE 100% is meaningful (a
        // guaranteed proc plus an extra roll) — DT resolves to 129%.
        status_chance: (base.base_status_chance * (1.0 + sc) + base.post_mod_status_chance)
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
        charge_seconds: base.charge_seconds.map(|c| {
            let from_rate = if base.fire_rate_shortens_draw {
                fr + evo_fr_bonus
            } else {
                0.0
            };
            c / (1.0 + cr + from_rate).max(1e-9)
        }),
        ammo_cost: base.ammo_cost,
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
        headshot_damage_bonus: base.headshot_damage_bonus,
        noncrit_bonus: base.noncrit_bonus,
        plain_hit_bonus: base.plain_hit_bonus,
        reload_on_headshot: base.reload_on_headshot,
        multishot: base.base_multishot * (1.0 + evo_ms_bonus + ms),
        base_multishot: base.base_multishot,
        // Magazine capacity: +% of base, floored to whole rounds (in-game).
        // A charge-backed Incarnon magazine is a fixed resource OUTSIDE the
        // ammo system — magazine mods are inert, so it never scales.
        magazine_size: if base.incarnon.is_some() {
            base.magazine_size
        } else {
            (base.magazine_size * (1.0 + mag)).floor()
        },
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
        forced_procs: base.forced_procs.clone(),
        tendril_max: base.tendril_max,
        cc_per_tendril: per_tendril_cc,
        sc_per_tendril: per_tendril_sc,
        mag_refill_on_kill: mag_refill,
        syndicate_radial,
        reload_seconds: base.base_reload / (1.0 + rl),
        reload_bonus: rl,
        base_damage_bonus: bd,
        co_behavior: base.co_behavior,
        co_base_fraction: base.co_base_fraction,
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
        cd_on_kill,
        fr_on_reload,
        bd_on_reload,
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

    /// The sim used to satisfy `while_aiming` silently, so every aim-gated
    /// buff fired whether or not the scenario implied aiming (user,
    /// 2026-07-30: "aim 会影响一些 buff 的触发，我们目前都让这些 buff 触发了").
    /// `resolve_with(.., aiming)` is the knob; `resolve` keeps assuming aim.
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
}

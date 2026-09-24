use super::*;

/// WHAT THIS WEAPON DOES THAT ITS STATS DO NOT SAY — its passives, as
/// sentences GENERATED from the data that implements them.
///
/// Generated and not written down, for the same reason a mod's effect lines
/// are: a sentence in the YAML would be prose in a data field, and worse, a
/// second copy of a number that could drift from the one the sim uses. Every
/// line here is built from the values the engine actually reads, so a passive
/// cannot be described as something it is not.
///
/// This exists because a weapon whose passive is invisible reads as an ordinary
/// weapon: Gotva Prime's crit set and Dual Toxocyst's
/// Frenzy are most of what those weapons ARE, and nothing on the page said so.
pub fn passive_lines(weapon: &str) -> Vec<String> {
    let Some(s) = spec(weapon) else { return Vec::new() };
    let mut out = Vec::new();

    if let Some(w) = s.weakpoint_stacks {
        out.push(format!(
            "A WEAK-POINT HIT grants a stack, up to {}, each lasting {:.0}s from the last one — worth +{:.1}x on the finished crit multiplier and +{:.0}% on the finished status chance apiece{}. Per pellet, so one shot can fill the pile.",
            w.max_stacks, w.duration_seconds, w.crit_multiplier, w.status_chance * 100.0,
            if w.ammo_efficiency >= 1.0 { ", and the shot costs no ammo at all" } else { "" },
        ));
    }
    if let Some(sc) = s.super_crit_on_status {
        out.push(format!(
            "A Status Effect has a {:.0}% chance to SET the next hit's crit chance to {:.0}% — ignoring every other crit bonus. Rolled per pellet.",
            sc.chance * 100.0,
            sc.crit_chance * 100.0
        ));
    }

    // A BLAST THAT LANDS BEHIND WHAT YOU SHOT. Said out loud for the same
    // reason the headshot line below is: it is the other way a number comes out
    // SMALLER than a reader expects, and an unexplained drop reads as a bug in
    // the sim rather than as the weapon. Both halves are on the line, because
    // the trade is the whole point — punch through buys bodies and costs the
    // explosion (MEASUREMENTS M53).
    if s.attack.radial.as_ref().map(|r| r.blast_kind) == Some(BlastKind::Terminal) {
        out.push(
            "Its round bores THROUGH what it hits and explodes where it finally stops, so unlike other explosive weapons it takes punch-through mods normally — and they cut both ways: in a crowd the blast lands on whichever enemy the round could not get out of, deeper down the line, while against a lone enemy it is carried PAST the target and away from it."
                .to_string(),
        );
    }

    // A HEAD THIS WEAPON DOES NOT CARE ABOUT. Said out loud because it is the
    // one weapon stat that makes a number SMALLER than the reader expects, and
    // an unexplained small number reads as a bug in the sim rather than as the
    // weapon. Both halves are on the line: the multiplier is the weapon's, and
    // a headshot mod still pays — which is what keeps Primary Deadhead worth
    // fitting on a gun whose own head bonus is nothing.
    if let Some(m) = s.headshot_multiplier {
        out.push(format!(
            "A headshot with this weapon is worth {m:.0}x rather than the enemy body part's own multiplier, so aiming for the head buys nothing by itself. Headshot mods still apply on top of it, and a critical headshot does not double its critical damage."
        ));
    }

    // An innate headshot bonus normally joins the additive bracket; this flag
    // marks the weapon whose does not (Cernos Prime, wiki: "unique and stacks
    // MULTIPLICATIVELY with Primary Deadhead's").
    if s.headshot_bonus_multiplicative {
        out.push(
            "Its innate headshot bonus MULTIPLIES the headshot bracket instead of joining it, so it compounds with Deadhead and Target Acquired rather than adding to them."
                .to_string(),
        );
    }

    // THE OCUCOR'S TENDRILS. Says what it is AND what it is worth HERE, because
    // the second half is the surprising one: this is the weapon's whole
    // identity and its damage against a lone enemy is zero. A line that only
    // described the passive would read as a promise the number does not keep.
    if let Some(t) = s.tendrils {
        out.push(format!(
            "Every kill spawns an energy tendril that reaches for ANOTHER enemy, up to {}; a reload or an empty magazine clears them all. Their damage is NOT counted here — a tendril homing on the target you are already shooting is cosmetic (wiki) and this sim fights one enemy — but the count is, because Sentient Surge pays crit chance and status chance per active tendril.",
            t.max
        ));
    }

    // PYRANA PRIME'S SECOND GUN. The rate it needs is stated with it, because
    // that is the half a reader cannot see: the buff cards open at zero and
    // stay there all fight unless the kills come fast enough, so a card alone
    // reads as a passive that does not work.
    if let Some(k) = s.kill_streak_summon {
        out.push(format!(
            "{} kills, each within {:.0} s of the last, summon a second copy of this weapon for {:.0} s: magazine ×{} and fire rate ×{}. This is simulated, and a kill while it is up does not refresh it — a fight whose bodies take longer than {:.0} s each never sees it at all.",
            k.kills,
            k.kill_window_seconds,
            k.duration_seconds,
            k.magazine_multiplier,
            k.fire_rate_multiplier,
            k.kill_window_seconds,
        ));
    }

    // THE SHOT COMBO COUNTER and THE SCOPE, both stated because both are
    // silent otherwise: neither is a stat on the panel and neither is a mod, so
    // a player reading a sniper's damage has no way to see that two of its
    // factors are the weapon's own.
    if let Some(c) = s.sniper_combo {
        out.push(format!(
            "Scoped in, consecutive hits build a Shot Combo Counter: {} landing {} multiply damage by 1.5x, and every threefold count past that adds another 0.5x. It drops by one for every {:.0} s without a hit, and it pays nothing at all from the hip.",
            c.min,
            if c.min == 1 { "hit" } else { "hits" },
            c.seconds
        ));
    }
    if let Some(z) = s.scope {
        // THE SENTENCE NAMES WHAT THE SCOPE ACTUALLY PAYS. It printed
        // `headshot_damage` whatever the grant was, so eight of the ten scoped
        // weapons in the roster read "+0% headshot damage" on a scope granting
        // +50% critical damage. A scope grants exactly ONE of the
        // four fields — that is why they are four fields — so the first
        // non-zero one is the grant.
        let (fraction, granted) = if z.headshot_damage != 0.0 {
            (z.headshot_damage, "headshot damage, additive with headshot mods")
        } else if z.crit_multiplier != 0.0 {
            (z.crit_multiplier, "critical damage")
        } else if z.crit_chance != 0.0 {
            (z.crit_chance, "critical chance, relative to the unmodded base")
        } else {
            (
                z.crit_chance_post_mod,
                "critical chance, applied after mods",
            )
        };
        // A MAGNIFICATION IS ONLY QUOTED WHEN THE PAGE PUBLISHES ONE. The
        // Vesper 77's aim bonus rides a laser sight and its page states no zoom
        // level at all, so the clause about trading field of view for
        // magnification has nothing to be about.
        out.push(match z.magnification {
            Some(magnification) => format!(
                "Its scope's top zoom ({magnification:.1}x) grants +{:.0}% {granted} while aiming. This arena has no field of view to trade for magnification, so the scope is always at that level.",
                fraction * 100.0
            ),
            None => format!(
                "Aiming grants +{:.0}% {granted}. The page publishes no zoom level for it, and this arena aims by default.",
                fraction * 100.0
            ),
        });
    }

    // THE SPOOL, which is the one passive that makes the stat above it WRONG
    // rather than incomplete: the panel prints one fire rate and the weapon
    // never fires at it for long. A reader who sees only the printed rate has
    // no way to tell whether the DPS below it is the one they measured, so the
    // line states where the rate starts, where it ends, and when.
    //
    // Each direction is phrased with the number ITS OWN page prints — a faller
    // is given as a span ("over 51 shots", the Phenmor's words), a riser as the
    // shot it is finally full on ("from the 9th", the Gorgon's) — rather than
    // one shape forced onto both. Same field, same arithmetic, two sentences.
    if let Some(sp) = s.attack.sustained_fire_rate {
        out.push(if sp.end < sp.start {
            format!(
                "Its fire rate FALLS while the trigger is held — to {:.0}% of the listed rate over {:.0} shots — and rebuilds the moment you stop firing. This is simulated; the sim holds the trigger until the magazine is dry.",
                sp.end * 100.0,
                sp.over_shots
            )
        } else {
            format!(
                "Its fire rate SPOOLS UP while the trigger is held — from {:.0}% of the listed rate, full from the {}th shot. This is simulated, and it rebuilds after every reload.",
                sp.start * 100.0,
                sp.over_shots.ceil() as i64 + 1
            )
        });
    }

    // NOT `no_resupply`. It was listed here and taken out:
    // every ground Arch-Gun is removed when its reserve runs out, so it says
    // nothing about THIS weapon. A line that is true of a whole class does not
    // belong on the entry for one member of it — it reads as a distinguishing
    // feature and distinguishes nothing.
    //
    // The rule still reaches the player where it is a decision: the scenario's
    // Infinite-ammo control is forced off for such a weapon, and says why.

    // A PERK NAMES ITSELF AND STOPS THERE. `data::weapons::PerkSpec` carries the
    // reference and its element injection; the NUMBERS live in the perk's own
    // module (`rules::perks::frenzy`) and reach the player through its buff card,
    // which already shows the stacks, the duration and what it grants.
    //
    // So this line's job is to tell you the weapon HAS one — which is the whole
    // complaint: nothing on the page said Dual Toxocyst had Frenzy at all, and
    // a passive you do not know to look for is a passive you do not have. The
    // buff card is where its numbers belong and where they already are; a
    // second copy here is a second thing to keep true.
    for p in &s.perks {
        let r = p.resolve();
        let mut line = format!("Weapon passive: {} — see its buff card", pretty_id(&r.id));
        if let Some(inj) = r.grants.as_ref().and_then(|g| g.injected_element.as_ref()) {
            line.push_str(&format!(" (grants +{} {} while active)", inj.amount, inj.element));
        }
        out.push(format!("{line}."));
    }
    out
}

/// `dual_toxocyst_fevered_frenzy` -> "Fevered Frenzy". Display only.
pub(super) fn pretty_id(id: &str) -> String {
    id.split('_')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

use super::*;

/// HOW A WEAPON IS PLAYED FOR A WHOLE ENGAGEMENT — a policy over its forms.
///
/// A FORM is what the weapon is at an instant (base, charged, Incarnon); a MODE
/// is what you do with those forms for three hundred seconds. One field cannot
/// express both: `form: incarnon_cycle` is a mode wearing a form's name, and
/// `form: default` resolves to one or the other depending on a weapon flag, so
/// a benchmark that may not name a form could only ask for "however it is
/// normally played" and never for "the Torid without ever transmuting".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayMode {
    /// The arsenal's form, all engagement. Every weapon has this one.
    Base,
    /// A FREE other form, all engagement — a bow that never charges, an
    /// Arch-Gun fired on its alt. Nothing is spent to be in it, so it can be
    /// held for a whole engagement and a ruler may rank it.
    Alternate,
    /// A GAUGE-FED other form, all engagement. Same shape as [`Self::Alternate`]
    /// and a different claim: you cannot be in it for a whole engagement, so no
    /// ruler ranks it, and it exists as a mode only so the builder can show the
    /// form's own numbers.
    ///
    /// Its own mode rather than an `Alternate`, because a bow with an adapter
    /// has THREE forms — drawn, tapped, Incarnon — and a build names a mode.
    Transformed,
    /// Fill the gauge in a form you can HOLD, spend it in the other, come
    /// back. One per such form: which one fills it is a build, and the
    /// Ballistica Prime's tapped shot fills it three times as fast.
    Cycle,
}

impl PlayMode {
    pub fn id(self) -> &'static str {
        match self {
            PlayMode::Base => "base",
            PlayMode::Alternate => "alternate",
            PlayMode::Transformed => "transformed",
            PlayMode::Cycle => "cycle",
        }
    }
}

/// One way this weapon can be played, and whether a ruler may rank it.
#[derive(Debug, Clone, Copy)]
pub struct WeaponPlayMode {
    /// The mode's own id, and the only name a submission or a board row uses.
    pub id: &'static str,
    pub mode: PlayMode,
    /// The form entry this mode fires — for a cycle, the one it returns to.
    pub weapon_id: &'static str,
    /// The other form, for a cycle.
    pub other_id: Option<&'static str>,
    /// May a standard benchmark rank this? See [`play_modes`].
    pub sustainable: bool,
}

/// Every way this weapon can be played, derived from the forms it registers.
///
/// SUSTAINABILITY IS DERIVED, NOT DECLARED. A form entered by filling a gauge
/// that then empties (`auto_transmute_out: on_incarnon_ammo_empty`) cannot be
/// held for a whole engagement — "always Incarnon" is not a playstyle, it is a
/// thing that happens for a few seconds at a time. A form with no gauge is just
/// a trigger pull and can be used forever, which is why a Cernos Prime that
/// never charges IS a way to play it and belongs on a board.
///
/// So nothing has to be marked. The gauge is read off the FORM ENTRY rather
/// than off the form's NAME, which is what keeps this true for the next
/// gauge-switched weapon that is not an Incarnon — Mausolon's alt-fire is
/// charged by kills, and it will get its cycle from declaring a gauge and
/// nothing else.
impl WeaponPlayMode {
    /// The `form` a fight request must carry to be played this way.
    ///
    /// The fight parser's vocabulary is form KINDS plus the one policy word:
    /// `gauge_cycle` runs the cycle, and anything else names a single form
    /// to fire. So a mode is translated at that boundary and nowhere else —
    /// which is what lets "played without ever transmuting" be ASKED FOR at
    /// all, where `form: default` could only ever mean "however it is normally
    /// played" and resolved to the cycle behind your back.
    pub fn form(&self) -> &'static str {
        match self.mode {
            // NOT `incarnon_cycle`: the policy is "fill a gauge in one form,
            // spend it in the other, come back", and an adapter is one thing
            // that produces it — the Mausolon earns its alt-fire with KILLS.
            // The old spelling is still accepted by the parser.
            PlayMode::Cycle => "gauge_cycle",
            // The single form this mode fires, named by its KIND — the Cernos
            // Prime's `base` mode is its CHARGED form, because that is the one
            // the arsenal gives you.
            _ => spec(self.weapon_id).map_or("default", |s| s.form_kind().id()),
        }
    }
}

/// DOES ENTERING THIS FORM COST SOMETHING YOU HAVE TO EARN?
///
/// The question [`play_modes`] asks of every alternate form, and the whole of
/// what decides its mode: a form you can simply hold is a `alternate` a ruler
/// may rank, and one you have to pay for is a `transformed` mode plus a
/// `cycle`. "Always in it" is not a playstyle when it costs something.
///
/// TWO GATES ANSWER YES and the answer does not distinguish them: an
/// Incarnon-style [`GaugeFormSpec`] is earned with HITS, a Tome's
/// [`MeterSpec`] with SECONDS, and a third will say so here.
///
/// A function rather than a line inside `play_modes`, because the roster's own
/// ratchet asks the same question and a second copy of the test would drift in
/// exactly the place built to catch drift.
pub fn is_gauge_fed(weapon_id: &str) -> bool {
    spec(weapon_id).is_some_and(WeaponSpec::has_gauge)
}

pub fn play_modes(weapon_id: &str) -> Vec<WeaponPlayMode> {
    let forms = forms_of(weapon_id);
    let Some(default) = forms.iter().find(|f| f.is_default).or(forms.first()) else {
        return Vec::new();
    };
    let alts = || forms.iter().filter(|f| f.weapon_id != default.weapon_id);
    // The GAUGE, not the kind: "does entering this cost a meter you must earn",
    // answered by the entry. A form that answers no can be HELD — a mode of
    // its own, and a form a cycle can be fed from.
    let held = |f: &&FormRef| !is_gauge_fed(f.weapon_id);
    let mut out = vec![WeaponPlayMode {
        id: PlayMode::Base.id(),
        mode: PlayMode::Base,
        weapon_id: default.weapon_id,
        other_id: None,
        sustainable: true,
    }];
    for alt in alts().filter(held) {
        out.push(WeaponPlayMode {
            id: free_form_id(alt.kind),
            mode: PlayMode::Alternate,
            weapon_id: alt.weapon_id,
            other_id: None,
            sustainable: true,
        });
    }
    for alt in alts().filter(|f| !held(f)) {
        // ONE CYCLE PER FORM YOU CAN HOLD: the form you fill the gauge IN is
        // a different build — see `PlayMode::Cycle`.
        for feeder in std::iter::once(default).chain(alts().filter(held)) {
            out.push(WeaponPlayMode {
                id: cycle_id(feeder.weapon_id == default.weapon_id, feeder.kind),
                mode: PlayMode::Cycle,
                weapon_id: feeder.weapon_id,
                other_id: Some(alt.weapon_id),
                sustainable: true,
            });
        }
        // …AND A FORM YOU NEVER HOLD GETS NO MODE OF ITS OWN. `Transformed`
        // is a state you are IN, and a METERED form is not one: you throw the
        // orb and you are back on the primary before it lands, so a Tome has
        // exactly two — `base` and `cycle`. MEASUREMENTS §"Two modes, and
        // `transformed` is not one of them".
        if spec(alt.weapon_id).is_some_and(|s| s.attack.meter.is_some()) {
            continue;
        }
        out.push(WeaponPlayMode {
            id: PlayMode::Transformed.id(),
            mode: PlayMode::Transformed,
            weapon_id: alt.weapon_id,
            other_id: None,
            // A gauge you must fill and then run dry is exactly what cannot be
            // sustained; anything else can.
            sustainable: false,
        });
    }
    out
}

/// The mode id of a form you can HOLD, and two of them must DIFFER: a mode id
/// is what a build names, so two sharing one names neither.
///
/// A MODE IS NAMED FOR ITS FORM, but only where it has to be. Every kind that
/// existed before keeps `"alternate"`, because a mode id is what a saved
/// preset, a share link and a board row carry and renaming one would orphan
/// every stored build. Only the kinds that could not have been stored yet —
/// the Kuva Hind's two extra triggers, melee's seven — take their own name.
pub(super) fn free_form_id(kind: FormKind) -> &'static str {
    match kind {
        FormKind::SemiAuto | FormKind::Auto => kind.id(),
        // Melee's seven are all FREE, so `Alternate` cannot tell them apart,
        // and each one IS an independent build — see MELEE.
        k if k.is_melee() => k.id(),
        _ => PlayMode::Alternate.id(),
    }
}

/// A CYCLE IS NAMED FOR THE FORM THAT FEEDS IT, and the arsenal's own form
/// keeps the bare `cycle` — what every stored build already means by it. A
/// feeder with no name falls back to it and COLLIDES, which the roster's
/// "two modes share an id" ratchet catches.
pub(super) fn cycle_id(is_default_form: bool, kind: FormKind) -> &'static str {
    if is_default_form {
        return PlayMode::Cycle.id();
    }
    match free_form_id(kind) {
        "alternate" => "alternate_cycle",
        "semi_auto" => "semi_auto_cycle",
        "auto" => "auto_cycle",
        _ => PlayMode::Cycle.id(),
    }
}

/// Does this weapon have a form you TRANSFORM into? Only such a weapon has a
/// cycle to simulate — anything else is fired in one form at a time, whatever
/// forms it registers.
///
/// DECLARED, NOT INFERRED. Asking the form's KIND makes "has a gauge" and "is
/// the Incarnon form" the same sentence; the Mausolon's kill-fed alt-fire is
/// the counter-example, and it is a `charged` form.
pub fn has_gauge_switched_form(weapon_id: &str) -> bool {
    forms_of(weapon_id)
        .iter()
        .any(|f| spec(f.weapon_id).is_some_and(WeaponSpec::has_gauge))
}

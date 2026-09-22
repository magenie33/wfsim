// SPDX-License-Identifier: AGPL-3.0-or-later
//! **THE ACTION PRIORITY LIST — what the player is DOING, as an ordered list.**
//!
//! This simulator has always executed a priority list; the list was written in
//! Rust and there were four of them. A weapon's MODE says it out loud —
//! *"a policy over its forms … what you do with those forms for three hundred
//! seconds"* (`data::weapons::play_modes`) — and `PlayMode::Cycle` is a rule
//! list in prose: fill the gauge in the form you can hold, spend it in the
//! other, come back.
//!
//! So a mode IS an APL, and this is that concept made first-class, on SimC's
//! shape: **scan top down, the first rule whose condition holds is what you do,
//! and the last rule is the one that always holds.**
//!
//! **THE LIST IS SCANNED BETWEEN SHOTS, NEVER INSIDE ONE.** A shot resolves
//! whole — every pellet of its multishot — and only then is the next action
//! chosen. That is what makes the gauge OVERSHOOT, which is the real thing: a
//! 7-pellet shot into a 30-charge gauge arrives at 35, and the shot that
//! crossed the line was fired in the form you were already in.
//!
//! **THE FIGHT EXECUTES THIS.** Every reload, every transmute and every shot is
//! the list's call: the loop scans the list at each point it acts, so a rule
//! inserted above `reload` stops the reload from happening.
//!
//! WHAT IS NOT THE LIST'S YET: arming a MELEE Incarnon, which is a heavy attack
//! at a combo the vocabulary cannot say. Its gauge reads 0, so no rule can take
//! that swing away, and the condition is what a reader adds when one is needed.
//!
//! NOT A TEXT EXPRESSION, deliberately. SimC's conditions are strings parsed at
//! load, and a typo there reads as a rule that simply never fires. Every
//! condition here is a TYPED field, refused at load when it names something
//! this engine does not have — and it is still printed the way SimC writes it,
//! `warcry,if=buff.warcry.remains<2`, because that is the form a reader knows.

use serde::Deserialize;

/// What a rule does when its condition holds.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "do", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    /// Pull the trigger — the rule every list ends with.
    Shoot,
    /// Put a magazine in.
    Reload,
    /// Go into the other form, and come back out of it. TWO ACTIONS AND NOT
    /// ONE: the record logs a transmute's two ends separately, and a list that
    /// said only "transform" would mean different things on different lines —
    /// which is also what makes leaving EARLY expressible, since a rule can
    /// name the way out without naming the way in.
    TransformIn,
    TransformOut,
    /// Cast a Warframe ability (`data/abilities/<id>.yaml`). It costs energy
    /// and, when it roots the frame, the shooting it interrupts.
    Cast { ability: String },
}

/// When a rule applies.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "if", rename_all = "snake_case", deny_unknown_fields)]
pub enum When {
    /// `if=always` — the fallback every list ends with.
    Always,
    /// `if=!can_fire` — the magazine is empty, or the weapon otherwise cannot
    /// fire this instant.
    CannotFire,
    /// `if=gauge.pct>=N` — the Incarnon gauge, as a share. `>=` and not `==`
    /// because the gauge arrives PAST the line it crossed, never on it.
    GaugeAtLeast { pct: f64 },
    /// `if=gauge.pct<=N` — spent, which is what ends a cycle's other half.
    GaugeAtMost { pct: f64 },
    /// `if=buff.<ability>.remains<N` — the buff is down, or has less than this
    /// long to run. Zero means "only once it is actually down".
    BuffRemainsUnder { ability: String, seconds: f64 },
}

/// One line of the list.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub action: Action,
    /// Tagged `if` inside, because that is SimC's word for a condition.
    pub when: When,
}

impl Rule {
    /// The line as SimC would write it, which is what the page shows and what a
    /// reader can paste into a report.
    pub fn to_simc(&self) -> String {
        let act = match &self.action {
            Action::Cast { ability } => ability.clone(),
            Action::Shoot => "shoot".into(),
            Action::Reload => "reload".into(),
            Action::TransformIn => "transform_in".into(),
            Action::TransformOut => "transform_out".into(),
        };
        match &self.when {
            When::Always => act,
            When::CannotFire => format!("{act},if=!can_fire"),
            When::GaugeAtLeast { pct } => format!("{act},if=gauge.pct>={pct}"),
            When::GaugeAtMost { pct } => format!("{act},if=gauge.pct<={pct}"),
            When::BuffRemainsUnder { ability, seconds } => {
                format!("{act},if=buff.{ability}.remains<{seconds}")
            }
        }
    }
}

/// THE LIST, in priority order. Empty is every fight this app has ever run.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(transparent)]
pub struct Apl(pub Vec<Rule>);

impl Apl {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Every ability this list can cast, in the order it first names them.
    pub fn abilities(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for r in &self.0 {
            if let Action::Cast { ability } = &r.action {
                if !out.contains(&ability.as_str()) {
                    out.push(ability);
                }
            }
        }
        out
    }

    /// The list as SimC writes one.
    pub fn to_simc(&self) -> String {
        self.0.iter().map(Rule::to_simc).collect::<Vec<_>>().join("\n")
    }

    /// **WHAT THE PLAYER DOES NOW** — the first rule that holds, and `Shoot`
    /// when none does, which is also what an empty list answers.
    ///
    /// A RULE WHOSE ACTION IS NOT POSSIBLE IS SKIPPED, not taken and refused:
    /// you cannot go into a form you are already in, and the scan has to carry
    /// on to the rule that says what you do instead. Without it every fight
    /// would open on `transform_out,if=gauge.pct<=0`, the base form's gauge
    /// being empty at the start of every engagement.
    pub fn pick(&self, now: &Now<'_>) -> Action {
        for r in &self.0 {
            let possible = match &r.action {
                Action::TransformIn => now.in_base_form,
                Action::TransformOut => !now.in_base_form,
                _ => true,
            };
            if !possible {
                continue;
            }
            let holds = match &r.when {
                When::Always => true,
                When::CannotFire => !now.can_fire,
                When::GaugeAtLeast { pct } => now.gauge_pct >= *pct,
                When::GaugeAtMost { pct } => now.gauge_pct <= *pct,
                When::BuffRemainsUnder { ability, seconds } => (now.remaining)(ability) < *seconds,
            };
            if holds {
                return r.action.clone();
            }
        }
        Action::Shoot
    }

    /// **IS THIS THE ACTION THE LIST CALLS FOR?** — what the fight asks at each
    /// point it acts.
    ///
    /// Through [`Self::pick`] and never by looking for the rule, so the LIST'S
    /// ORDER decides: a rule inserted above `reload` stops the reload from
    /// happening that instant, which is the whole of what a priority list is.
    pub fn wants(&self, action: &Action, now: &Now<'_>) -> bool {
        self.pick(now) == *action
    }
}

/// **THE FACTS A CONDITION MAY READ**, and nothing else.
///
/// A closed set on purpose: it is what stops a rule asking a question the fight
/// cannot answer, and it is the list that grows when a new condition is worth
/// having. The fight fills it; this module never learns how a gauge is kept.
pub struct Now<'a> {
    pub can_fire: bool,
    /// **HOW MUCH OF THE EARNED FORM YOU HAVE LEFT, OR HOW MUCH OF THE NEXT ONE
    /// YOU HAVE EARNED** — one number, because a cycle only ever asks one
    /// question. In the form you can HOLD it fills 0 → 1; in the earned form it
    /// drains 1 → 0, whether what drains is a charge magazine or a clock.
    ///
    /// NOT CAPPED AT 1: the shot that fills it pays in whole pellets, so 35
    /// charges into a 30-charge gauge is 1.17, and a rule reading a clamped
    /// value could not tell a full gauge from an overfilled one.
    pub gauge_pct: f64,
    /// Which half of a cycle the player is in — `true` on a weapon with no
    /// other form at all. A transmute's two ends are each possible from exactly
    /// one of them, which is what [`Apl::pick`] reads it for.
    pub in_base_form: bool,
    /// How long a named ability's buff has left, 0 when it is down.
    pub remaining: &'a dyn Fn(&str) -> f64,
}

/// **THE LIST A FIGHT RUNS**: what the player inserted, then the fight's own.
///
/// INSERTED RULES GO ON TOP, and that is what makes them do anything: the
/// mode's last rule is `shoot`, which always holds, so a cast written below it
/// is a cast that never happens. It is also what a cast MEANS — you cast
/// INSTEAD of shooting this instant, and pay the shooting for it.
///
/// **THE FIGHT'S OWN HALF IS CHOSEN BY WHETHER IT CYCLES, NOT BY A MODE NAME.**
/// `base`, `alternate` and `transformed` are one form fired throughout and
/// differ in nothing this list can say; a cycle is the only mode with a
/// transmute to decide. Composed where the params are built, so a fight cannot
/// run a list assembled from a name that no longer describes it.
pub fn for_fight(inserted: &Apl, has_cycle: bool) -> Apl {
    let mut out = inserted.0.clone();
    out.extend(preset(if has_cycle { "cycle" } else { "base" }).unwrap_or_default().0);
    Apl(out)
}

/// **THE FOUR MODES, WRITTEN OUT AS THE LISTS THEY ALREADY ARE.**
///
/// `data::weapons::play_modes` says it in prose — *"a policy over its forms …
/// what you do with those forms for three hundred seconds"* — and this is the
/// same policy in the one vocabulary the COMBAT RECORD already uses for what a
/// fight does: a shot, a reload's two ends, a transmute's two ends
/// (docs/RECORD.md §"The stream is the four things a fight does").
///
/// `mode` is a BOARD axis, so every published row on every weapon rides on
/// these being the policy its own Rust was: the decisions were routed through
/// this list one at a time, each compared against the branch it replaced on
/// every fight the suite runs and on the board's leading rows.
pub fn preset(mode: &str) -> Option<Apl> {
    let rule = |action: Action, when: When| Rule { action, when };
    let shoot_and_reload = || {
        vec![
            rule(Action::Reload, When::CannotFire),
            rule(Action::Shoot, When::Always),
        ]
    };
    Some(Apl(match mode {
        // THE ARSENAL'S FORM, ALL ENGAGEMENT: there is nothing to decide but
        // when to put a magazine in.
        "base" | "alternate" | "transformed" => shoot_and_reload(),
        // FILL THE GAUGE IN THE FORM YOU CAN HOLD, SPEND IT IN THE OTHER, COME
        // BACK — `PlayMode::Cycle`'s own sentence, as three rules.
        "cycle" => {
            let mut v = vec![
                rule(Action::TransformIn, When::GaugeAtLeast { pct: 1.0 }),
                rule(Action::TransformOut, When::GaugeAtMost { pct: 0.0 }),
            ];
            v.extend(shoot_and_reload());
            v
        }
        _ => return None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now<'a>(remaining: &'a dyn Fn(&str) -> f64) -> Now<'a> {
        Now { can_fire: true, gauge_pct: 0.0, in_base_form: true, remaining }
    }

    fn cast(id: &str, under: f64) -> Rule {
        Rule {
            action: Action::Cast { ability: id.into() },
            when: When::BuffRemainsUnder { ability: id.into(), seconds: under },
        }
    }

    /// **TOP DOWN, FIRST MATCH WINS, AND SHOOTING IS THE FALLBACK** — SimC's
    /// rule and the one the reader picked.
    #[test]
    fn the_first_rule_that_holds_is_what_the_player_does() {
        let apl = Apl(vec![cast("warcry", 2.0), cast("roar", 2.0)]);
        // Both down: the one nearer the top wins.
        assert_eq!(apl.pick(&now(&|_| 0.0)), Action::Cast { ability: "warcry".into() });
        // Warcry up, Roar down: the second rule.
        let up_warcry = |id: &str| if id == "warcry" { 30.0 } else { 0.0 };
        assert_eq!(apl.pick(&now(&up_warcry)), Action::Cast { ability: "roar".into() });
        // Both up: nothing to do but shoot, which no rule had to say.
        assert_eq!(apl.pick(&now(&|_| 30.0)), Action::Shoot);
        // …and an empty list is every fight this app has ever run.
        assert_eq!(Apl::default().pick(&now(&|_| 0.0)), Action::Shoot);
    }

    /// A LIST READS AS SimC WRITES ONE, because that is the form a reader knows.
    #[test]
    fn a_list_prints_the_way_simc_writes_one() {
        let apl = Apl(vec![
            cast("warcry", 2.0),
            Rule { action: Action::Shoot, when: When::Always },
        ]);
        assert_eq!(apl.to_simc(), "warcry,if=buff.warcry.remains<2\nshoot");
        assert_eq!(apl.abilities(), vec!["warcry"]);
    }

    /// A RULE NAMING SOMETHING THIS ENGINE DOES NOT HAVE IS REFUSED AT LOAD, so
    /// a typo cannot read as a rule that never fires — SimC's own failure.
    #[test]
    fn an_unknown_action_or_condition_is_refused() {
        let of = |y: &str| serde_norway::from_str::<Rule>(y);
        assert!(of("action: {do: cast, ability: warcry}
when: {if: always}
").is_ok());
        assert!(of("action: {do: shoot}
when: {if: always}
").is_ok());
        // An action nothing implements, a condition nothing reads, and a field
        // the rule does not have: each is a typo SimC would read as a rule that
        // never fires, and each is refused here instead.
        assert!(of("action: {do: channel, ability: warcry}
when: {if: always}
").is_err());
        assert!(of("action: {do: shoot}
when: {if: combo_at_least, n: 12}
").is_err());
        assert!(of("action: {do: shoot}
when: {if: always}
prio: 3
").is_err());
    }

    /// **THE FOUR MODES READ AS THE LISTS THEY ARE**, in the record's own
    /// vocabulary. This is the translation a reader checks BEFORE the fight is
    /// made to execute it — `mode` is a board axis, so being wrong here is
    /// every published row on that weapon moving.
    #[test]
    fn every_mode_is_a_list_and_it_reads_like_one() {
        assert_eq!(preset("base").unwrap().to_simc(), "reload,if=!can_fire\nshoot");
        // THE INCARNON CYCLE, which is the one that was never just "shoot":
        // go in when the gauge is full, come back out when it is spent.
        assert_eq!(
            preset("cycle").unwrap().to_simc(),
            "transform_in,if=gauge.pct>=1\ntransform_out,if=gauge.pct<=0\nreload,if=!can_fire\nshoot"
        );
        // EVERY MODE THE WEAPONS DECLARE HAS ONE, or the translation is partial
        // and the day the fight reads these a weapon plays as a bare shoot.
        for m in ["base", "alternate", "transformed", "cycle"] {
            let apl = preset(m).unwrap_or_else(|| panic!("{m}"));
            assert!(matches!(apl.0.last().map(|r| &r.when), Some(When::Always)), "{m} ends with a fallback");
        }
        assert!(preset("no_such_mode").is_none());
    }

    /// **WHAT THE PLAYER INSERTS GOES ABOVE WHAT THE MODE ALREADY DOES**, or it
    /// does nothing: the mode ends in `shoot`, which always holds.
    #[test]
    fn an_inserted_rule_outranks_the_mode_and_an_empty_insert_changes_nothing() {
        let mine = Apl(vec![cast("warcry", 0.0)]);
        assert_eq!(
            for_fight(&mine, false).to_simc(),
            "warcry,if=buff.warcry.remains<0\nreload,if=!can_fire\nshoot"
        );
        // NOTHING INSERTED IS THE FIGHT EVERY BOARD ROW WAS MEASURED UNDER, and
        // it has to be the mode's own list to the line.
        for m in ["base", "alternate", "transformed", "cycle"] {
            let cycles = m == "cycle";
            assert_eq!(for_fight(&Apl::default(), cycles), preset(m).unwrap(), "{m}");
            assert!(for_fight(&Apl::default(), cycles).abilities().is_empty(), "{m} casts nothing");
        }
        // …and the ability the fight casts is the one the inserted rule names.
        assert_eq!(for_fight(&mine, true).abilities(), vec!["warcry"]);
    }

    /// A GAUGE CONDITION READS THE GAUGE, which is the fact the cycle turns on.
    #[test]
    fn the_cycle_turns_on_the_gauge() {
        let apl = preset("cycle").unwrap();
        let none = |_: &str| 0.0;
        // IN THE FORM THE GAUGE FILLS IN, which is where `transform_in` is the
        // question; the way back is asked from the other half, below.
        let at = |pct: f64| Now { can_fire: true, gauge_pct: pct, in_base_form: true, remaining: &none };
        assert_eq!(apl.pick(&at(1.0)), Action::TransformIn, "full: go in");
        let spent = Now { can_fire: true, gauge_pct: 0.0, in_base_form: false, remaining: &none };
        assert_eq!(apl.pick(&spent), Action::TransformOut, "spent: come back");
        // …AND THE WAY OUT IS NOT OFFERED IN THE FORM YOU WOULD BE LEAVING FROM
        // ANYWAY: a fight opens in the base form with an empty gauge, which is
        // `gauge.pct<=0` to the letter, and it must not read as "revert".
        assert_eq!(apl.pick(&at(0.0)), Action::Shoot, "empty gauge in base form: just shoot");
        assert_eq!(apl.pick(&at(0.5)), Action::Shoot, "filling: keep shooting");
        // AN OVERFILLED GAUGE IS THE ORDINARY CASE, not an edge one: the shot
        // that crossed the line paid in whole pellets and was fired in the form
        // the player was already in. 35 charges into a 30-charge gauge.
        assert_eq!(apl.pick(&at(35.0 / 30.0)), Action::TransformIn, "overfilled: still go in");
        // …AND THE OVERSHOOT REACHES THE RULE rather than being flattened to a
        // full gauge, which is the difference a capped value could not tell: a
        // rule asking for more than full fires at 35 charges and not at 30.
        let past_full = Apl(vec![Rule {
            action: Action::TransformIn,
            when: When::GaugeAtLeast { pct: 1.1 },
        }]);
        assert_eq!(past_full.pick(&at(35.0 / 30.0)), Action::TransformIn);
        assert_eq!(past_full.pick(&at(1.0)), Action::Shoot, "exactly full is not past full");
        // …AND AN EMPTY MAGAZINE IS A RELOAD WHATEVER THE GAUGE SAYS.
        let dry = Now { can_fire: false, gauge_pct: 0.5, in_base_form: true, remaining: &none };
        assert_eq!(apl.pick(&dry), Action::Reload);
    }
}

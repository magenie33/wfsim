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
//! **A HEAVY ATTACK IS PRESSED EITHER ALWAYS OR ON THE FLASH, AND THERE IS NO
//! THIRD WAY.** `heavy` as the list's last rule is the build that presses it all
//! engagement; `heavy,if=tennokai` is the one swing the window converts. A rule
//! that pressed it on a combo count is not a build anyone plays, so the
//! vocabulary has no such condition and `an_unknown_action_or_condition_is_refused`
//! keeps it out. Arming a melee Incarnon is therefore not the list's: its gauge
//! reads 0, so no rule can take the swing that arms it away.
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
    // ---- ONE PRESS OF ONE INPUT, and the list ends with whichever of these
    // the mode is played on -------------------------------------------------
    //
    // NAMED FOR THE INPUT AND NOT FOR WHAT IT PRODUCES, the same rule
    // `model::FormKind` states for its melee half: a stance names its neutral
    // combo (Crushing Ruin calls it Raging Whirlwind) and the next stance names
    // it something else, while the button does not change. One action per form
    // kind, which `every_form_kind_is_one_input` pins.
    /// The ordinary trigger. Also what a TRANSFORMED weapon is fired on — an
    /// Incarnon form is a state you are in, not a button you press.
    Shoot,
    /// A drawn shot, where holding is a different press from tapping.
    Charged,
    /// The second trigger, and the two further pulls a weapon that CYCLES
    /// triggers has (`FormKind::SemiAuto`).
    AltFire,
    SemiAuto,
    Auto,
    /// The stance's four ground combos, by the input that starts them. The
    /// fight plays the whole combo out; the list names the press.
    Neutral,
    Forward,
    Block,
    BlockForward,
    /// Slide attack, heavy attack, heavy slam — each its own press.
    Slide,
    Heavy,
    HeavySlam,
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

impl Action {
    /// **THE PRESS THAT FIRES THIS FORM.** One per form kind and no choices:
    /// the vocabulary and `model::FormKind` are the same set of inputs said
    /// twice, and `every_form_kind_is_one_input` refuses a drift between them.
    ///
    /// `Incarnon` answers `Shoot` because it is the one kind that is not an
    /// input at all — you are IN the form, and what you press is the trigger.
    pub fn firing(form: crate::model::FormKind) -> Self {
        use crate::model::FormKind as F;
        match form {
            F::Base | F::Incarnon => Action::Shoot,
            F::Charged => Action::Charged,
            F::AltFire => Action::AltFire,
            F::SemiAuto => Action::SemiAuto,
            F::Auto => Action::Auto,
            F::Neutral => Action::Neutral,
            F::Forward => Action::Forward,
            F::Block => Action::Block,
            F::BlockForward => Action::BlockForward,
            F::Slide => Action::Slide,
            F::Heavy => Action::Heavy,
            F::HeavySlam => Action::HeavySlam,
        }
    }
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
    /// `if=tennokai` — the Tennokai flash is up, and what it buys is ONE swing
    /// turned into a heavy attack that spends no combo. A condition and not a
    /// property of the heavy action, because on a form that already spends
    /// combo the flash pays the other way and no rule fires.
    Tennokai,
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
            Action::Charged => "charged".into(),
            Action::AltFire => "alt_fire".into(),
            Action::SemiAuto => "semi_auto".into(),
            Action::Auto => "auto".into(),
            Action::Neutral => "neutral".into(),
            Action::Forward => "forward".into(),
            Action::Block => "block".into(),
            Action::BlockForward => "block_forward".into(),
            Action::Slide => "slide".into(),
            Action::Heavy => "heavy".into(),
            Action::HeavySlam => "heavy_slam".into(),
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
            When::Tennokai => format!("{act},if=tennokai"),
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
        self.pick_rule(now).map_or(Action::Shoot, |r| r.action.clone())
    }

    /// **THE RULE ITSELF**, for a caller that has to tell two lines with the
    /// same action apart — `heavy,if=tennokai` on a light combo is a different
    /// swing from the `heavy` a heavy-attack build presses all engagement.
    pub fn pick_rule(&self, now: &Now<'_>) -> Option<&Rule> {
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
                When::Tennokai => now.tennokai,
            };
            if holds {
                return Some(r);
            }
        }
        None
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
    /// Is the Tennokai flash up — a melee-only fact, and `false` on every
    /// weapon and every instant that has no window open.
    pub tennokai: bool,
    /// Which half of a cycle the player is in — `true` on a weapon with no
    /// other form at all. A transmute's two ends are each possible from exactly
    /// one of them, which is what [`Apl::pick`] reads it for.
    pub in_base_form: bool,
    /// How long a named ability's buff has left, 0 when it is down.
    pub remaining: &'a dyn Fn(&str) -> f64,
}

/// **WHAT THE FIGHT ITSELF CONTRIBUTES TO THE LIST** — the three facts that
/// decide its own half, and nothing a rule reads at run time.
pub struct Shape {
    /// The press this mode is played on (`Action::firing`).
    pub attack: Action,
    /// Is there a transmute to decide? The only mode with two forms in it.
    pub has_cycle: bool,
    /// Can the Tennokai flash turn a swing into a heavy attack? False on a gun
    /// and on a form that already SPENDS combo, where the flash pays the other
    /// way round and no rule of this shape fires.
    pub tennokai_heavy: bool,
}

/// **THE LIST A FIGHT RUNS**: what the player inserted, then the fight's own.
///
/// INSERTED RULES GO ON TOP, and that is what makes them do anything: the
/// fight's last rule is its attack, which always holds, so a cast written
/// below it is a cast that never happens. It is also what a cast MEANS — you
/// cast INSTEAD of attacking this instant, and pay the attack for it.
///
/// **THE FIGHT'S OWN HALF IS BUILT FROM WHAT IT IS, NOT FROM A MODE NAME.**
/// Which press, whether there is a transmute, whether a flash can convert a
/// swing: a name could disagree with any of them, and these cannot.
pub fn for_fight(inserted: &Apl, shape: &Shape) -> Apl {
    let rule = |action: Action, when: When| Rule { action, when };
    let mut out = inserted.0.clone();
    // FILL THE GAUGE IN THE FORM YOU CAN HOLD, SPEND IT IN THE OTHER, COME
    // BACK — `PlayMode::Cycle`'s own sentence, as two rules.
    if shape.has_cycle {
        out.push(rule(Action::TransformIn, When::GaugeAtLeast { pct: 1.0 }));
        out.push(rule(Action::TransformOut, When::GaugeAtMost { pct: 0.0 }));
    }
    // *"Performing a Heavy Attack or Heavy Slam during this flash"* — one swing
    // converted, and it outranks the combo because it REPLACES it.
    if shape.tennokai_heavy {
        out.push(rule(Action::Heavy, When::Tennokai));
    }
    out.push(rule(Action::Reload, When::CannotFire));
    out.push(rule(shape.attack.clone(), When::Always));
    Apl(out)
}

/// **THE FOUR MODE KINDS AS THE LISTS THEY ARE**, for a reader who has a mode
/// name and not a fight — the page, before a run has answered.
///
/// It is [`for_fight`] with the ordinary trigger and no flash, so the two
/// cannot drift: a mode kind says only whether there is a transmute.
pub fn preset(mode: &str) -> Option<Apl> {
    let has_cycle = match mode {
        "base" | "alternate" | "transformed" => false,
        "cycle" => true,
        _ => return None,
    };
    Some(for_fight(
        &Apl::default(),
        &Shape { attack: Action::Shoot, has_cycle, tennokai_heavy: false },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now<'a>(remaining: &'a dyn Fn(&str) -> f64) -> Now<'a> {
        Now { can_fire: true, gauge_pct: 0.0, tennokai: false, in_base_form: true, remaining }
    }

    /// An ordinary gun: the trigger, and no flash to convert a swing.
    fn gun(has_cycle: bool) -> Shape {
        Shape { attack: Action::Shoot, has_cycle, tennokai_heavy: false }
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
        // An action nothing implements, a condition NOBODY WANTS, and a field
        // the rule does not have: each is a typo SimC would read as a rule that
        // never fires, and each is refused here instead.
        //
        // `combo_at_least` is the deliberate one: a heavy attack is pressed
        // either all engagement or on the Tennokai flash, so a rule gating it
        // on a combo count describes no build and stays out of the vocabulary.
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
            for_fight(&mine, &gun(false)).to_simc(),
            "warcry,if=buff.warcry.remains<0\nreload,if=!can_fire\nshoot"
        );
        // NOTHING INSERTED IS THE FIGHT EVERY BOARD ROW WAS MEASURED UNDER, and
        // it has to be the mode's own list to the line.
        for m in ["base", "alternate", "transformed", "cycle"] {
            let cycles = m == "cycle";
            assert_eq!(for_fight(&Apl::default(), &gun(cycles)), preset(m).unwrap(), "{m}");
            assert!(for_fight(&Apl::default(), &gun(cycles)).abilities().is_empty(), "{m} casts nothing");
        }
        // …and the ability the fight casts is the one the inserted rule names.
        assert_eq!(for_fight(&mine, &gun(true)).abilities(), vec!["warcry"]);
    }

    /// **EVERY FORM KIND IS ONE INPUT, AND THE WORDS DO NOT COLLIDE.** The
    /// vocabulary and `model::FormKind` are the same set of presses said twice,
    /// so a kind that answered the wrong word would put a Magistar's slide
    /// attack on the list as a block combo. `Action::firing` matches
    /// exhaustively, which is what makes a NEW kind a compile error here.
    #[test]
    fn every_form_kind_is_one_input() {
        use crate::model::FormKind as F;
        let word = |f: F| Rule { action: Action::firing(f), when: When::Always }.to_simc();
        // The melee half is named for the BUTTON, and each button is its own.
        let melee = [F::Neutral, F::Forward, F::Block, F::BlockForward, F::Slide,
                     F::Heavy, F::HeavySlam];
        let mut seen: Vec<String> = melee.iter().map(|f| word(*f)).collect();
        assert_eq!(seen, ["neutral", "forward", "block", "block_forward", "slide",
                          "heavy", "heavy_slam"]);
        // …and a melee form's mode id IS that button (`play_modes::free_form_id`),
        // so the list a reader sees names the mode they picked.
        for f in melee {
            assert_eq!(word(f), f.id(), "{f:?}");
        }
        // THE TRIGGERS, and the one kind that is not a press at all: you are IN
        // the Incarnon form, and what you pull there is the ordinary trigger.
        assert_eq!(word(F::Base), "shoot");
        assert_eq!(word(F::Incarnon), "shoot");
        for f in [F::Charged, F::AltFire, F::SemiAuto, F::Auto] {
            assert_eq!(word(f), f.id(), "{f:?}");
        }
        seen.extend(["shoot".into(), "charged".into(), "alt_fire".into(),
                     "semi_auto".into(), "auto".into()]);
        let mut sorted = seen.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), seen.len(), "two inputs share a word: {seen:?}");
    }

    /// **A MELEE LIST NAMES THE PRESS, AND THE FLASH OUTRANKS IT.** One swing
    /// converted into a heavy attack is what the Tennokai window buys, so the
    /// rule sits above the combo it replaces — and on a form that already
    /// spends combo there is no rule, because the flash pays the other way.
    #[test]
    fn a_melee_list_is_the_button_with_the_flash_above_it() {
        let light = Shape { attack: Action::Slide, has_cycle: false, tennokai_heavy: true };
        assert_eq!(
            for_fight(&Apl::default(), &light).to_simc(),
            "heavy,if=tennokai
reload,if=!can_fire
slide"
        );
        let heavy = Shape { attack: Action::Heavy, has_cycle: false, tennokai_heavy: false };
        assert_eq!(for_fight(&Apl::default(), &heavy).to_simc(), "reload,if=!can_fire
heavy");
        // AND THE TWO `heavy` LINES ARE TOLD APART BY THEIR CONDITION, which is
        // the only thing that distinguishes a converted swing from the press a
        // heavy build makes all engagement.
        let none = |_: &str| 0.0;
        let flash = Now { can_fire: true, gauge_pct: 0.0, tennokai: true,
                          in_base_form: true, remaining: &none };
        let converted = |a: &Apl| a.pick_rule(&flash)
            .is_some_and(|r| r.action == Action::Heavy && r.when == When::Tennokai);
        assert!(converted(&for_fight(&Apl::default(), &light)), "the light combo converts");
        assert!(!converted(&for_fight(&Apl::default(), &heavy)), "a heavy build converts nothing");
    }

    /// A GAUGE CONDITION READS THE GAUGE, which is the fact the cycle turns on.
    #[test]
    fn the_cycle_turns_on_the_gauge() {
        let apl = preset("cycle").unwrap();
        let none = |_: &str| 0.0;
        // IN THE FORM THE GAUGE FILLS IN, which is where `transform_in` is the
        // question; the way back is asked from the other half, below.
        let at = |pct: f64| Now { can_fire: true, gauge_pct: pct, tennokai: false, in_base_form: true, remaining: &none };
        assert_eq!(apl.pick(&at(1.0)), Action::TransformIn, "full: go in");
        let spent = Now { can_fire: true, gauge_pct: 0.0, tennokai: false, in_base_form: false, remaining: &none };
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
        let dry = Now { can_fire: false, gauge_pct: 0.5, tennokai: false, in_base_form: true, remaining: &none };
        assert_eq!(apl.pick(&dry), Action::Reload);
    }
}

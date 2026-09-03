# Melee — seven ways to swing one weapon

The roster's first melee weapon landed on 2026-08-28: the **Magistar**, as seven
entries. This is what melee is, what it cost, and what is still owed.

---

## 1. THE ONE DECISION EVERYTHING ELSE FOLLOWS FROM

> **Each way of swinging a melee weapon is an independent BUILD, and therefore
> an independent board row**.

A melee player picks one loop and runs it for the whole engagement. That is the
definition of a MODE in this repo — `WeaponPlayMode::sustainable` asks exactly
"can you be in it for three hundred seconds" — so the four stance combos, the
heavy attack, the slide and the heavy slam are seven `FormKind`s and seven
modes, not seven animations of one.

It was proposed here first as a new axis called a "rotation", on the reasoning
that a player mixes attacks inside one fight. The owner corrected it: they do
not. Melee therefore needed **no new build axis at all** — `mode` already means
this — which took the largest piece of speculative work off the plan.

The count is not a problem to be managed. Seven rows per weapon per ruler is
seven different builds being ranked, which is what the board is for.

| mode id | input | Crushing Ruin's name | the swings |
| --- | --- | --- | --- |
| `base` | stationary | Raging Whirlwind | 400% (forced Impact) -> 200% (360deg) -> 300% (360deg) -> 500% (forced Knockdown) |
| `forward` | forward | Tidal Force | 100% -> 100% (360deg) -> 100% (360deg) -> 200% -> 300% |
| `block` | block | Winding Temper | 300% -> 200% -> 400% |
| `block_forward` | block + forward | Shattered Village | 300% -> 2x50% (360deg) -> 300% -> 2x50% (360deg) -> 400% (360deg, Impact) -> 500% (Knockdown) |
| `heavy` | heavy | Crowd Fall | 600% (Lifted) -> 600% (Knockdown) |
| `slide` | slide | Hell's Wave | 200% (360deg, forced Impact) |
| `heavy_slam` | heavy, airborne | — | 630 Blast, 10 m sphere |

**THE ROSTER HOLDS TWO CLASSES**, and the second one is what proves the first
was not hard-coded. A Tonfa (the Praedos) differs from a hammer in every number
the class owns — 1.17 attack speed against 0.833, a **4x** heavy against 6x,
0.6 Follow Through against 0.4, a 0.4 s heavy CHARGE against the hammer's
published 1.2 — and it needed no engine change to say so. Its stances are two more cards in a pool of their own.

**THE TONFA HEAVY IS THE ONE NUMBER TWO WIKI SOURCES DISAGREE ON.**
`Module:Stances/data` prints `Dmg = { 250 }, Hits = { 2 }` for Discord Sewn,
which reads as 500%; DE's export says 4x on all six Tonfas in the game (Boltace
704/176, Kronen 520/130, Kronen Prime 848/212, Ohma 896/224, Telos Boltace
840/210, Praedos 800/200). Six weapons agreeing beat one row, so the entries
take the module's HIT COUNT and the export's TOTAL: two hits of 200%.

**A MODE'S NAME IS FIXED AND ITS STRENGTH IS NOT**. The id
is the INPUT — `neutral`, `block_forward` — and so is what a reader sees;
swapping the stance changes what `neutral` is WORTH and never what it is called.

That was briefly the other way round: the label was derived from the equipped
stance, so the same mode read "Raging Whirlwind" under Crushing Ruin and
"Falling Rock" under Shattering Storm. It is worse, and for the one question a
stance slot exists to answer — *which stance is best for the neutral combo* —
cannot be asked at all if the two builds call that mode different things. One
vocabulary, seven entries, and the numbers underneath them move.

The combo's own name stays in a comment above each block in the stance file,
where it is documentation for whoever transcribes the next stance rather than a
label the app draws.

---

## 2. THE MECHANICS, AND WHERE EACH ONE CAME FROM

Every line below is quoted from `wiki.warframe.com` and cross-checked against
WFCD's export where the export carries the field.

### The combo counter

- **Points, not hits.** *"Stance attacks add combo points, scaling with the
  attack's stance damage multiplier (100% stance damage multiplier = 1 point)"*
  — so a 400% swing is worth 4, and one number does both jobs.
- **Per body landed.** The wiki's own reading of the Rauta: *"generates 2 combo
  points per pellet landing on enemy (max 28 points across 14 pellets)"*.
- **The ladder** is `1 + floor(points / 20)` capped at 12: 2x at 20, one more
  every 20, 12x at 220. Venka Prime's 13x and Dex Nikana's shortened 110 are the
  two exceptions the page names, and neither is in the roster.
- **Five seconds**, refreshed by any landed swing.

### …AND IT DOES NOT MULTIPLY A NORMAL SWING

> *"**Melee Combo Multiplier does not multiply the damage of your normal
> attacks**. Instead, you can spend Melee Combo Count to perform Heavy Attacks,
> which deals between **2x** and **12x** damage."*

This is the single most counter-intuitive fact in melee and it was verified
twice, from the raw wikitext and from the rendered page. It is what makes the
seven modes genuinely different builds rather than seven damage numbers:

- in a **combo** mode the counter climbs and pays through **Blood Rush**
  (crit chance) and **Weeping Wounds** (status chance), both of which read it
  and never spend it;
- in a **heavy** mode the swing that reads the counter is the swing that empties
  it, so those two cards are worth almost nothing and **initial combo** is worth
  everything.

Both cards enter brackets this engine already had — the wiki writes the formula
that way itself:

```
Crit Chance   = Weapon Crit Chance   x [1 + Mod Crit Bonus   + BloodRush     x (Combo - 1)] + Static
Status Chance = Weapon Status Chance x [1 + Mod Status Bonus + WeepingWounds x (Combo - 1)]
```

**ONE BRACKET IS ONE SUM.** The status line above is assembled once and read by
the ROLL; a second sum over the same bracket is a second answer, and a card that
lands only in the unread one pays nothing while the panel shows it paying —
which is what Weeping Wounds, an on-kill status buff and Enduring Affliction all
did until the two sums became one.

### Initial combo is a FLOOR that refills

> *"Initial Combo grants a minimum value of combo points when **idle** or after
> a combo reset. Heavy attacks spend initial combo, which regenerates at a rate
> of 40 combo points per second."*

**THE CLOCK IS THE WEAPON'S, AND A FORM INHERITS IT.** `combo_duration_seconds`
is five seconds on almost every melee and sits on the weapon entry, so a form
that does not inherit it reads zero and is floored at the 0.1 s the wiki names —
a counter that dies between every pair of swings. Six of the Magistar's seven
modes were in that state: Blood Rush and Weeping Wounds climbed nothing, and
Heavy Attack Efficiency kept points that were gone before the next swing.

**IT SPENDS WHAT THE SWING READ, AND REGENERATES FROM WHAT IS LEFT.** *"40%
heavy attack efficiency will change the amount spent to 60% combo points …
capped at 90%"* and *"Heavy attacks spend initial combo, which REGENERATES at a
rate of 40 combo points per second"* — so 90% of a 140-point counter is 126, and
the missing 14 are back in 0.35 s. Spending only the EARNED half spent nothing
at all in a heavy mode, which earns none: the card sold in that family of modes
paid zero there. With it, a crowd build carrying Galvanized Reflex's +80 holds
**8x** between slams instead of 4x.

**A ZERO OR NEGATIVE DURATION IS ITS OWN RULE** — *"prevents increasing the
combo counter"* — which is a harder stop than the 0.1 s floor, and a MELEE RIVEN
is what reaches it: Combo Duration's malus is -8.2 s at disposition 1.35 against
a five-second weapon. `ResolvedWeapon::combo_frozen` carries it and the counter
is cleared where it is read, so no swing has to remember; the initial-combo
floor still pays, because a floor is not an increase.

**THE FIGHT OPENS WITH IT FULL.** The 40 a second is what a heavy attack owes
back, not what a player owes on the way in — so the first heavy of the
engagement already pays the floor, and a half-second fight (one slam) is the
only thing that can tell the two rules apart.

It is the whole of the pure-heavy build. The Magistar's Incarnon Form carries
**+30**, which is back inside **0.75 s** against a **0.8 s** wind-up (1.2 s
reduced by the form's own +50% wind-up speed) — so every heavy attack lands at
**2x** rather than 1x. `melee_combo_points` takes the higher of what was earned
and what has refilled, which is what makes "spend it and it comes back" and
"build on top of it" one number.

### Follow Through

> *"Proportion of weapon damage = FT^(n-1)"*, and *"Follow Through does not
> affect: (Heavy) Slam Attacks, Any attack that shoots projectiles or deals
> AoE."* A Hammer is **0.4** and a Tonfa **0.6**.

**AND A SLIDE ATTACK, which that list omits and the page's own stat glossary
does not**: *"Follow Through: when hit multiple targets in one strike, the part
of damage remaining after each target (excludes Slam Attacks **and Slide
Attacks**)"*. The two agree once a spin is read as the AoE the second bullet
means — the stance module marks every slide combo `Types = { "360" }` — and the
glossary is the half that names the case.

**IT IS `1.0`, NOT ABSENT** (`notes: slide_ignores_follow_through`). The two are
different claims in this model: 1.0 is the wiki's own bottom row, every target
at 100%, while a form carrying NO follow through reaches the aimed body ALONE.
That is right for a SLAM, whose damage is its radial and whose sphere is what
reaches the room, and wrong for a spin. The Praedos's slam form carried 0.6
where the two hammers carried nothing; it now carries nothing either.

### Slams

- 2x a normal attack, **3x for a heavy slam**.
- *"linearly diminishing with distance from the point of impact to 50% (70% for
  heavy slam) at the edge of its radius."* WFCD's `attacks[]` array agrees
  exactly: `falloff {start: 0, end: 10, reduction: 0.3}`.
- The sphere is centred on the **wielder's own feet** — `BlastKind::Slam`, its
  own variant beside `Contact` and `Terminal` because it answers the same
  question (where is the epicentre) and a third variant keeps that a total
  function. It is why a slam ignores the 2.5 m reach that decides every other
  melee mode.

### …and the heavy slam LOOP, which is the only cadence a player chooses

`climb -> slam -> recover`, and **only the recovery is a fixed cost**:

- **The slam is instant.** *"Heavy slams do not have any wind up"* — so the
  1.2 s charge that sets the pure-heavy mode's whole cadence is not paid here,
  and no wind-up card pays anything in this mode.
- **The climb is free.** A slam is launched from mid-air and nothing bounds how
  long the player spends getting there, so the interval is the recovery plus
  whatever ascent is worth waiting through.
- **…so the wait is priced against the counter.** See below: the wait is the
  same decision every heavy mode makes, and here it costs nothing at all,
  because the climb was the player's own time anyway.

The climb and the 150% radius are DERIVED from what the entry already says —
the explosion is a `Slam` — rather than declared per weapon, so the next slam
weapon gets the loop for free.

### A HEAVY MODE SWINGS WHEN THE COUNTER IS WORTH SPENDING

Not when the animation allows, which is the same rule for a standing heavy and
for a slam — `heavy_cycle_seconds`, gated on the swing SPENDING the counter and
on nothing about the mode.

The counter climbs in STEPS (`1 + floor(points / 20)`, the initial-combo floor
refilling at 40 a second), so the candidates are the animation's own floor and
each rung that floor can still reach, ranked by multiplier per second. **Waiting
for the FULL refill is the intuitive rule and the wrong one**: 110 initial combo
is 6x after 2.75 s, which is 2.18 multiplier-seconds against the 4.00 that the
2x at 0.5 s already pays.

It decides real builds in both modes. Three wind-up cards take the Magistar's
standing heavy to 0.43 s, which is SHORT of the first rung — 17 points is still
1x, so Corrupt Charge's +30 buys **nothing at all** if the swing goes the moment
it can, and **x1.7** if it waits the extra 0.07 s. The proxy is multiplier per
second and it UNDER-states a wait, since Blood Rush reads the same counter.

**A HEAVY ATTACK EARNS NO COMBO POINTS.** *"Connecting with a heavy attack does
not add to the combo counter"*, and it is the swing's kind that says so rather
than the mode: a Tennokai heavy on a light combo is one too, which is why a
build converting one swing in four climbs the counter Blood Rush reads more
slowly than its two chances suggest.

### …AND THE ONE CARD THAT PAYS A SLAM, WHICH IS NOT THE HEAVY ONE

Shockwave Synergy — *"for each enemy hit by Slam radius, gain 4 Combo Count"* —
pays the ORDINARY slam and pays a HEAVY slam nothing. So it
is the trailing slam of a combo that earns it: the Praedos's `block_forward`
ends in one, nine bodies in the sphere is 36 points, and Blood Rush is what
turns those into damage.

**THE GATE IS THE GENERAL RULE, not a carve-out for this card.** A swing that
SPENDS the counter adds nothing to it, and the perk reads the same
`spends_combo` flag every other combo earner reads — so the heavy slam and the
heavy attack are one answer rather than two.
`shockwave_synergy_is_paid_by_the_crowd` asserts the gain SCALES with the crowd
on the combo that slams, and asserts the heavy slam FLAT beside it, which is
what makes the gate checkable at all.

**COMBO COUNT CHANCE SCALES IT rather than rolling for it**: *"True Punishment
affects Shockwave Synergy, effectively doubling the Combo Count gain from 4 to
8"* — True Punishment is +100% chance, so the grant is `4 x (1 + chance)`.

### The Sacrificial pair enhance each other

*"The Sacrificial Set enhances all equipped mods within the set. Increases the
effects of both mods by 25% when both are equipped together"* — a SET that grows
its own members, which no other set in the data does. It is COMPLETION and not
per member: one card alone is worth its face, and the pair are worth 25% more
each (Sacrificial Steel's +220% crit chance becomes +275%, Sacrificial
Pressure's +110% melee damage becomes +137.5%).

`ModEffect::scaled` is what grows a card's numbers, and it answers `None` for a
kind it cannot reach — a member carrying one would have that half silently
unpaid, so a test walks every self-scaling set's members and refuses it.

**AND THE `x1.33 DAMAGE TO SENTIENTS` ON EACH CARD IS SIMULATED**, not declared:
`Faction::Sentient` is a faction this engine already resolves, so the bonus pays
whatever the fight's target is — zero against every body in the roster today,
and 33% the day a Sentient is fought. A condition about the TARGET is simulated;
reading it as the set bonus is what left the pair paying each other nothing.

### Condition Overload (melee)

`Total = Base x [1 + Damage Mods + (CO x n)] x (1 + Elemental Mods)`, +80% per
type at rank 5, additive with Pressure Point — which is `co_behavior`'s existing
`additive_with_base_damage`, unchanged. Two of its own rules are **not modelled
yet** and are declared on the entries: it does not apply to slams or radial
attacks, and the status a hit applies does not amplify that same hit.

---

## 3. THE STANCE SLOT — the first mod that changes what a weapon FIRES

Every other card in this app changes what a weapon fires WITH. A stance
publishes a combo per FORM, and installing one replaces the weapon entry's own
script — so the same Magistar in the same mode is a different sequence of swings
under Crushing Ruin (Raging Whirlwind: 1400% over 3.00 s, three inputs) and
under Shattering Storm (Falling Rock: 2100% over 4.90 s, four inputs each
ending on a slam) — 466.7%/s against 428.6%/s, the wiki's own column.

**IT NEEDS NO FIELD OF ITS OWN ON THE WIRE**, and that is what made it cheap. A
stance mod is legal in the stance slot and NOWHERE else, so a flat mod list can
say which entry is the stance by looking at it. That is exactly what the exilus
slot could NOT do — an exilus-eligible mod is legal in a main slot too — which
is why THAT one travels in a field of its own (AGENTS.md) and this
one rides `mods`, appended. Nothing about the share link, the board record, the
worker's table or `builds::identity` had to change.

**A TENNOKAI HEAVY BREAKS THE CHAIN**, so the next light swing starts the combo
over. The wiki says nothing about a stance chain's position
— asked directly, and the page is silent on what advances it and what resets it
— so this is his answer and is recorded as one rather than derived.

It decides which swings ever happen, which is why it could not be left to
whichever behaviour fell out of the code. Raging Whirlwind is
`400 / 200 / 300 / 500`, and a chain that restarts on every window fires the
opener again and again. The sharp case is Discipline's Merit: it opens the
window every FOUR hits, which is exactly that combo's length, so the 500%
finisher is never reached at all. `swing_idx += 1` against `swing_idx = 0` was
the whole difference.

The slot is drawn on a melee weapon and on nothing else, the picker's filter
runs BOTH ways (a stance is refused from the eight, and only a stance is offered
in the tenth), and `builds::validate_with` does not count it against
`MAIN_SLOTS` — a melee build carrying one is `8 + 1` in the same list.

**THE COMBOS COME FROM `Module:Stances/data`**, the wiki's own Lua table, which
publishes `Dmg`, `Hits`, `Procs`, `Types`, `ImpactMultiplier` and `Duration` per
combo. It was found only after the first transcription, and it corrected three
things: a swing that lands TWICE (`Hits = { 1, 2 }`), a bonus to the Impact
component alone (`ImpactMultiplier = { 1.5 }`, which is a different thing from
the forced Knockback proc several of the same swings also carry), and the SLAM
three of Crushing Ruin's four combos end on.

**TAKE `Duration` FROM THE MODULE, NEVER FROM THE RENDERED TABLE'S TWO
COLUMNS.** Deriving it as `total damage / the printed %/s` is right only if
every damage row was transcribed, and a transcription that DROPS one lands on a
duration that is wrong by exactly the same fraction — silently, because the
derived figure still divides out to the printed rate. That is how Falling Rock
shipped as 3.03 s against its published 4.90, and Smashing Fury as 3.16 against
3.55.

**ONE `Attacks` ENTRY IS ONE ATTACK INPUT**, which is what the table's columns
are (`Module:Stances` draws one icon per entry) and what the combo's clock is
divided by — `notes: combo_clock_is_the_input`.

**FETCH THE MODULE WITH `curl`, not through a summariser.** It is 222 KB and
arrives whole (`?action=raw`), which is how all four Tonfa and hammer stances
were transcribed; a summarising fetch cuts it off alphabetically and the reader
invents plausible numbers past the cut.

The module's whole vocabulary, for whoever transcribes the next stance:

- `Types`: `360` (a spin, reaching everything in range), `Sweep`, `Thrust`,
  `Slam`, `Ranged`. Two are modelled — `360` and the slam — and `Sweep`,
  `Thrust` and the empty string all become the forward 90-degree arc, which is
  the one invented number in the model and is declared.
- `ImpactMultiplier` / `SlashMultiplier`: a bonus to that PHYSICAL component
  alone. Both are exact rather than approximations — neither type enters the
  elemental hierarchy, so nothing can have consumed it on the way.
- `Procs`: `Knockback` (Impact's own), `Bleed` (Slash's), `Puncture`,
  `Knockdown`, `Lifted`, plus `Ragdoll`, `Stun`, `Impair`, `Stagger`,
  `Finisher`, `Detonate` — all crowd control with no damage payload here.

---

## 4. WHAT THE ENGINE GREW

| piece | where |
| --- | --- |
| seven melee `FormKind`s, each its own mode | `weapons_data::FormKind`, `play_modes` |
| the combo script — a swing with its own multiplier, delay, wind-up, hit count, Impact and Slash bonuses, 360deg flag, forced procs and trailing slam | `weapons_data::ComboHit` |
| combo points an ORDINARY slam grants per body (Shockwave Synergy) | `EvoEffect::ComboCountOnSlamHit`, gated off a swing that spends the counter |
| a STANCE as a mod that supplies those scripts | `loadout::ModDef::stance`, `resolve` |
| the combo counter, its ladder and its refilling floor | `dummy::melee_combo_multiplier`, `melee_combo_points` |
| Blood Rush / Weeping Wounds | one `(combo - 1)` term in each of two existing brackets |
| Follow Through, `FT^(n-1)` over the bodies a swing reached | `spread_from_follow_through`, `Origin::FollowThrough` |
| a slam's epicentre at the wielder's own feet | `BlastKind::Slam` |
| the heavy wind-up as its own clock | `ComboHit::windup_seconds`, `ModEffect::HeavyWindUpSpeed` |
| Knockdown as a real status | `dummy::DebuffState::knockdown` |
| melee has no ammo, aims at nothing, puts nothing on a head | `scenario::Capability` |
| eleven mod effect kinds | crit/status per combo, combo duration as seconds and as a multiplier, initial combo, heavy efficiency, heavy damage, slam damage, melee reach in metres, combo count chance, wind-up speed, crit chance on a slide |
| six evolution effect kinds | relative base damage, initial combo, melee reach, follow through, slam radius, wind-up speed, proc conversion |
| the melee riven pool, and the counter it can STOP | `data/rivens/melee.yaml`, `ResolvedWeapon::combo_frozen` |

**THE RIVEN POOL IS A DIFFERENT ITEM, not the rifle's with rows crossed out.**
`PlayerMeleeWeaponRandomModRare` is 24 stats sharing twelve with a gun's: no
Multishot, Magazine, Reload, Ammo, Zoom, Recoil, Punch Through or Projectile
Speed, and eleven of its own — Attack Speed, Combo Duration, Initial Combo,
Heavy Attack Efficiency, Range, Finisher Damage, Critical Chance for Slide
Attack and the combo-count pair. Every one lands in the bucket its MOD already
lands in, so the riven's Critical Chance carries True Steel's `(x2 for Heavy
Attacks)` and its Range is Reach's flat metres.

Two shapes are new with it. **A stat can be malus-only** (`bonus: false`),
which makes "what the picker offers" two lists rather than one — DE ships
Additional Combo Count Chance and Chance to Gain Combo Count as separate
entries, one for each slot. And **a malus-only stat keeps DE's sign** rather
than being flipped into one, which is the whole of MEASUREMENTS M71.

**Nothing else moved.** Every new field is empty or zero on a gun, and all 824
engine tests — the golden values among them — are unchanged.

---

## 5. THE ARCANE SLOT, AND ONE BUG THE AUDIT FOUND

A melee weapon seats a MELEE arcane, and `arcane_pools` already answered
`["melee"]` before there was a pool behind it. All twelve are in now. **Four of
them pay and eight declare**, which is an honest ratio for a family whose
triggers are a Warframe's shields breaking, a roll, a finisher and a knockdown:

- **Melee Exposure** is the pool's biggest number and the one card that reaches
  a slam: *"On Ability Cast: Gain 60% Corrosive Damage on Melee strikes for 25s.
  Stacks up to 240%"*. The trigger is a cast this arena cannot perform, so the
  choice was the cap or nothing — and the cap is what a melee player holds, so
  every stack is held for the whole engagement. It lands in the ELEMENTAL-MOD
  bracket (`ArcaneFx::added_elements`, the shape an ability's added element
  already had), which is why it pays the explosion where Condition Overload
  cannot, and why it does not combine with what the mods make.

- **Melee Retaliation** reads a PLAYER stat — `+30% Melee Damage per 200 current
  Shields` — the way Primary Bulwark reads armour, through `tenno_scaled`. The
  neutral Tenno has zero shields, so it pays nothing and the panel says why:
  *"this Warframe has 0 shields — 0 whole steps of 200, so it pays 0%"*. That is
  the honest answer rather than a broken gate, the same reading Secondary
  Kinship gets in a solo fight.
- **Melee Duplicate** is the biggest declared gap in the pool: *"On Base
  Critical Hits: 100% chance for your attack to strike a second time"* — an
  extra hit in exactly the sense `docs/EXTRA_HIT.md` means, and worth a second
  copy of every critical swing. The engine's extra-hit machinery fires from a
  PERCENTAGE and an element; this one repeats the instance off a crit roll,
  which is a trigger nothing here has. Approximating it would be most of the
  weapon.
- **Melee Influence** is the meta card, it is entirely a CROWD effect, and it
  is the fourth that pays. An Electricity status opens an 18 s window that
  *cannot be refreshed while it runs*; inside it, every spreadable elemental
  status the swing applies lands again on everything within 20 m of the body it
  struck, each of them dealt that element's own damage from the hit.
  `dummy::spread_from_influence` is the mechanic and docs/EXTRA_HIT.md is where
  its two odd clauses are argued — its Condition Overload is the STRUCK body's,
  and its status burns off the swing's base rather than its own.

  **IT SPREADS FROM EVERY BODY THE SWING STRUCK**, not from the aimed one
  alone: *"Melee Influence only triggers from direct melee strikes"*, and a
  swing reaching past the first body at `FT^(n-1)` is one of those — so a
  Follow Through instance reports what it landed and seeds a spread of its own.
  Against ONE target it is worth exactly nothing, which is asserted rather than
  described: the copies have nowhere to go, so the number must not move at all.

  **A ONE-HIT KILL SEEDS NOTHING.** *"Hits that one-hit-kill enemies cannot
  trigger nor benefit from Melee Influence"* — the wiki files it under Bugs and
  it is the behaviour all the same, so a body the swing killed spreads nothing.

### …and the bug

**Condition Overload paid EXACTLY ZERO in all seven melee modes**, and the audit
caught it. Melee's Condition Overload is the ORIGINAL one and is unconditional —
no kill, no stacks, no clock, just the target's status count read on every swing
— and it was routed through the Galvanized family's path, which earns the same
PAYLOAD on a kill and therefore opens at zero stacks. It waited for a trigger it
does not have, for the whole fight, on the single most important card in the
melee pool.

`starts_full` is the fix, and it is NOT derived from `duration == NO_TIMEOUT`,
which would have been the cheap test and is wrong: locking a buff card writes
exactly that duration, and locking *"removes the expiry and nothing else — the
count still starts where the card sets it"*.

The three answers are now three different numbers, which is the assertion:
**x2.29** in the light combo, **x1.44** in the pure heavy, and **x1.0000** in
the heavy slam — the last one because *"this damage does not apply to Slams,
Heavy Slams, or Radial Attack explosions"*, which falls out of
`takes_condition_overload` defaulting to false on an explosion rather than being
arranged.

**And a crit card that says `(x2 for Heavy Attacks)` is doubled there and
nowhere else.** True Steel, Sacrificial Steel and Galvanized Steel all print it;
Blood Rush sits in the same bracket and prints nothing of the kind, so the CARD
carries the rule rather than the bucket. `20 x (1 + 1.20)` is 44% on a swing and
`20 x (1 + 2.40)` is 68% on a heavy, asserted as a pair.

---

## 5b. TENNOKAI — the exilus slot's whole decision

Tennokai is the one melee mechanic that changes what the LOOP does rather than
what a number is. When its window is open the next swing of a light combo
becomes a HEAVY ATTACK — the class's multiplier in place of the stance's, times
a combo multiplier it reads **and does not spend**. A combo build climbs the
counter to 12x with its swings and fires FREE 12x heavy attacks between them,
which is why a 15% chance is worth nearly three times the build. The play
pattern is to use the window the moment it fires.

**A TENNOKAI HEAVY BREAKS THE STANCE CHAIN**, so the next light swing starts
the combo over, and that decides which swings ever happen: Raging Whirlwind is
`400/200/300/500` and Discipline's Merit opens the window every FOUR hits,
which is that combo's length — so under a restarting chain the 500% finisher is
never reached.

**A TENNOKAI SWING LANDS ONCE.** The window does not buff a light swing, it
SUBSTITUTES a heavy attack for it — and a heavy attack's multiplier is the
class's whole total, so a Rogue Edict row that lands a 50% spin five times is
one 4x heavy when the flash converts it, not five. `swing_instances` is the
rule and it is asserted on the rule rather than on a fight: the window also
costs a wind-up the light swing did not pay, and that alone moves every total
the other way, so a fight-level assertion passes with the rule deleted.

**AND IT CHARGES AT ITS OWN SPEED.** *"The Wind-Up Speed of Tennokai attacks is
not affected by Wind-Up Speed bonuses from other sources"* — so
`Tennokai::windup_seconds` is resolved apart from the bucket every other
wind-up reads, off the CLASS's charge. A heavy build carrying both wind-up
cards therefore charges a Tennokai attack SLOWER than its ordinary one, which
is the card's own clause and not an artefact. The window's own speed-up is
**+100%** and it is the one number in the mechanic DE publishes nothing for —
`loadout::TENNOKAI_WINDUP_SPEED`, declared on every melee entry.

**ALL SEVEN TENNOKAI CARDS ENABLE IT.** Every one opens with the same three
words on its own card and only then says what else it does, so the mechanic is
read off the card and not off a list of card names. The negative control is a
build carrying none of them. The other four melee exilus cards are blocking and
movement, which this arena has neither of.

---

## 6. WHAT IS STILL OWED

Each of these is on the page, in both languages, on the entry or the card it
applies to.

1. **Melee Duplicate**, and the eight other arcanes whose triggers this arena
   has not got — see §5.
2. **Finisher Damage on a melee riven.** It rolls, it occupies a slot and it
   names the card, and a finisher is an animation this arena has none of — the
   same answer Finishing Touch already gets. The editor and the mod list both
   say which line pays nothing.
3. **Power Spike's partial combo decay** — a Warframe passive, so the counter
   here drops to zero where a real build keeps most of it.
4. **One attack input's own animation length.** The module publishes a combo's
   DURATION and one entry per input, and nothing inside an entry, so the INPUTS
   share it evenly and an entry's rows land together. It moves a status tick's
   start by fractions of a second and moves no total.
5. **`Sweep` and `Thrust` are one shape here** — a 90-degree arc in front,
   `dummy::MELEE_ARC_DEG`. A sweep is wider than that and a thrust is
   narrower, the real answer is per attack INPUT, and nothing published gives
   any of them an angle.
6. **Forma on the stance SLOT.** The grant itself is modelled — a stance is an
   Aura, not a cost, and hands back 5 points or 10 on a matching slot
   (`mods::stance_capacity`, and the slot's polarity is the weapon's own). What
   is not planned is REPOLARIZING that slot to buy the double, which the wiki
   says a Forma can do, so a build that would spend one reads five capacity low
   here — the conservative direction: a build that fits here fits in game.
7. **Three cards that name a state this arena has not got**: Relentless
   Combination wants a combo point when a Slash DoT ticks, Spring-Loaded Blade
   wants a stacking reach buff, and Shattering Impact wants a flat armour strip
   per Impact hit.

   **THE TWO LIFTED CARDS ARE OFF IT.** `Lifted` is a status this engine
   tracks, so Enduring Strike's combo-point chance and Enduring Affliction's
   status chance are SIMULATED gates — a condition about the target, which this
   repo simulates rather than assumes. What decides them is the CADENCE, since
   the swing that lifts never amplifies itself: `LIFTED_SECONDS` is 1.0 s, so a
   heavy slam on a 2 s recovery lets every Lift lapse and pays nothing, while
   three wind-up cards on the standing heavy bring the cycle to 0.43 s and
   double the rolls. That makes `LIFTED_SECONDS` — a stand-in DE publishes no
   figure for, and which the wiki says grows with the combo multiplier — the
   number those two cards are worth.

   **Galvanized Reflex is off that list too**, and it was the biggest thing on it:
   *"On Melee Kill: +20 Initial Combo for 20s. Stacks up to 4x"* is COMBO
   POINTS, so four stacks raise the floor a heavy mode returns to by +80 — four
   tiers — and it is worth **x2.83** on a slam loop that gets kills. It cost no
   engine code beyond one `BuffGrant`: `ModEffect::GrantsStackingBuff` is the
   door for exactly this, a trigger and a grant that are both already words.
   What DID have to move is where the floor is read: it was a build-time
   constant, and a floor that is earned during the fight has to be read at the
   swing.

### …and two numbers that need measuring

- **`KNOCKDOWN_SECONDS` is a stand-in.** DE publishes no duration and the wiki
  flags its own table as under-researched. It stands at 1.0 s because `Lifted`
  does. Every slam forces a knockdown, so on a slam build this decides whether
  Condition Overload reads one more type between slams — 80% of a base-damage
  bucket either way.
- **A HEAVY SLAM'S LANDING RECOVERY is a stand-in.** Nothing publishes it, and
  it is the whole of that mode's cadence now that the wind-up is gone. It stands
  at **2.0 s** at 1.0x attack speed — uncancellable, and attack speed shortens
  it, which is what `delay_seconds` means everywhere else.
- **`TENNOKAI_WINDUP_SPEED` is the other.** *"Performing a Heavy Attack or
  Heavy Slam during this flash increases its Wind Up Speed"*, and DE publishes
  no figure. It stands at +100% (half the charge) and is bounded either way: at
  0% the window is still worth a free heavy attack, and at +100% it is worth
  that plus a fifth of a second.
- **Where the stance multiplier sits relative to QUANTIZATION.** It is folded
  into the swing's base here, which makes the snap grid scale with it. That is
  the reading with an argument behind it (DE publishes a damage figure per combo
  attack) and it is the harmless one, since quantizing `kX` against `ks` is `k`
  times quantizing `X` against `s`. Unmeasured either way.

---

## 7. THE INCARNON, WHICH IS A BUFF AND NOT A FORM

> *"Reach **6x** Combo and then Heavy Attack to activate Incarnon Form"*,
> lasting **180 seconds**, persisting through holstering and removed only on
> death.

**A FORM IS A DIFFERENT WEAPON; A BUFF IS DIFFERENT NUMBERS ON THE SAME ONE.**
That is the whole of the split, and the data already answered it: a gun's
Incarnon unlocks a weapon entry with its own attack and its own charge
magazine, and no melee Genesis grants `unlocks_weapon` because there is nothing
to unlock. A melee Incarnon changes numbers on the swings the weapon already
has, so it is drawn as a buff (`melee_incarnon` in the roster, a card with the
usual two knobs) and never as a transform — no animation is played, nothing is
billed, and `transforms` counts none of it.

**UNDERNEATH IT IS THE SAME MACHINERY, and that is deliberate.** The engine has
exactly one concept for "the numbers change part-way through the fight" — two
resolved panels and a rule for which one you are in — and the melee one is the
same weapon resolved twice, once with the tier that states the window and once
without (`DummyParams::for_panel`, which is where the DECISION lives so no
surface can skip it). Only two things differ, and both are data:

| | arms | ends |
| --- | --- | --- |
| gun | `Gauge { charge_on, charges_to_fill }` | `ChargeMagazine` |
| melee | `HeavyAtCombo(x)` | `After(seconds)` |

The alternative was a buff carrying a set of grants, and it was rejected for a
reason that would have rotted: every grant needs a LIVE bucket, the Magistar's
wind-up speed and the Praedos's reach have none, and the next Genesis grants
something nobody made live — a card paying nothing while the panel shows it
paying. Two panels make every stat live at once and for good.

**A HEAVY ATTACK IS THE WHOLE OF THE CONDITION**: a stationary heavy, a
heavy slam, or a **Tennokai** heavy, which is what gives a light combo mode any
way in at all — its loop performs no heavy of its own. Three consequences the
sim reports rather than assumes away:

- a **light combo mode with no Tennokai card never arms it**, and the Genesis
  is worth exactly zero there;
- a **pure-heavy mode** earns no combo points, so it arms it only off the
  initial-combo FLOOR — Corrupt Charge's +30 is 2x and 6x needs 100, so it
  takes Galvanized Reflex's earned +80 as well, and against a target that never
  dies it takes the card's stack knob;
- the **Praedos's 90 s is half a 180 s engagement**, where a Genesis's 180 s is
  all of it. That gap is a number here rather than a declared generosity.

`a_melee_incarnon_is_earned_and_a_heavy_attack_is_what_earns_it` pins all three,
and `magistar_evo_armed` holds the window open where a test means to ask what
the Genesis is WORTH rather than whether the mode can arm it.

---

## 7b. HOW A MODE EXPLAINS ITSELF, AND WHY IT GOT SHORTER

Seven modes broke the mode-def block twice on the same day, in opposite
directions.

**It listed them all, and that was a wall.** The block was written to explain
every mode a weapon has, which was right at two or three
and is seven paragraphs above the build at seven — six of them about something
the reader did not pick. It draws the PICKED one now; comparison is what the
board does, one row per mode.

**Every line had to earn its place**. Three of the four
sentences a melee mode printed said nothing:

| line | why it went |
| --- | --- |
| "Swung as its Neutral Combo for the whole engagement." | restates the heading, which already reads *Neutral Combo* |
| "Nothing is spent to be in it, so it can be held forever." | true of all seven |
| "— a ruler ranks it." | true of all seven |

What replaced them is per-mode by construction: how many of its swings are
**spins** (a spin reaches the whole room and a sweep reaches one body — the one
spatial fact separating two combos of the same weapon), whether it **spends the
combo counter** (which makes it a different BUILD, since Blood Rush and Weeping
Wounds read the counter it empties), whether its damage is a **slam** the
weapon's reach does not bound, and what it **forces** whatever the roll says.

**The three numbers are in one unit, and two conversions get them there.** A
combo script's multipliers are relative to the ENTRY they are written in, and
the explosion is not in the script at all. `magistar_heavy_slam` states
`damage: { impact: 0.0 }` — the whole attack is its 630 Blast `radial:` — so a
summary counting swing multipliers reported **100% of base** for an attack that
deals **300%**, and naively adding the radial gave 400%. `swing_share`
(this entry's vector over the weapon's, 1.0 everywhere but the slam) and
`radial_share` are both on the FORM rather than in `combo_summary`, because
`stance_combos` has no weapon entry behind it: anything read off the entry would
vanish the moment a stance went in the slot.

---

## 8. WHAT MELEE COSTS FROM HERE

The Magistar paid for nearly all of the machinery. The second melee weapon is a
`data/weapons/melee/<id>.yaml` per mode — about 40 lines each, every number from
the wiki's infobox and its stance's published table — plus one line in
`assets.yaml` and one in `data/i18n/zh/names.yaml`.

**Two traps, both hit on the way in.** The export carries `Beginner /
Intermediate / Expert` internal tiers of the same mod under one display name:
joining by NAME put a phantom Pressure Point (+200%, rank 10) in front of this
work, and the bare path is the player's card. And WFCD's top-level `damage`
object has the Magistar's Puncture and Slash swapped, where its own
`damagePerShot` array, its own `attacks[]` block and the wiki all agree with
each other. Join by `uniqueName`, and let the wiki settle it.

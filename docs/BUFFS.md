# wfsim — Buff System Architecture

How the simulator models **buffs** — the runtime states you *gain* during a
fight (arcane stacks, weapon passives like Frenzy, ability effects, conditional
mods, combo). There are many and some are complex (stacking, resets, durations,
rate caps, feedback loops). Terminology follows [`GLOSSARY.md`](GLOSSARY.md).

## The model: you gain a buff, overlaid on a target

A **buff** is not a static weapon property. When a trigger condition fires, you
**gain a buff** — a live overlay with stacks and (optionally) a duration, shown
in the HUD **buff bar**. Examples: landing a headshot grants Dual Toxocyst's
**Frenzy** buff on that weapon; hitting with Secondary Enervate equipped grows
its **stacking** buff. The arcane and the passive are both *sources* that grant
buffs.

Three concepts, kept distinct:

- **Buff** — the runtime overlay: `{ id, scope, stacks, expiry, contributions }`.
  It is what the buff bar displays.
- **Buff bar** (`BuffBar`) — the single container of all active buffs, mirroring
  the player's HUD. A buff appears here regardless of scope.
- **Perk** (`Perk`) — the held/equipped grantor (arcane / weapon passive /
  Incarnon evolution) that, on a trigger event, applies / refreshes / resets its
  buff in the bar. Holding the perk is what enables the buff. The perk keeps
  private bookkeeping (rate-limit timers, counters) the UI does not show; the
  buff holds the visible stacks/duration.

**Scope.** A buff's `BuffScope` (Weapon / Warframe / Companion / Companion
weapon / Squad — extensible) decides where its contributions actually apply. The
HUD shows every buff regardless of scope, so "shown in the UI" ≠ "applies to this
weapon". We track the precise target and can display **more finely than the game
HUD**. Frenzy and Secondary Enervate are `Weapon`-scoped.

**Same name, two things.** A perk and the buff it grants often share a name — the
*Frenzy perk* (Dual Toxocyst's passive) grants the *Frenzy buff*. Keep them
distinct in speech and code.

## Debuffs: the same machinery, pointed at the target

Status effects (procs) are **not** a separate system. Per the core
philosophy: *a proc is only a trigger event; the entity is a
**debuff** applied onto the target*, symmetric to perks granting buffs:

```
player side:  Perk   --trigger-->  Buff    in the player's BuffBar
enemy  side:  proc   --trigger-->  Debuff  in the target's DebuffBar
```

The container on the target is named **`DebuffBar`**
— same machinery as `BuffBar` (stack tracking, per-stack expiry,
contribution snapshots), rendered in the arena UI as the enemy's status
icon row.

**Every actor carries both bars** — the structure is
symmetric across sides; only the contents differ:

|            | BuffBar (gained boons)                          | DebuffBar (suffered afflictions) |
|------------|--------------------------------------------------|----------------------------------|
| player     | arcane stacks, Frenzy, ability buffs             | enemy procs on the player (Magnetic drain, Toxin DoT, Heat armor strip) |
| enemy      | Ancient Healer 90% DR aura, Guardian Eximus overguard regrant, Shield Osprey shields, Eximus auras | our procs: Stagger, Corrosive strip, Viral, DoTs |

Mitigation and damage layers on *either* side read the same two snapshots:
the attacker's `BuffBar` and the defender's `DebuffBar` (+ the defender's
`BuffBar` for protective auras like the Healer's DR).

- A **debuff** has the same shape as a buff: stacks, per-stack duration,
  overflow policy (e.g. Stagger: 5 stacks, 6 s each, 6th proc replaces the
  oldest), per-stack modifiers, caps, conditions. Stored in
  `data/debuffs/` as standalone files — unlike player-side buffs (inlined at
  their granting item), a debuff is shared by EVERY source of its status
  type, so it keeps its own file.
- The target-side pipeline layers read a **contribution snapshot from the
  target's `DebuffBar`** exactly like the weapon pipeline reads the player's
  `BuffBar`:
  armor reduction (Corrosive), damage-taken multipliers per pool (Viral →
  health, Magnetic → shields/overguard), slow / crit-received (Cold),
  Parazon threshold (Impact/Stagger), etc.
- **DoT debuffs** (Heat, Toxin, Slash, Gas, ...) are debuffs whose stacks
  emit damage events on the timeline (layer [8]) — each stack ticking on
  its own clock.
- **The provenance principle**: *every trigger has a
  source, and every debuff instance records its full applier context.*
  A stack = `{ application timestamp (the FIFO key), applier context
  snapshot (the hit-formula inputs), expiry, payload values }`. This is
  what keeps 4 players × 100 weapons on one target coherent: each Bleed
  ticks at its own value, each Magnetic stack's break-chunk reads its own
  mods, FIFO replacement swaps whole instances with their provenance.
  System-issued effects derive their context from instances (per-stack
  sum — Blast radial, expected for the Magnetic break-proc) or from a
  designated trigger instance (Frozen's reset stacks), never from thin
  air. Heat's singleton accumulator is the lone exception with a single
  shared context slot (locked to its first proc).
- CC components (stagger, knockdown) are debuff properties with per-unit
  immunities (Ospreys/Bosses/Tenno ignore Stagger's CC while still
  carrying its stacks); Overguard grants blanket CC immunity.

## The core split

The 8-layer damage pipeline ([`CORE.md`](CORE.md) §3) is a **pure function**:
given a snapshot of the current modifier state, it computes one Hit's damage.
Statefulness lives in the timeline (layer [8]):

```
timeline loop (layer [8]) — stateful, single seeded RNG, fixed tick rate
   │  emits typed Events (Hit{big_crit}, Kill, Headshot, Reload, Tick, ...)
   ▼
Perks react → apply/refresh/reset their Buff in the BuffBar
   │  (BuffBar also expires duration-based buffs as time advances)
   ▼
BuffBar.total_contributions() → a modifier-state snapshot
   │
   ▼
pure pipeline [1]–[7] computes this Hit from the snapshot (never mutates buffs)
```

## `Perk` and `Buff`

```rust
trait Perk {
    fn id(&self) -> &str;
    fn on_event(&mut self, event: &Event, t_secs: f64, bar: &mut BuffBar);
}

struct Buff { id, scope, stacks, expiry_secs: Option<f64>, contributions }
```

`Contributions` has one field per **additive** bucket (currently
`flat_crit_chance`; grows as buffs land). Multiplicative buckets (e.g. an
independent fire-rate multiplier) are *not* summed here — the mod-resolution
layer (layer [1]) combines buckets by their real rules. `expiry_secs = None`
means "until reset/removed" (Secondary Enervate); `Some(t)` means a timed buff
(Frenzy's 3 s).

## Declarative first, coded escape-hatch second

Most perks reduce to a small set of primitives:

```
trigger    (OnHit / OnKill / OnHeadshot / OnBigCrit / Periodic / ...)
accumulator(per-stack value / max stacks / duration / refresh / decay /
            rate cap Hz / reset condition)
contribution(which bucket: flat crit chance / crit chance multiplier /
            fire-rate multiplier / injected element / ...)
scope      (Weapon / Warframe / Squad)
```

**Where the data lives: INLINE at the source.** A
perk+buff pair is written as ONE `kind: buff` block inside the yaml of the
thing that grants it — the mod, the arcane, the weapon (its `passives:`).
There are no standalone `data/perks/` / `data/buffs/` files: a triggered
buff is 1:1 with its granter, so the pair is declared together (the block's
trigger/condition IS the perk; the rest IS the buff). The block may carry a
`perk: <id>` field when the mechanic needs a hand-written stateful
implementation (`engine::perks::<id>` behind the `impl Perk` trait) — the
data still records the trigger, values, and wiki-verified boundary notes;
the code implements the state machine (Enervate's ramp/reset, Frenzy's
multiplicative fire rate + injection). A buff would GRADUATE back to a
standalone file only when it stops being 1:1 — shared by several granters,
or granted by a non-item source (abilities, team buffs) — neither exists in
the pool today.

## Mod data: triggered effects are `kind: buff`

A mod's conditional/triggered effect is written as one declarative buff (the mod
is the perk that grants it). UNCONDITIONAL modifiers stay as their plain bucket
(`base_damage_bonus`, `multishot_bonus`, …); only triggered ones use `buff`.
A "permanent + triggered" mod (the Galvanized family) is simply BOTH — a plain
bucket effect **and** a `kind: buff` effect.

```yaml
- kind: buff
  trigger: on_kill        # on_kill | on_headshot | on_headshot_kill | on_ability_cast | on_reload | on_hit | passive
  condition: while_aiming # optional
  grants: multishot       # the bucket it feeds (multishot | condition_overload | crit_chance | crit_damage | status_chance | fire_rate | accuracy | …)
  rank0: 0.027            # per-stack value at rank 0
  rankMax: 0.30           # per-stack value at max rank
  max_stacks: 4           # 1 for a non-stacking triggered buff
  duration: 20            # seconds; omit = until reset
  decay: lose_one_and_reset   # lose_one_and_reset | per_stack_expiry | all_drop

### The decay families, and which are real

`lose_one_and_reset` is the Galvanized rule; `all_drop` is the on-status
arcane family (Cascadia Flare). A stacking buff decays by ITS OWN rule, never by
whichever one happens to be implemented.

`per_stack_expiry` became real on 2026-08-07: each stack keeps its OWN clock and
expires on it, oldest first. Stormburst is the first perk that needed it
(observed in game), and at the cap a new stack evicts the oldest rather
than being dropped.

`all_at_once` became real on 2026-08-18, on a MOD rather than a perk. Split
Flights states both halves of it in consecutive lines — *"Subsequent hits
refresh all stacks' duration"* and *"Stacks expire all at once after 2 seconds
without a hit"* — so it shares `lose_one_and_reset`'s single clock and differs
only in what falls due: the pile, not one stack.

**The difference is not cosmetic.** Under `lose_one_and_reset` a single hit
inside the window refreshes the whole pile, so one hit every 2 s holds three
stacks. Under `per_stack_expiry` that same hit holds exactly one — sustaining
three needs three hits per window. On the Furis that moved Stormburst's extra
pellets from 11 to 7 over the same engagement. `all_at_once` is identical to
the Galvanized rule while you keep hitting and four times harsher the moment you
stop — a full pile drains over four windows there and vanishes in one here,
which is why the choice has to be data rather than a default nobody re-read.

### A MOD can grant one too

Every stacking buff before Split Flights came from an EVOLUTION or an ARCANE,
so `WeaponBase::stacking_buffs` was the only door and a mod that stacked on a
trigger had to invent a bespoke `ModEffect` — `OnKillMultishot`,
`OnHeadshotKillCritChance`, `ConditionOverload`, three variants for one idea.
Split Flights is a trigger already in `BuffTrigger` (a hit) feeding a grant
already in `BuffGrant` (the multishot percentage bracket), so a fourth would
have been absurd.

`ModEffect::GrantsStackingBuff` carries the whole spec and `resolve` hands it to
the panel beside the weapon's own. Everything downstream — the buff card, the
replay curve, the stack config, the sampler — walks that list by construction
and keys on `id`, so **a second mod on any trigger/grant pair in those two
vocabularies costs no engine code at all**. The yaml is
`kind: stacking_buff` with `trigger:` / `grants:` / `max_stacks:` /
`duration:` / `decay:`, and the buff's id is the MOD's, leaked once — which is
what stops those four readers from drifting.

It is a separate `kind:` from `kind: buff` deliberately. That one contributes
at the ASSUMED MAX through `CondBucket`, which is the right answer for a card
whose trigger the sim has no event for and the wrong one the moment it does — so
a card opts in per mod rather than being moved by a change to the family.
  one_stack_per_instance: true  # optional, arcanes: cap the GRANT (see below)
```

### `one_stack_per_instance:` — capping the grant, not the decay

`decay:` says how stacks LEAVE; this says how fast they ARRIVE. Default false =
one stack per trigger event, so a status-triggered buff gains one per PROC, and
a multishot volley that procs on five pellets grants five.

Cascadia Flare states otherwise and is the only entry in its family that does —
verbatim: *"Only one stack can be added per damage instance; applying multiple
Heat status effects, such as via Multishot or Archon Vitality in a single hit
will not generate multiple stacks."* So the instance is the **trigger pull**,
not the pellet and not the proc: `ArcRuntime::next_instance` opens one per pull,
and separately one per lingering-field tick and per syndicate blast, because
those are their own instances at their own times.

Measured on a 2.8x-multishot Laetum Incarnon: 40 stacks in **3.0 s** per proc
against **4.8 s** per pull. Over a 120 s benchmark the DPS barely moves — the
ceiling is reached either way and then held — so this is a RAMP correction, and
it is worth having exactly where a fight is short or a build is thin.

The three other 40-stack on-status arcanes (Primary Blight, Primary Frostbite,
Conjunction Voltage) do NOT carry the flag: their pages do not state the rule,
and absence is not evidence of it. `arcanes_data` asserts them false so a
copy-paste cannot spread it quietly.

### `condition:` — the state that has to hold

`condition:` gates ANY effect, not only a `kind: buff` one: put it on a plain
bucket and the bucket only counts while the state holds.

Every value names a state of the fight's TENNO (`data/tenno/`), and they all
resolve to `ModEffect::WhileTenno(TennoCondition, …)`, which
`loadout::resolve_for` evaluates against the Tenno it was handed:

| value | `TennoState` field | means |
|---|---|---|
| `while_aiming` | `aiming` | aiming down sights — Galvanized Crosshairs / Scope, Argon Scope, … |
| `while_invisible` | `invisible` | Spectral Serration's "+330% Damage while Invisible" |
| `while_airborne` | `airborne` | the Aero set |

`while_aiming` is one of these rather than a case beside them: it was a bool
threaded through the resolver while the other states lived on the Tenno, which
is two homes for one kind of fact.

The neutral Tenno is aiming and doing nothing else, so a while-Invisible mod
contributes nothing until a scenario says otherwise — and the panel labels the
row with the condition rather than hiding it. An unrecognised `condition:`
gates NOTHING, which `mods_data`'s card-vs-model test catches as "the card
states a condition and the model has none".

## Arcane data: a Warframe stat is `kind: tenno_scaled`

An arcane whose value comes from the PLAYER rather than from anything the
weapon does reads it off the fight's Tenno:

```yaml
- kind: tenno_scaled
  stat: armor           # armor | max_energy
  above: 1000           # the first N units pay nothing (default 0)
  per_unit: 0.01        # bonus per unit past `above`
  min_energy_pct: 0.9   # optional gate: at or above this fraction of the pool
  grants: base_damage   # base_damage | multishot — the bucket it joins
  rank0: 2.5            # the CAP at rank 0
  rankMax: 5.0          # the CAP at max rank
```

It resolves to a passive one-stack `ArcBuffSpec` (`ArcTrigger::Passive`,
pinned): a Warframe stat does not decay mid-fight and no event grants it, so it
rides the grant machinery the on-kill arcanes already feed correctly instead of
adding a static bucket to the damage path. A bonus of zero produces NO buff at
all — a zero-value stack would still list in the picker and invite someone to
turn it up.

`engine::mods_data` maps the modeled `(trigger, grants)` combos to the buff
`ModEffect` variants at max rank (`OnKillMultishot`, `ConditionOverload`,
`OnHeadshotCritChance`, `OnHeadshotKillCritChance`); triggers not yet modeled
keep their uniform data and resolve to a no-op until the generic interpreter
below lands. Replaces the old ad-hoc `stacking_buff` / `on_headshot_*` kinds.

## Activation policy: what a buff is worth at t = 0

A conditional/stacking buff can be evaluated under three policies
(recorded 2026-07-24; see [`OPTIMIZER.md`](OPTIMIZER.md) §3):

1. **`assumed_max`** — full stacks, 100% uptime. What the **PANEL** shows: a
   build's ceiling, which is the question the panel answers.
2. **`configured`** — explicit per-buff stack counts/uptimes (the buff cards).
3. **`emergent`** — the timeline grants and decays stacks itself. What the
   **SIM** runs.

The policy changes results, so it is part of any evaluation cache key.

### "No timeout" OVERWRITES the duration

The buff card carries two knobs, and they answer different questions:

- **stacks** — what the count is at t = 0;
- **no timeout** — whether a stack can ever expire.

Locking removes the **expiry and nothing else**. The count
still starts where the card sets it and still climbs on every trigger.

**It is not a flag. It is the duration**. `apply_buff_config`
writes `loadout::NO_TIMEOUT` (`f64::INFINITY`) into the buff's own `duration`,
and that is the entire implementation — every clock in the sim is
`expiry = now + duration`, so an infinite duration is a buff that earns
normally and never falls off. Nothing downstream knows the concept exists:
there is no `pinned`/`locked` field on any buff spec any more, and no read site
has a branch for it.

That shape is the point. The flag it replaced had to be honoured wherever a
stack count was read, and it was missed at enough of them that "no timeout"
came to mean its own opposite — the stacks decayed anyway while the trigger was
skipped, so a locked buff decayed to zero and could never come back. It was
wrong in three of the five families at once (Galvanized on-kill stacks, Lethal
Rearmament, Overwhelming Attrition), and a player reported the worst of them. A duration cannot be forgotten, because
it is the thing the clocks already read.

Two consequences worth knowing:

- **`AssumedMax` is `initial_stacks = max` plus `NO_TIMEOUT`** — "full, and
  nothing can take it away", said once instead of as a second flag.
- **A buff with no clock states it as `NO_TIMEOUT`, never as `0.0`.** The
  `tenno_scaled` arcanes (a Warframe stat does not decay mid-fight) carried
  `duration: 0.0` beside the flag; a zero duration in a decay loop is an
  infinite loop waiting for a reader.

### A buff whose end is an EVENT, not a clock

`NO_TIMEOUT` is the implementation of locking because every buff in the pool
ends on a timer — except one. The **Ocucor's tendrils** (`tendrils`) are gained
on a kill and cleared by a MAGAZINE EVENT: "Tendrils disappear upon reloading
or emptying the magazine." There is no duration to overwrite, so the card's two
knobs land where the same sentences point:

- **stacks** → `DummyParams::tendrils_initial`, the count the run opens with,
  spent by the same event that clears an earned one;
- **no timeout** → `tendrils_held`, i.e. that event no longer clears them.
  Same statement as everywhere else — *nothing takes it away* — pointed at the
  thing that actually ends this buff.

It is a buff by every test that matters (a trigger grants it, a trigger takes
it, it has a cap), and it had no card until 2026-08-08. That was not cosmetic:
a tendril costs a kill, so at a level where kills are slow — and against a
target that does not die at all — **the Ocucor's only augment measured as
nothing** — which is exactly what a player reported: the augment's card
offered no stack count, so its damage could not be measured at all. The count is rostered only when a mod READS it: the tendrils' own
damage is cosmetic on the beam's target and is deliberately not modelled, so
without Sentient Surge a card for them would move no number.

The cap is the WEAPON's (`data/weapons/.../ocucor.yaml` `tendrils.max`), never
the mod's — the mod states only the rate, and a card carrying its own maximum
would be free to disagree with the passive that produces it.

### A buff card is THREE lists, and they have to agree

- `DummyParams::buff_roster` — what exists in the run;
- `enumerate_buffs` / `evo_buffs` (webapi) — what is drawn as a card;
- `DummyParams::apply_buff_config` — what the run actually OBEYS.

Deadly Efficiency was in the first two and missing from the third, so its card
was drawn, set, and dropped — for as long as it had existed. Nothing in the UI
could show that: a knob that does nothing looks exactly like a knob whose buff
is not currently up. Its clock was seeded from a literal `0.0` too, where its
three siblings seed from their card.

`every_buff_the_roster_offers_is_actually_read` now closes it generically: it
sets one roster id at a time and asserts the params CHANGED, so a buff added
later is covered without anyone remembering this note. The one exemption is
`frenzy`, applied outside these params (the api builds a `locked_buffs` entry).

### Every timed buff starts EARNED, at zero

**A buff starts full only if it is neither timed NOR consumable. Everything
else starts at 0.**

- **timed** — it has a duration, so a lull empties it;
- **consumable** — it is SPENT by being used, whatever its duration says. A
  "next shot deals X" buff is the clear case, and DEACTIVATION counts as
  consumption. An infinite duration does not save it: the
  question is whether the fight can hand it to you at t = 0, and a buff you
  have already spent is not one you are holding.

The `permanent` flag on a buff card is exactly this distinction, and it is the
only input the rule takes.

**Secondary Enervate (次要·失活) is the worked example of the consumable half.**
Untimed and UNCAPPED — a hit adds a stack of +10 flat crit chance with no
ceiling — but a big crit wipes the pile, so it starts at 0 like everything else
that can be spent. It lives in a PERK rather than in `arcane.buffs`, which is
why it had no card at all until 2026-08-03: the arcane whose entire point is a
stack count was the one you could not set. `BuffMeta.uncapped` says there is no
maximum, the card shows `/ ∞` and its input takes no `max`, and
`DummyParams::enervate_stacks` carries the configured pile into the run. In the whole data set today, exactly one buff
qualifies: **Fevered Frenzy** (the Dual Toxocyst evolution).

The modelled fight is therefore: *you have been at it a while, but you have not
been in contact for the last few seconds and are about to be.* Whatever
survives a lull is up; whatever expires in one is not.

This REPLACES the 2026-08-02 decision that every buff starts full. That one was
made to keep the buff cards uniform, and uniform they stay — the number they
open on is what changed. The reason it had to change is not the scenario, it is
that the old default asserted stacks the fight could not produce:

| target | full-start | zero-start | apart |
|---|---|---|---|
| Lv 30 | 524.80 | 520.00 | **0.9%** |
| Lv 100 | 58.81 | 49.94 | 15% |
| Lv 300 | 38.82 | 28.63 | 26% |
| Lv 1000 | 22.80 | 7.95 | **65%** |
| Lv 9999 SP | 4.87 | 1.95 | **60%** |

(Torid, Galvanized Chamber + Galvanized Aptitude + Primary Deadhead, 300 s,
60 runs, KPM.) Where kills are fast the seed is irrelevant — the fight rebuilds
the stacks in seconds and the two answers converge to under a percent. Where
kills are slow it is *everything*: at Lv 9999 you kill twice a minute, so
on-kill stacks never accumulate, and seeding them full handed the build a buff
it can never earn and then held it there for five minutes. Zero-start is not a
more pessimistic guess in that case; it is the correct one.

Which buffs open full, in the whole data set — **three, and the list is closed**:

| buff | where | why it is on the list |
|---|---|---|
| **Fevered Frenzy** | Dual Toxocyst | no trigger the sim can fire, so the count is a static choice and full is the only honest opening |
| **Reified Bane** | both Boars, the three Latos | on reload from empty, nothing takes it |
| **Fresh Havoc** | both Somas | on reload from empty, nothing takes it |

THREE BUFFS, EIGHT ROWS. The unit the allowance is granted in is the PERK, not
the weapon: Reified Bane is one buff with a different number on each of its five
weapons, so a sixth weapon inheriting it needs no new decision.
`only_the_three_named_buffs_open_full` reads the whole roster back and refuses a
fourth — written that way round because a test naming the three and checking
they open full would pass on a build that opened thirty.

Everything else — every Galvanized
mod, every on-kill/on-status arcane, Argon Scope, Sharpened Bullets, and the
Dual Toxocyst's own **Frenzy** passive — has a 3–30 s timer and is earned.

Reified Bane and Fresh Havoc are the second kind of full, and they are a
**decision** where Fevered Frenzy's is a fact. Fevered Frenzy has no trigger the sim can fire, so its count
is a static choice and there is nothing else it could open on. Fresh Havoc is
earned by an empty reload the fight performs several times over — the sim can
build it perfectly well. It opens full because keeping it up is not something a
player has to think about, so a fight that opens without it is the less
realistic of the two.

**A CLOSED LIST, and the owner's own allowance.** `card_opens_full:` on the
buff. It is a judgement about how the game is actually played, not a claim off
the card, and the two diverge: a card can say "lasts permanently throughout the
mission" for a pile that takes a hundred kills to fill, and that one should NOT
open full. So nothing derives this — not from the wiki text and not from the
shape, which would be the trap the paragraph below names from the other side:
**twenty** stacking buffs in `data/evolutions/` reach the engine with no clock
and nothing that clears them, because that is the DEFAULT for a card stating
neither.

Only the OPENING moves. The trigger still fires, the clear still clears, the
decay still decays — a buff on this list that a fight does take away is taken
away in the sim. And it is a DEFAULT, not a rule: the stack stepper on the card
takes any count, so a reader who disagrees with the allowance says so in their
own scenario, and the official benchmarks simply inherit it.

Two things carry no duration but are NOT permanent, and must never be treated
as such by inferring the rule from `duration == 0`: **Secondary Enervate** (a
ramp/reset perk) and **Secondary Surge** (affects the next shot only). They end
by their own rule, not by a timer.

`tenno_scaled` arcanes (Primary Bulwark, Primary Overcharge) are not buff cards
at all. Their value is a Warframe stat, not a stack anyone earns or loses; they
ride the buff machinery to reach their bucket and that is an implementation
detail. Their control is WF Armor / WF Energy in the Tenno block.

## What actually guarantees correctness

The architecture does not "prove" a buff right — it makes each perk
**isolated, deterministic, and individually testable** so tests can pin a bug to
one perk. Correctness comes from tests at two levels:

1. **Perk state-machine tests** — pure logic, no game needed: e.g. "buff resets
   after N big crits", "stack gain capped at 30/s", "duration buff expires".
2. **Golden tests vs Simulacrum** — the north star. Full sim with the buff active
   vs a recorded in-game damage-vs-time trace, matched within rounding. A
   mismatch means the data or engine gets corrected in place.

Supporting rules:

- **Determinism** — one seed per engagement, fixed tick rate, no ambient
  randomness. Makes Monte Carlo reproducible and golden tests stable. (Critical
  here because random big crits can *feed back* into a buff's own reset.)

  The seed drives THREE streams rather than one (`rng::Draws`):
  `spine` (multishot, crit tier, promotion, body part), `status` (whether a hit
  procs and with what), `extra` (buff triggers, arcane rolls). One stream made
  the sim answer "what does this mod change?" much more loudly than the mod: a
  status chance high enough to land one more proc drew one more number to pick
  its element, and every crit after it was a different draw, so two builds that
  differed in nothing that pays came back differing anyway. Splitting them is
  what makes a paired comparison mean something — a status-only change now
  leaves the damage decisions bit-identical.
- **Trace output** — the sim can emit a per-event log (buff bar over time, crit
  tiers, per-bucket contributions) to diff against an in-game trace and localize
  any mismatch. You cannot align what you cannot inspect.

## Three questions about a buff are not three copies of it

A buff is asked three things, in three places, and it is tempting to read that
as duplication to be collapsed:

| where | question |
| --- | --- |
| `DummyParams::buff_roster` | does this build have it, and how big does it get? |
| `DummyParams::apply_buff_config` | what does the card's setting do to it? |
| `sample_stacks` | what is its live count at this instant? |

Plus `enumerate_buffs` in `webapi`, which asks what card to draw for it.

**The evolution-granted family really is one shape** and is derived: a
`StackingBuff` declares its trigger, grant, decay, cap and duration, and all
four sites loop over the vector. A perk added to `data/evolutions/` needs no
line in any of them.

**The mod-granted ones are not, and folding them in was tried and rejected
.** They do not share a scaling rule, and the differences are the
mechanics rather than an accident of where the code was written:

- `cc_on_headshot` and `cc_stack` feed the RELATIVE crit bucket — a fraction of
  the UNMODDED base (`ap.unmodded_crit_chance * cc_rel`), beside Pistol Gambit;
- `fr_on_reload` adds to the live fire rate and is then multiplied by the buff
  bar, and a LOCKED fire rate bypasses it entirely;
- `bd_on_reload` is a live share of the BASE-DAMAGE bucket;
- `StackingBuff`'s own `FireRate` grant is an ABSOLUTE rate, converted per form
  at resolve time.

Giving `StackingBuff` a "which bucket, at what scaling, and how does a lock see
it" field would put every one of those differences back inside the type, and a
type that is a union of unlike things is worse than three honest lists.

What DOES have to be guaranteed is that the lists agree, and that is done by
check rather than by construction — three derived tests, each of which walks
the roster instead of naming buffs:

- `every_buff_the_roster_offers_is_actually_read` — roster ↔ config;
- `no_rostered_buff_draws_a_flat_zero_it_did_not_earn` — roster ↔ replay
  (`sample_stacks` ends in `_ => 0`, so a missing arm draws a flat line rather
  than failing);
- `card_and_sim_agree` in `webapi` — card ↔ roster, over every weapon-mod and
  weapon-arcane pair.

Each was verified to FAIL when its arm is deleted. Adding a buff to one place
and forgetting another is therefore a red test, not a silent wrong number —
which is the property the collapse was wanted for.

## A WARFRAME ABILITY is a buff nobody in this repo grants

Roar, Eclipse, Nourish, Xata's Whisper and the four elemental augments
(`data/abilities/`) are
buffs in the ordinary English sense and **not** `Buff`s in the sense the rest of
this document uses. Nothing here grants them: there is no perk holding them, no
trigger that fires them, no bar they appear in. They are a property of the
FIGHT — a thing another actor, or your own frame, is doing to this weapon for a
while — and they arrive on [`Arena`] beside the target and the duration.

That is the whole design, and everything else follows from it:

- **The optimizer gets them for free.** `parse_fight` is the one module both the
  simulator and the search read, so a candidate is scored under the same Roar
  the replay will run. No optimizer code mentions abilities.
- **A build never carries one.** Two builds compared under the same Roar is a
  comparison; one of them getting it is not.
- **The board sends none.** A ruler that cast Roar would make its board a
  statement about Rhino. `check_wf_buffs.mjs` asserts it as a negative control.
- **They are not in the buff bar**, and should not be: the bar shows what this
  build gained during the run. An ability you cast is an input to the run.

### Four effect kinds — three multipliers and one INSTANCE

Each is a different BUCKET, and the differences are quoted rather than assumed
(`data/abilities/*.yaml` carries the sentence and the page it came off):

| kind | example | where it lands | on a status tick |
|---|---|---|---|
| `faction_damage` | Roar +50% | the bracket a Bane mod is in | **twice** — the bracket double-dips |
| `final_damage` | Eclipse +200% | its own multiplier | once |
| `add_element` | Shock Trooper +100% Electricity | the FINISHED vector | its own element's DoT |
| `extra_hit` | Xata's Whisper +26% Void | **nowhere — it fires a second instance** | rolls its own, independently |

**The split that matters is three-and-one, not four.** The first three are
multipliers: whoever needs one reads it at the point in the pipeline where it
belongs, and no caller has to know the ability exists. An `extra_hit` is a
damage INSTANCE, so something has to FIRE it, and that something has to know
what triggered it — which body part was struck, how many faction layers the
trigger already carried, whether it was a weapon hit at all. `fire_extra_hits`
takes those as arguments for exactly that reason, and MECHANICS §7 §"Extra Hit"
is where the rules live. Expect the next `extra_hit` (Toxic Lash, Silken Stride,
Resupply) to be a data file and nothing else.

The first two differ by one wiki sentence and it is worth stating twice:
*"Unlike faction damage, which double dips for status effects, the one from
Eclipse is applied once."* Getting that wrong is a factor of three on a DoT
weapon, and `roar_is_used_twice_on_a_status_tick_and_eclipse_once` is the test.

`add_element` is the one with a shape of its own. **It does not combine**
 — every one of the four augment pages says so — so it is
added AFTER `elements::combine` has run: a weapon whose mods make Radiation,
under Volt, deals Radiation *and* pure Electricity. It is still **sized** like
an elemental mod ("additive with elemental mods"): a percentage of that attack
part's own ModifiedBase, which is why an explosion's share differs from the
direct hit's, and why it also raises that element's DoT bracket.

### Strength, duration, and what moves when frames land

Two inputs, both supplied by the caller rather than read from anywhere:
`abilities_data::resolve(picks, strength)`. Today the page asks for an Ability
Strength and a per-buff duration; when Warframes are modelled, the strength comes
from the frame and the duration from its Ability Duration. **The definitions do
not change then** — that is the point of taking both as arguments, and the reason
the section says "early access" on screen rather than only in a comment.

### The one rule a page cannot be trusted with

Same `family:` → only the strongest runs. The wiki states it on Freeze Force —
*"Multiple Freeze Forces do not stack; the buff with the highest Ability Strength
will take effect"* — and the owner asked for it by name for Roar vs Roar
(Helminth). `resolve` settles it once, comparing by RESOLVED value so a
200%-strength Helminth Roar beats an unbuffed Rhino's, and the page draws the
loser dimmed with a line saying why. Adding two Roars would be +80% against +50%,
which is a 20% error nobody spots in a DPS figure.

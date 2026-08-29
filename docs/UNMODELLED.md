# What is not modelled, and what would have to exist first

A catalogue of the EDGES, so that "why is this perk worth nothing" is a lookup
rather than an investigation.

It is deliberately not a list of perks — `python scripts/intake_report.py --full`
prints that, per weapon, derived from the data and therefore never stale. This
is the other half: the half-dozen REASONS behind every entry on that list, and
what each one is waiting for. A gap with no class here is a gap nobody has
thought about yet, which is itself worth knowing.

Everything here is also on the PAGE — a weapon banner, an evolution chip, a mod
line (`scripts/check_disclosure.mjs`). This file is for deciding what to build;
the page is for a player deciding whether to trust a number.

---

## Saying WHICH kind of gap it is, in the data

A perk clause declares its class rather than being sorted into one by a reader:

```yaml
  - kind: out_of_scope
    clause: "On Shield Break: Increase Base Damage by +80 for 8 seconds"
    reason: nobody_shoots_back
```

`reason:` is one of the seven slugs below (`one_target`, `no_distance`,
`no_movement`, `no_holster`, `infinite_ammo`, `nobody_shoots_back`,
`warframe_abilities`) and the engine refuses one it does not know, so a clause
cannot claim a class this file has not written down.

**It is a different admission from `unmodelled_*`, and the page says so.** An
`unmodelled_*` kind is a TODO — work someone can do, counted by the ratchet
(`the_number_of_unmodelled_evolution_effects_only_goes_down`), gold on the tile.
An `out_of_scope` is an EDGE: nothing about this engine will close it, it is off
the ratchet, and its chip is muted and reads "nothing to earn here" with the
reason in the tooltip. The mods have had this split since 2026-08-05 for the
reason it was added here on 2026-08-12: printing "not modelled yet" over both is
what makes the whole app look unfinished, and it tells a player nothing about
whether to wait.

The first one caught a real mis-reading on the way in. Hoplite Virtue's "On
Shield Break" is on **six** guns, and only the Gorgon's page says which shield:
*"This is on personal shield break, not breaking enemy shields."* The Lex's page
says nothing at all — so reading the weapon you are working on would have left
it a coin flip between a trigger this sim fires constantly (an enemy's shields
go down every fight) and one it can never fire.

## The classes

### 1. ONE TARGET

The arena has a single enemy. Anything whose payoff is a second body is worth
exactly zero and always will be, until the sim is spatial.

- punch through (innate, mod-granted, or perk-granted) — the Felarx's infinite
  body punch-through, Dual-Mode Chamber's +4 m;
- "on punch through N enemies" triggers — Ruptured Plentitude, and **Unseen
  Dread**, whose invisibility needs one shot to strike three enemies. Its
  CRITICAL DAMAGE half still pays, because the wiki says the bonus takes
  invisibility "from any source" — so the mod is partly modelled rather than
  filed here whole;
- a SECOND ENTITY beside the target: **Neutralizing Justice**'s Nullifier
  generator (the wiki: it "has no effect on any other enemy in Warframe"), and
  **Double Tap**'s object-hit reset, which needs a bubble or a tornado to hit;
- chaining, radial spread onto others, corpse effects.

**What would have to exist first:** more than one body. That is the same change
AoE geometry needs (see `docs/CORE.md` and the AoE reference in memory), and it
is the single biggest structural gap in the model.

### 2. DISTANCE — MOSTLY MODELLED SINCE 2026-08-15

The fight has a range. The arena carries two POINTS rather than a distance
(`engine::space`), the scenario's *Distance (m)* sets it, and **both official
rulers pin it at 0**, which is the fight every board row and every golden value
was measured under — so nothing moved when this landed.

What it turned on:

- **damage falloff** on the direct hit — the published window per weapon, with
  Projectile Speed scaling the window (the first thing that stat has ever been
  worth here). 23 admissions closed;
- **spread**, so a shot can MISS. The cone comes per ATTACK from the wiki's own
  weapon module (`MinSpread`/`MaxSpread`, degrees from the reticle) rather than
  from the Arsenal's derived `Accuracy` scalar, which is one rounded number per
  WEAPON and therefore cannot describe a form at all;
- **radial falloff**, because a missed projectile detonates beside the target
  and the explosion finally has an epicentre distance to read. 10 admissions;
- **the shot combo counter dropping on a miss**, which is the other half of a
  mechanic that previously only decayed on its timer.

What is still open here:

- **THE BLOOM.** `min` is *"Deviation With Aim"* and `max` is where sustained
  fire takes the cone; the ramp between them is published nowhere, so a pellet
  draws inside the AIMED cone and `max` is carried unused. A weapon held on the
  trigger is therefore more accurate here than in game, most visibly on the
  widest windows — every sniper is `0 / 15`. Inventing a bloom rate for 224
  entries is exactly the kind of thing this file exists to refuse;
- **the model's one free parameter**, `space::BODY_RADIUS_M` — 0.2 m, a guess
  and stated as one. THE PLANE IS THE MODEL rather than an approximation of a
  solid to be corrected: the geometry answers only "did the
  pellet reach the target", and where a landed pellet went is `headshot_pct`'s
  question, already pinned per pellet. So there is one circle and one number,
  and one Simulacrum measurement settles it — a counted number of pellets, a
  known range, a weapon of known spread, count what lands;
- **17 entries have no transcribed spread**, down from 62. Sixteen of them are
  SENTINEL weapons, and that one is a source gap rather than a matching one:
  the wiki's companion module carries no `MinSpread`/`MaxSpread` for any of
  them. The seventeenth is the Miter, whose weapon publishes a cone for its
  uncharged shot and its Incarnon form and none for the charged shot our entry
  IS — so the intake takes nothing rather than the neighbouring one. Each says
  so (`spread_not_transcribed`); re-running `scripts/intake_spread.py` only
  ever lowers the count and a test holds the ceiling;
- **beam RANGE** — a beam still reaches whatever it is aimed at;
- **Dizzying Rounds**' stun, which applies "from less than 8m" — the distance
  half is answerable now and the STUN half is not: it opens a finisher, and
  nothing here takes one. Filed under `nobody_shoots_back`'s neighbourhood
  rather than here. Its status chance is the half that pays.

**AND FIVE CLAUSES STOPPED BEING EDGES** the day the range landed, which is the
worse half of adding a mechanic: an `out_of_scope` declaration that is no longer
true tells a player to distrust a number that is now right. All five were
rewritten as real effects the same day:

| clause | was | is |
| --- | --- | --- |
| Lone Enforcer (Vectis, Vectis Prime) — *"+25% Multishot if no enemies are within 5m"* | `no_distance` | `multishot_beyond_range`, settled against the arena in `DummyParams::from_panel` |
| Hunter's Mantra (Boltor, Boltor Prime, Telos Boltor) — *"With Channeled Ability active: +40% Accuracy"* | `no_distance` | `GatedGrant::Accuracy` — a narrower cone, so more pellets land |

Both are worth exactly zero at point blank, which is why no board row moved.
The OTHER half of Hunter's Mantra (Punch Through +4) is still an edge and still
says so: it needs a second body.

The three `no_distance` clauses left are all the same one — Moonrise Velocity's
*"Increase Range by +7/+8"* on the Atomos and the two Gammacors — and they are
waiting on beam range, above.

### 3. NO MOVEMENT, NO STANCE

The wielder aims and fires. They do not slide, aim-glide, bullet-jump, or get
knocked down.

- Agile Executor (ammo efficiency while aim gliding and sliding);
- "with sprint speed or higher" perks;
- self-stagger from one's own explosions (Cautious Shot);
- Stagger as a proc — it is deliberately dropped when transcribing
  `ForcedProcs`, because a knockdown has nothing to act on here.

**What would have to exist first:** a wielder with a position and a state
machine. `data/tenno/` is the seam; `TennoState` already carries `aiming`,
`invisible`, `airborne` and every `condition:` reads them, so a fourth state is
cheap — the mechanics behind them are not.

### 4. NO HOLSTER, NO SECOND WEAPON — BUT THE LOADOUT IS FULL

One weapon FIRES for the whole engagement.

- Evolved Autoloader (+50% magazine per second while holstered);
- swap-speed perks, "on swap" buffs;
- anything that assumes a loadout rather than a gun.

**What would have to exist first:** a loadout and a swap policy. The policy is
the hard half — see §"Open decisions" below, it is the same problem.

**THE OTHER SLOTS ARE OCCUPIED**. What the Tenno FIRES and
what the Tenno CARRIES are two different facts, and only the first is "one
weapon". The ruling answers every clause about the other slots at once, in both
directions:

| clause | answer | why |
| --- | --- | --- |
| Lone Gun (Vasto, Vasto Prime) — *"With No Primary Equipped"* | **no** | there IS a primary |
| Stalker's Vendetta (Despair) — *"With Dread and Hate equipped"* | **no** | the slots are full but unspecified |
| Stalker's Resentment (Dread) — *"With Hate and Despair Equipped"* | **no** | same |

That makes Lone Gun's condition ANSWERED rather than unreadable, which is worth
the distinction: the sim is not declining to evaluate it, it evaluates to false.

**…AND THE ANSWER IS NOW A SETTING**. The scenario carries
`solo_weapon` — *Only this weapon* on the Technique block, off by default — and
the table above is what OFF means. It is a `TennoGate` like `overshields` and
`channeling`, so it is asked of the fight's Tenno and travels with the
scenario, the panel, the optimizer and a share link
with no code of its own anywhere.

Ticking it does NOT move this section into the modelled column; it splits it:

| clause | full loadout (default) | only this weapon |
| --- | --- | --- |
| Lone Gun — *"With No Primary Equipped"* | no | **YES** — +40 base damage, +14 base magazine |
| Stalker's Vendetta / Resentment — *"With Dread and Hate equipped"* | no (unspecified) | **no**, and now definitively |
| Deathtrap Trigger — *"On Equip From Primary"* | no (we never swap) | **no** — there is no primary to equip from |
| Evolved Autoloader and the *"while Holstered"* family | no (we never swap) | **no** — there is nothing to holster to |

So one clause becomes reachable and four become IMPOSSIBLE rather than merely
unmodelled, which is a better state for both: the second column has no swap
policy left to invent (§"Open decisions"), so those rows are edges by the
setting's own logic rather than by ours.

Lone Gun is the only card in the roster the setting turns on today, and that is
the honest scope of it — the option exists so the next one does not need a
ruling, and
`loadout::tests::lone_gun_pays_its_two_halves_only_with_no_other_weapon` asserts
the list is CLOSED so a card that spells the condition some other way fails
rather than paying nothing in silence.

### 5. AMMO IS INFINITE BY DEFAULT

`infinite_ammo` defaults on because the sim models no
pickups, so a finite reserve is the pessimistic half of a mechanic we only half
have. Ammo economy perks are therefore worth ~0 in the headline number even
though the machinery for them exists and works when the setting is off.

- ammo efficiency perks and arcanes;
- ammo mutation, reserve size.

**What would have to exist first:** ammo pickups, or a scenario that means to
run the reserve dry. The reserve itself IS modelled — every draw inside the
Incarnon cycle was made to bill it.

### 6. NOBODY SHOOTS BACK

The target has no attack, so the player has no incoming damage, no shields to
break, no overguard to gain.

- Secondary Fortifier (overguard per damage dealt);
- health/shield/overguard gating on the wielder;
- HEALING — Winds of Purity's life steal, and **Bhisaj-Bal**'s 300 health per
  three status effects. There is no pool for it to go into;
- CROWD CONTROL, which is the mirror of the same fact: the target never acts, so
  stopping it acting is worth nothing. **Metamorphic Magazine**'s petrify and
  **Dizzying Rounds**' stun are here; both cards are equipped for their other
  half (magazine/ammo, status chance);
- `data/tenno/default.yaml`'s `health`/`shield` are placeholders at 1 for
  exactly this reason.

**What would have to exist first:** the target as an attacker. The `Arena`
already has two actors, which is the seam.

### 7. WARFRAME ABILITIES, BEYOND THE BUFFS

`data/abilities/` covers seven damage buffs (Roar, Eclipse, Nourish and the four
elemental augments) as an EARLY-ACCESS block on the scenario. What it does not
cover is everything else a frame does — armor strip, ability damage, energy
economy, the GunCO "Adding" omission list (Vex Armor, Furious Javelin,
Parasitic Link — MECHANICS §6).

**What would have to exist first:** a Warframe, with stats and mods of its own.
When it lands, `abilities_data::resolve`'s two inputs (strength, duration) come
from the frame and nothing about the buff definitions changes — that is why they
are arguments.

## What the ratchet has left

**Five inert evolution clauses**, down from 226 in early August and 51 at the
start of this pass, and none of them is waiting on the evolution layer:

| clause | what it is actually blocked on |
|---|---|
| Neurotoxin (Dual Toxocyst) | DE's own wiki says *"Currently does not work"*. Measure it after they fix it. |
| Dual Mode Chamber (Felarx) | an OPEN DECISION — see below. |
| Devastation Cascade (Onos) | every stack pays out on the fully charged blast, an ATTACK PART the weapon entry does not carry. |
| Precision's Payoff (Zylok, Zylok Prime) | needs the DUPLEX trigger, which `data/weapons/secondary/zylok.yaml` declares as its own gap: *"one pull fires TWO rounds in game and this entry paces one per pull"*. |

That shape is the useful part. A gap in this list is a pointer at a named thing
somewhere else — a DE bug, an unwritten ruling, a missing attack part, a missing
firing mode — rather than "nobody got to it", which is what the count meant when
it was in the hundreds.

**Two PLAYER STATES were added to close the biggest groups**, both declared
rather than observed, because this arena fires one weapon and casts nothing:

- `overshields` — ten cards (Haven Foray, Guardian's Might) read it. Nothing
  here takes them away, so it is a declaration; earning them mid-fight is a
  separate clause and stays out of scope under `nobody_shoots_back`.
- `channeling` — seven cards (Daring Reverie, Hunter's Mantra). The card's own
  note defines it and the control carries the definition: the ability must be
  DRAINING ENERGY over time, so Desecrate, Haven and an empty Gloom do not count.

---

## A SHOT THAT CROSSES NOBODY LEAVES NO SPHERE

Aim is a place (MECHANICS §11), and pointing at bare floor is a legal shot that
deals ZERO — *"if it hits, it hits, and if it does not, it is zero"*. That much is modelled, and the api stopped refusing such a shot on
the same day.

**WHAT IS NOT** is the sphere it should leave where it landed. A weapon with a
damage radius that strikes nobody still detonates on the floor in game, and
anyone standing near that spot takes it — which makes "aim BESIDE the crowd so
the splash catches more of them" a real tactic. Here it is worth nothing,
because a missed shot produces no instance at all.

**WHY IT IS NOT A ONE-LINE FIX.** The miss is a `continue` that skips the whole
direct stage, and it skips it for good reasons named at the site: the status
draw, the gauge charge, the on-kill buffs and the combo count all live inside
it, and a hit that dealt zero is not what a miss is. It was tried by keeping the
instance alive with its damage zeroed, and that made a MISS proc status and
charge the gauge. Doing it properly means separating "compute this instance"
from "apply its consequences", or giving the sphere a STAGE of its own beside
the radial's — the second is the shape the code already has and is the way in.

Aiming AT a body, or within a body's width of one, is unaffected: the line
crosses it, the shot lands, and the sphere seeds from its surface as it should.

## Open decisions, not missing machinery

These are things the engine COULD do today and deliberately does not, because
doing them means inventing a play pattern. The repo's rule is that a policy is
the owner's call, not the model's (the 99-stack decision, 2026-08-08).

### Bullet Attractor — OPEN, and it wants a MEASUREMENT (Scourge, Scourge Prime)

*"Alternate Fire throws the Scourge … causing a 2 meter Bullet Attractor field
on the heads of enemies within 14 meters of the impact point, allowing easier
headshots."*

**The CO half is DONE (M42): the throw plants the debuff.** `attractor_seconds:
4.7` on both thrown entries, feeding `DebuffState::attractor` — the same debuff
Xata's Whisper's Void instance lands, which is not an analogy but the same
effect. It is worth exactly one line in the Condition
Overload counter, and 4.7 s against a ≤1.6 s throw cycle means it is up
continuously.

**What stays open is the other half**: what the field is worth as an AIMING
AID. This arena has a headshot rate — `headshot_pct` on the fight — so the
wiring is again three lines.

**What is missing is the number, and the wiki says so itself:** *"Bullets and
projectiles fired at enemies will be drawn to the head. However, this does not
guarantee a headshot."* Setting the rate to 100% would be inventing the figure
the page refuses to give; leaving it at the scenario's own is what happens
today. Either is a decision, and only a measurement settles it — fire a known
number of shots into a field and count the heads (docs/MEASUREMENTS.md).

It is filed here rather than in the list above because nothing about the arena
prevents it: no distance is involved (the field is on the target), no play
pattern has to be invented (you throw, then you fire), and one weapon in the
roster plants one on itself.

**The UPTIME is measured too (M42), and the two clocks pull opposite ways.** The
old FIELD dies when the next throw STARTS, not when the new spear lands, so its
20 s life and every-5-s pulses are ceilings a throw build never reaches — one
field there never lives long enough to pulse twice. But what a field already
applied is NOT taken back with it, and 4.7 s on the target outlasts the 1.6 s
cycle, so the debuff is continuous exactly where the field is shortest. When the
headshot half is finally wired, it is the TARGET's clock it should read, not the
field's.

### Dual Mode Chamber — OPEN (Felarx)

*"Reload toggles the weapon between +100% Projectile Speed and +4m Punch
Through."*

A TOGGLE is a play pattern, and picking a side for the player is the same class
of decision as reload interruption. Half of it is an edge and half is not, which
is what keeps it here rather than in the list above: punch through buys a second
body this arena does not have, but projectile speed is a stat the engine models
where it changes a pool. A perk that is half an edge is not an edge.

What is needed is a ruling, not machinery: "which side is the build in", or "it
alternates and the sim should model both halves of the cycle".

### Reload interruption — DECIDED: never interrupt

**The Felarx, and every by-round reloader.** In game you can fire mid-reload and
keep the shells already loaded; here a reload runs to the end. The machinery was
never the problem — the sim owns the shot schedule and a by-round reload is
already per shell — but *when* to interrupt is a POLICY, and the two extremes
disagree:

- **never interrupt** — every reload is a full magazine, and on the Felarx a full
  magazine of Mounting Momentum stacks with it;
- interrupt as soon as one shell is in — fastest back to firing, and worst for
  any per-shell buff.

**Never interrupt is the ruling.** It is the reading the weapon's
own numbers describe — a listed reload is a whole reload — and it is the one that
needs no play pattern invented for it. The alternative is not "more accurate", it
is a different player.

It stays in this file because it is still a thing the model does not do, and the
weapons that reload by round say so on their own cards: a player who interrupts
is trading stacks for uptime and will not reproduce these numbers exactly.

### Playing around a SPOOL — the SAME shape, DECIDED the same way

Six weapons spool (MECHANICS §9). Every spool is modelled; what is not, on any
of them, is the play pattern that dodges it:

- the Phenmor's Incarnon form FALLS to 60% over 51 held shots and resets when
  you stop firing, so a player who taps keeps more of it;
- the Gorgons and Somas CLIMB from 20-25%, and their pages say *"Burst firing
  maintains spool-up"* — so a player whose pauses are short enough may keep the
  climb through a reload, where this sim rebuilds it.

**The sim holds the trigger until the magazine is dry**, and the same ruling
covers both — the same one as the reload above, for the same reason. It is the
reading that needs no play pattern invented for it. Naming a burst length, or a
pause short enough to keep a spool, would be inventing the very thing the ruling
refuses to invent (owner, 2026-08-10 — taken to the limit, a weapon
exempted from its own spool is a weapon fired one round at a time).

Each weapon's own card says so, which is what makes the decision reachable: the
`unmodeled:` line carries the caveat and the panel's passive strip states where
the rate starts and ends, beside the single number the stat panel prints.

### The MANUAL reload — and what it costs Ready Retaliation

This sim reloads when it cannot fire, and never a round earlier. A player
reloads whenever it suits them, and on one perk that is the whole difference:

**Ready Retaliation** ("On Reload from Empty: +100% Reload Speed for 6 seconds")
is implemented and correct — pressing reload on an empty magazine arms it, so
that reload is already faster and every later one is too. Measured on the
Phenmor's base form: **+17.4% stock** (27 reloads become 30) and **+19.5% on a
fire-rate build** (58 become 67), the best tier-3 option in both cases.

The gap is the **Incarnon cycle**, where it is worth zero for a different reason:
the base form transmutes on a full gauge and therefore never empties, so no
window ever opens. The wiki's own note describes the play that fixes this —
*"Can affect transition into Incarnon form with a well-timed manual reload"* — and
a well-timed manual reload is precisely what this arena has no notion of. Same
ruling as everything else in this section: naming the moment to reload would be
inventing the play pattern.

Worth knowing because it changes what the numbers MEAN: a zero here is "under
this play pattern", and on this perk a player who reloads deliberately is buying
something real.

### The RELOAD'S TAIL — OPEN, and it wants a MEASUREMENT (every weapon)

A reload does not finish when its animation does. The magazine is actually full
at somewhere around **80–90% of the way through**, and what is left is a
recovery the player sits through. This engine models the
reload as one block of dead time with the magazine arriving at the END.

**Nothing computed today is wrong because of it.** The total is the same number
either way — the published reload time — so the shot cadence, the downtime and
every throughput figure are unaffected, which is why this is written down rather
than fixed.

What it WOULD move is when a reload-complete buff's window opens: earlier by the
length of the tail, on every perk that fires on `ReloadComplete` or
`ReloadFromEmpty`. On a buff with a clock that is a few tenths of a second of
uptime per reload; on a buff without one it is nothing at all.

The missing thing is a number, and it is per weapon: the real split between the
two halves. Until somebody measures it, a made-up 85% would be a precision
nobody checked — which is worse than the honest block this has now, because it
would read as measured. Recorded here so the question survives (see also
docs/MEASUREMENTS.md when it is answered).

### The 99-stack Mounting Momentum edge

Reaching the cap needs a magazine that never empties, which is a firing pattern
rather than a property of the perk. DECIDED against, twice. Written
here so it is not re-proposed.

---

## What is NOT on this list, and why that matters

A perk absent from every class above and still marked "not modelled" is a TODO,
not an edge — it is real damage the sim does not compute yet. Those are the ones
worth building, and `cargo test -p wfsim-engine the_number_of_unmodelled` is the
ratchet that keeps their number going one way. Two of them were retired the day
this file was written:

- **Mounting Momentum's Incarnon route** — entering the form is a reload, so it
  pays a reload's stacks (one shell in, the rest out). Modelled 2026-08-09.
- **Incarnon Catalyst / Incarnon Efficiency's "transmutation charge"** — never a
  gap at all: the intake's clause splitter cut one sentence in half and filed
  the tail as unmodelled, so a fully-modelled perk printed "partly modelled"
  about its own second clause.

## The SPATIAL audit

Every effect in `data/` whose payload is about REACH, checked against whether
the engine reads it. Prompted by "review what is missing now that a formation
exists", and done by walking the data's own `kind:` vocabulary rather
than from memory — 22 spatial kinds across mods, arcanes and evolutions.

### Reaching the sim

| kind | where | what reads it |
|---|---|---|
| `punch_through_bonus` | 17 mods + evolutions + rivens | `panel.punch_through_m` (MECHANICS §13) |
| `blast_radius_bonus` | 4 mods (Firestorm family) | `radial.radius_m` and `lingering.radius_m` |
| `aoe_echo` | 1 arcane (Secondary Irradiate) | `spread_from_echo` |
| `multishot_beyond_range` | 2 evolutions (Lone Enforcer) | `panel.multishot_beyond_range` |
| `accuracy_bonus` | 28 | the spread cone |
| `projectile_speed_bonus` | 4 | the falloff window |

`spread_audit` counts 101 of the roster's 224 entries reaching a formation now
— 7 beams, 54 explosive, 39 punch-through, 1 tendrils — against 62 before punch
through was read.

### LOADED AND NEVER READ — the gaps

**1. BEAM RANGE, and it is the biggest.** `beam.range_m` is in every beam
weapon's data and **nothing in the sim reads it**, so a beam reaches any body on
the line at any distance. On the group-clear ruler the far rank is 27 m out
while the Atomos is a 15 m beam and the Torid Incarnon a 37 m one — two weapons
the model cannot currently tell apart. The three mods that move it are unread
for the same reason: **Sinister Reach** (+12 m), **Ruinous Extension** (+8 m),
**Galvanized Acceleration** (+30%, and it stacks on kill).

It is the same shape as the punch-through gap was: a stat that sat in the data
unread because the arena had one body, and that a crowd makes load-bearing.

**2. `explosion_on_kill` — Combustion Beam.** *"Enemies killed explode, dealing
600 Damage shortly after death."* Worth exactly nothing against one target,
which is why it was filed as indirect; against a formation it is a chain
reaction and the mod's whole identity.

**3. `status_spread_chance` — Shivering Contagion.** *"On Cold Status Effect:
100% chance to spread that status to other enemies within 6m."* A pure formation
mechanic — there was no second body for a status to spread to.

**4. `range_bonus` — Ballista Measure** (+20% Range, Arch-Gun). What DE means by
"Range" on an Arch-Gun is not settled by the card, which is its own reason to
leave it: a number applied to the wrong quantity is worse than one applied to
nothing.

### Fixed while auditing

**A punched body reads its OWN damage falloff.** It inherited the aimed body's,
which understates the drop for a body further down the line — the punch-through
work landed with the aimed pellet's factor still inside `raw_per_bucket`. It is
divided back out and the body's own put in, the same arithmetic the blast does
with its epicentre's. 1.0 for the whole roster minus nineteen entries, and 1.0
at contact for all of them.

**A chain hop still inherits the direct hit's falloff, and that is deliberate:**
a hop is defined as a percentage of the previous instance's damage, so it is a
share of what the beam delivered rather than a shot of its own.

## A BUFF CAN DEPEND ON THE WEAPON BEING IN YOUR HANDS

**Open decision**. Reported off the Grimoire's Invocations:

> 次要射击的时候，如果我装了一个vome的，是不是理论上每次攻击都会叠层，但是如果我
> 期间切换武器，整个叠层就不会生效，但是我切回去，叠层又可以了。那就说明有些东西
> 生效完全取决于当前的武器是不是在场的

An Invocation's stacks are earned by the tome's alt fire and are worth something
only while the tome is OUT. Swap to the primary and the buff stops paying; swap
back and it pays again — the stacks are not lost, they are dormant.

THIS ENGINE HAS ONE WEAPON AND IT IS ALWAYS OUT, so nothing can be dormant and
the question cannot arise. It is recorded here rather than in a comment because
it is not a gap in a number — it is a rule the model has no PLACE for, and it
becomes a real decision the day weapon swapping lands:

* a buff would need to know whether its SOURCE is equipped, which is a third
  state beside "up" and "expired"
* a swap would have to be a play pattern somebody chose — how long you spend on
  each weapon is exactly the kind of thing this file says is the owner's call
  and not the model's

It is written down now because the observation is cheap to lose and expensive to
rediscover: every buff card in the roster is currently modelled as paying
whenever it is up, and that is right for one weapon and wrong for two.


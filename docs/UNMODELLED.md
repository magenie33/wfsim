# What is not modelled, and what would have to exist first

A catalogue of the EDGES, so that "why is this perk worth nothing" is a lookup
rather than an investigation (owner, 2026-08-09).

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

### 2. NO DISTANCE

Every shot lands at point blank.

- damage falloff (the Felarx's 14→28 m to 99%, and every shotgun's real one);
- Projectile Speed as a DPS stat — it is modelled where it changes a *pool*
  (riven rolls) but it moves no damage number here;
- range-gated perks, and **Dizzying Rounds**, whose stun applies "from less than
  8m": with no distance the condition is neither true nor false. Its status
  chance is the half that pays.

**What would have to exist first:** a distance on the scenario. Cheap to add and
expensive to be right about — falloff is per weapon and the data is only
partly transcribed (`FalloffSpec` exists; most weapons do not carry one).

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

**THE OTHER SLOTS ARE OCCUPIED** (owner, 2026-08-12). What the Tenno FIRES and
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

**…AND THE ANSWER IS NOW A SETTING** (owner, 2026-08-13). The scenario carries
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

`infinite_ammo` defaults on (user, 2026-08-01) because the sim models no
pickups, so a finite reserve is the pessimistic half of a mechanic we only half
have. Ammo economy perks are therefore worth ~0 in the headline number even
though the machinery for them exists and works when the setting is off.

- ammo efficiency perks and arcanes;
- ammo mutation, reserve size.

**What would have to exist first:** ammo pickups, or a scenario that means to
run the reserve dry. The reserve itself IS modelled — every draw inside the
Incarnon cycle was made to bill it (2026-08-04).

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

## What the ratchet has left (2026-08-12)

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
effect (owner, 2026-08-14). It is worth exactly one line in the Condition
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

### Reload interruption — DECIDED: never interrupt (owner, 2026-08-10)

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

### The 99-stack Mounting Momentum edge

Reaching the cap needs a magazine that never empties, which is a firing pattern
rather than a property of the perk. DECIDED against, twice (2026-08-08). Written
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

# What is not modelled, and what would have to exist first

A catalogue of the EDGES, so that "why is this perk worth nothing" is a lookup
rather than an investigation (owner, 2026-08-09: "其他地方就先记录下来，等需要时候
可以快速找到").

It is deliberately not a list of perks — `python scripts/intake_report.py --full`
prints that, per weapon, derived from the data and therefore never stale. This
is the other half: the half-dozen REASONS behind every entry on that list, and
what each one is waiting for. A gap with no class here is a gap nobody has
thought about yet, which is itself worth knowing.

Everything here is also on the PAGE — a weapon banner, an evolution chip, a mod
line (`scripts/check_disclosure.mjs`). This file is for deciding what to build;
the page is for a player deciding whether to trust a number.

---

## The classes

### 1. ONE TARGET

The arena has a single enemy. Anything whose payoff is a second body is worth
exactly zero and always will be, until the sim is spatial.

- punch through (innate, mod-granted, or perk-granted) — the Felarx's infinite
  body punch-through, Dual-Mode Chamber's +4 m;
- "on punch through N enemies" triggers — Ruptured Plentitude;
- chaining, radial spread onto others, corpse effects.

**What would have to exist first:** more than one body. That is the same change
AoE geometry needs (see `docs/CORE.md` and the AoE reference in memory), and it
is the single biggest structural gap in the model.

### 2. NO DISTANCE

Every shot lands at point blank.

- damage falloff (the Felarx's 14→28 m to 99%, and every shotgun's real one);
- Projectile Speed as a DPS stat — it is modelled where it changes a *pool*
  (riven rolls) but it moves no damage number here;
- range-gated perks.

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

### 4. NO HOLSTER, NO SECOND WEAPON

One weapon fires for the whole engagement.

- Evolved Autoloader (+50% magazine per second while holstered);
- swap-speed perks, "on swap" buffs;
- anything that assumes a loadout rather than a gun.

**What would have to exist first:** a loadout and a swap policy. The policy is
the hard half — see §"Open decisions" below, it is the same problem.

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

---

## Open decisions, not missing machinery

These are things the engine COULD do today and deliberately does not, because
doing them means inventing a play pattern. The repo's rule is that a policy is
the owner's call, not the model's (the 99-stack decision, 2026-08-08: "不要特殊化
处理99层那个了，不现实").

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

**Never interrupt is the ruling** ("换弹用不打断"). It is the reading the weapon's
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
refuses to invent (owner, 2026-08-10: "我们这个测试就是一按到底，没有理由给这个
特殊对待的。因为极限的话岂不是一发一发发射了？" — taken to the limit, a weapon
exempted from its own spool is a weapon fired one round at a time).

Each weapon's own card says so, which is what makes the decision reachable: the
`unmodeled:` line carries the caveat and the panel's passive strip states where
the rate starts and ends, beside the single number the stat panel prints.

### The MANUAL reload — and what it costs Ready Retaliation

This sim reloads when it cannot fire, and never a round earlier. A player
reloads whenever it suits them, and on one perk that is the whole difference:

**Ready Retaliation** ("On Reload from Empty: +100% Reload Speed for 6 seconds")
is implemented and correct — the window opens when the empty reload FINISHES, so
the reload that armed it never gets it, and the next one does if it comes inside
six seconds. Measured on the Phenmor's base form: **+19.1%** on a fire-rate build
(58 reloads become 67), and **exactly zero** on a stock one, where a magazine
takes longer to empty than the window lasts. That is the perk behaving correctly,
not a gap.

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

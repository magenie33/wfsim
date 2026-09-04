# M23 — Semi-Rifle Cannonade stated its rules in prose and modelled none (2026-08-02)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

The same shape as M22, found the same way — by looking at the one file whose
twin was already right. Its card:

> Only compatible with Semi-Auto Trigger. Fire Rate cannot be modified.
> +240% Damage / +1.5 Punch Through

`semi_pistol_cannonade` has carried `requires: semi_auto` and
`disables: [fire_rate]` since it was written. `semi_rifle_cannonade` had
NEITHER, plus a bare `- kind: fire_rate_bonus` with no value — a reading of
"Fire Rate cannot be modified" as an effect rather than as the lock it is. It
parsed to a zero-valued bonus, so it moved no number, and the lock went
unmodelled: Shred's +30% fire rate applied underneath it, and the mod paid its
+240% on a weapon it cannot go on.

Verified after: Shred is listed as a fire-rate source and the final fire rate
stays at the weapon's base, while the damage bonus pays (100 -> 505 with
Serration).

Values were already right (+240%, +1.5) — the mod-wide value sweep had
compared them against DE's card and found no disagreement. What the sweep
cannot see is a rule stated only in the description, which is why the
condition test from that pass exists.

### The SHOTGUN one was still wrong, a day later (2026-08-03)

"By looking at the one file whose twin was already right" found two of three.
`semi_shotgun_cannonade` had neither `requires` nor `disables` and still
carried the bare zero-valued `fire_rate_bonus` — so the card rendered
"+0% Fire Rate" under a sentence that forbids modifying it, on a mod that Boar
Prime (full-auto) could equip and the optimizer could return as a winner
(user: "半自动野猪是装不了的").

The lesson is about the METHOD, not the mod: comparing a file against its twin
finds a difference between two files and stops there. The family invariant is
now a test — every Cannonade states its equip rule, its calc gate and its lock,
and carries no fire-rate EFFECT under a fire-rate LOCK — which is a question
about all three at once and cannot be answered by reading any one of them.

Two more rules landed with it. `requires_weapon: semi_auto` is an EQUIP rule
and removes the mod from the pool entirely, which is the layer that matters
for the optimizer: `requires` only makes an equipped mod inert, and a build
that cannot be assembled in the arsenal should never be offered at all. And
the lock is symmetric — verified in both directions, a fire-rate bonus and a
fire-rate drawback (Critical Delay's -20%) both vanish under it, so the mod is
worth MORE on a build carrying a negative, not less.

### The equip rule is asked of EVERY firing mode (2026-08-04)

The wiki states the rest of it on the mod's own page: "Weapons with an Incarnon
mode must have Semi-Auto trigger type for **both firing modes** in order to
equip this mod, such as Bronco / Lato / Lex Incarnon Genesis." Dual Toxocyst,
Laetum and the Torid are all semi-auto and all transform into something that is
not (full-auto, full-auto, a held beam), so all three lose the Cannonade the
moment the Genesis goes in — and keep it while it does not (user, 2026-08-04:
"只要没点第一个 evo 就视为还是纯半自动，那就可以带，如果装上了就不可以带").

So the pool is a question about the BUILD, not about the weapon:
`mods_data::pool_for_build(weapon, evolutions)` is the rule and
`pool_for_weapon` is that function with nothing installed. A firing MODE is the
weapon's own trigger plus that of any form an evolution UNLOCKS — a CHARGED
form is not one, because charged vs uncharged is chosen on every trigger pull
and the weapon comparison lists a single trigger for such a weapon (Cernos
Prime is "Charge", Larkspur Prime "Held"). That is the line
`FormKind::is_gauge_switched` already draws.

It also settles the `continuous` case the same way, without changing it: the
Torid's Incarnon form IS a beam and the weapon still cannot take Sinister Reach,
because its other firing mode is a grenade launcher.

Both modules obey it from the same call. The simulator resolves its build
against `pool_for_build` with the fight's evolutions — which includes the one
the requested FORM implies, so asking for the Incarnon cycle is asking for the
weapon that has it — and the optimizer, where evolutions are a search
DIMENSION, vetoes the (subset, variant) PAIR rather than narrowing the scope:
the same eight mods are a legal build under a set that leaves tier 1 out.

### Open question, deliberately not changed

`traits_for` gives BOTH forms of a transform group the base entry's trigger, so
the Torid's Incarnon form (continuous) still counts as `semi_auto` for the CALC
gate (`requires`) and the mod keeps paying there. That is a documented choice —
"traits describe the WEAPON" — and it is separate from the equip rule above,
which has been settled: a build that reaches the calc has already been ruled
equippable. Whether DE keeps the bonus live once the weapon transforms is
UNVERIFIED, and needs an in-game measurement rather than a guess in either
direction. (In practice the two now rarely meet: a build that can wear a
Cannonade has no Incarnon form to transform into.)

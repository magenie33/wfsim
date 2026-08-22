# Kitguns — parts, and the one rule that assembles them

A Kitgun has no published stat line. It has a **Chamber**, a **Grip** and a
**Loader**, and a composition rule that turns the three into one. This directory
is the parts; `engine::kitguns_data` is the rule.

## THE WEAPON IS THE CHAMBER

Catchmoon is one weapon, and a primary Catchmoon and a secondary Catchmoon are
two forms of it. That is DE's own model, and three independent pieces of their
data say so:

- **Mastery.** Wiki, verbatim: *"the primary and secondary variants of each
  chamber **share the same Mastery progression**, so leveling both the primary
  and secondary variant of the chamber will not give more Mastery."* One
  chamber, one entry.
- **Riven compatibility.** DE's own weekly trade dump lists `Catchmoon`, `Gaze`,
  `Rattleguts`, `Tombfinger`, `Sporelacer`, `Vermisplicer` — six names, one per
  chamber, with no slot suffix. One riven fits both forms.
- **The wiki's own page.** One page per chamber, with Primary and Secondary as
  sections inside it — which is also what our URL rule asks for, since a URL
  mirrors the English wiki page name.

So the roster holds `catchmoon` with a form sibling, exactly as it holds an
Incarnon form or a charged alt-fire, and `/weapons/Catchmoon` is one page.

**"ONE WEAPON" IS ABOUT IDENTITY, NOT ABOUT DATA.** The two forms are genuinely
two stat blocks — `KitgunPrimary` and `KitgunSecondary` are two blocks in the
module and the difference reaches the damage TYPE (a secondary Gaze deals
Puncture + Radiation, a primary Gaze deals Radiation alone) — which is why a
chamber is stored per slot here rather than once with a flag.

## TWO PARTS DECIDE THE MOD POOL, AND NEITHER IS OBVIOUS

- The **GRIP** decides primary or secondary: *"determines whether the weapon is a
  primary or a secondary type weapon"*. Five grips per slot, and the name of the
  slot is the only thing separating the two lists.
- The **CHAMBER** decides the pool WITHIN the primary slot. Catchmoon's page:
  *"Uses **Shotgun** mods."* Gaze's: *"Uses **Rifle** mods."*
- The **LOADER** never decides anything about the pool.

So the pool is a function of (chamber, grip), and a build survives a part change
exactly when that pair's answer does not move. That is the rule the builder's
mod lock and its per-pool build cache are both keyed on — not on "which part
moved", which would have to enumerate two different ways of crossing the line.

## THE COMPOSITION RULE

Exact, with nothing to approximate:

```
damage        = chamber.damage[grip]              # published per grip
fire_rate     = chamber.fire_rate[grip]           # published per grip
charge_seconds= chamber.charge_seconds[grip]      # a charged chamber only
magazine      = chamber.magazine[loader.magazine] # size class -> a number
crit_chance   = chamber.crit_chance     + loader.crit_chance
crit_multiplier = chamber.crit_multiplier + loader.crit_multiplier
status_chance = chamber.status_chance   + loader.status_chance
reload_seconds= loader.reload_seconds
recoil        = grip.recoil
everything else is the chamber's
```

The three deltas are ADDITIVE and can be negative: Flutterfire is −8% crit
chance and +14% status. Nothing is a percentage of anything.

## WHAT IS HERE, AND WHAT IS NOT

`chambers/` holds Catchmoon and Tombfinger, each in both slots — four files. The
other four chambers (Gaze, Rattleguts, Sporelacer, Vermisplicer) are the same
shape and are not transcribed yet.

`grips_primary.yaml` and `grips_secondary.yaml` hold all ten grips. A grip's only
stat of its own is RECOIL: its effect on damage and fire rate is already resolved
into the chamber's per-grip tables, which is why those tables exist.

`loaders.yaml` holds all twenty, ONCE. The two slots publish identical loader
tables — same names, same numbers — and the generator asserts it rather than
assuming it, so a future divergence fails the build instead of being averaged
away.

## THE MAGAZINE SIZE CLASSES

A loader names a class and the chamber prices it. Eight classes are in use:
`lowest`, `low`, `med`, `high`, `highest`, `super_highest`, `mega_highest`,
`giga_highest`. They are not a scale anyone should read into — they are DE's own
keys, and a chamber may price them however it likes.

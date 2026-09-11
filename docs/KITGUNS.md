# Kitguns — a chamber in a slot is a weapon, and its parts are data

A Kitgun has no published stat line. It has a **chamber**, a **grip** and a
**loader**, and a composition rule that turns the three into one — exact, and
published by the wiki for all 1,200 assemblies through `Module:Modular`.
`data/kitguns/README.md` is the rule and the parts; this is the design.

## THE PARTS ARE DATA AND THE COMPOSITION IS PUBLISHED

`Module:Modular/data` is a structured Lua table, so a Kitgun costs part records
and one rule rather than 1,200 transcriptions:

| part | count | what it carries |
| --- | --- | --- |
| Chamber | 6 × 2 slots = 12 | crit, status, accuracy, ammo, spread, trigger, shot type, falloff, forced procs, multishot — plus a **damage table per GRIP**, a **fire-rate table per GRIP**, a **range table per GRIP** on a beam, a **charge table per GRIP** on Tombfinger's primary, and a **magazine table per SIZE CLASS** |
| Grip | 10 (5 primary, 5 secondary) | which SLOT the weapon is, and `Recoil` |
| Loader | 20 | three additive deltas, a magazine size class, a reload |

## THE SLOT IS THE WEAPON, NOT THE ASSEMBLY

DE's module has `KitgunPrimary` and `KitgunSecondary` as two blocks, and the
difference reaches the damage TYPE (a secondary Gaze deals Puncture + Radiation,
a primary Gaze Radiation alone). So the roster holds one entry per (chamber,
slot) — `gaze_primary`, `gaze_secondary` — each stating its own `mod_pools`.
The grip can then never move a mod pool: switching slots is switching WEAPONS,
and the page's Slot control does it without changing the address.

**ONE WEAPON IN IDENTITY, TWO IN DATA.** Both entries share the chamber's wiki
page and URL (the lower id keeps `/weapons/<Chamber>`, the other lives at its
id), its mastery track and its riven family. The riven's DISPOSITION is per
slot, and the chamber infobox module states both.

## THE ASSEMBLY IS A BUILD AXIS

A build carries `{grip, loader}` (the chamber is the entry). The builder binds
one pair, the optimizer and the ranked picker bind the set, a board row carries
its parts so two assemblies are two rows, and a share link carries them inline.
A request naming no parts is fought with `kitguns_data::default_assembly` — the
grip nearest the chamber's own `base` preview and the first loader that changes
nothing — never with the preview itself, which no player can build.

## WHAT THE MODULE LEAVES OUT

It marks a chamber `AOE` and publishes no radius and no radial damage; it names
no chain; it misses guaranteed procs a page states. All of that is read off the
chamber's own page, as for any weapon:

| chamber | primary | secondary |
| --- | --- | --- |
| Catchmoon | wide projectile, infinite body punch through, 42 m, no head | the same, 20 m, semi-auto |
| Gaze | beam with a 3 m sphere (the Ignis shape) | beam chaining twice at 0.2^n, 1 m punch through |
| Rattleguts | plain hit-scan auto | plain hit-scan auto |
| Sporelacer | Impact sac, 2.1 m Toxin explosion, bounces | Impact sac, 4.7 m Toxin explosion, three bomblets |
| Tombfinger | charged; quick and charged explosions per grip | carved explosion, 80.5% of the Radiation |
| Vermisplicer | tendril splitting to three at 70% | five tendrils (multishot 5), 1 m punch through |

An explosion is `blast:` on the chamber — ADDED (its own per-grip table) or
CARVED (a share of the shot's own damage type). A beam's sphere and chain are
the entry's `beam:` block, because they are geometry rather than damage.

## THE EXCLUSIVE ARCANES

A Kitgun seats one **Pax** or **Residual** arcane beside its ordinary one, in a
seat of its own:

| arcane | here |
| --- | --- |
| **Pax Charge** | modelled — the magazine becomes a battery at the chamber's own rate (`recharge_per_second`), the delay is the loader's reload |
| **Pax Seeker** | admitted — its bolts are a second attack triggered by a KILL; the formula is transcribed on the arcane |
| **Pax Bolt**, **Pax Soar** | admitted — the Warframe layer and handling |
| **Residual Boils / Malodor / Shock / Viremia** | admitted — lingering ground effects on a headshot kill |

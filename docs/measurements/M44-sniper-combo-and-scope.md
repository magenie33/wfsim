# M44 — the sniper combo and the scope, IMPLEMENTED AND UNMEASURED (2026-08-14)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

Both sniper mechanics now reach the number
(MECHANICS §7 §"THE SNIPER RIFLE"), and both are implemented from the wiki
alone. **No in-game capture backs either of them**, which is why this entry
exists: the repo's rule is that a faithful-looking implementation without a
measurement is not correct, and nothing here should be read as calibrated
until it is.

### What a capture would settle

Four questions the wiki does not answer, in the order they matter:

1. **Does the counter reach the DoT?** The multiplier is applied where every
   other final multiplier is, so a Slash proc off a combo'd hit inherits it —
   the same treatment Roar gets, and consistent for that reason rather than
   measured.
2. **Does an Incarnon form keep it?** The Vectis forms declare no combo and
   say so on their cards. A single scoped shot in Incarnon form with the
   counter visible under the reticle answers it.
3. **Do two Multishot pellets in one target really count as two?** The wiki
   says so outright; it is the one clause with a cheap in-game check (fire a
   Split Chamber'd Vectis Prime and see whether the counter goes up by 1 or 2).
4. **Does the second pellet of a shot pay the first pellet's increment?**
   Modelled as yes (each pellet reads the counter as of itself, like every
   other on-hit roll in the loop). Unknowable from the page.

### What it is worth today

Vectis Prime, base form pinned, Thrax Centurion lv 9999 Steel Path, 60 s,
100 runs, 100% headshots, eight mods (Serration / Split Chamber / Point Strike
/ Vital Sense / four elementals):

| combo | mean DPS | vs a counter earned from zero |
|-------|---------|-------------------------------|
| earned from 0 | 420,157 | 1.00x |
| held at 5 (x1.5) | 431,378 | 1.03x |
| held at 45 (x2.5) | 514,726 | 1.23x |
| held at 135 (x3.0) | 615,952 | 1.47x |
| held at 405 (x3.5) | 715,872 | 1.70x |

The interesting row is the first: **a fight long enough does not need the
card.** 76 shots at ~2.0 multishot is ~152 landing hits, so an earned counter
is already past the fourth tier by the end of a 60 s engagement, and over 180 s
(226 shots, ~452 hits) it passes 405 and the run's biggest hit lands at the
full x3.5. The card is for the short fight and for stating what a player walks
in holding — it is not how the multiplier is normally reached.

Played as its Incarnon CYCLE the same weapon gains far less (1.27x at a held
x3.5 against 1.70x in base form), because most of a cycle's damage is dealt in
a form that declares no combo. That gap is a claim about question 2 above and
nothing more.

### Sources

Wiki `Sniper Rifle` §Shot Combo Counter / §Zoom Buffs, `Vectis`,
`Vectis Prime`, `Vectis Incarnon Genesis` — cached under `vendor/wiki/`.

# M20 — Primary Frostbite could never earn a stack (2026-08-02)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

Found while checking a community claim ("Torid with Deadhead/Merciless crushes
Bulwark/Frostbite"). `ArcTrigger::ColdStatus` was declared and matched in the
data loader, but no `bump_trigger` call in `dummy.rs` ever fired it — Toxin,
Electricity and Heat were wired, Cold was missed. Arcane stacking buffs seed at
`max_stacks`, so Frostbite spent one 12 s window at 40 stacks and then sat at
zero for the rest of every run. It listed, it described itself correctly, and
after twelve seconds it did nothing.

Measured on Cernos Prime (a bow: innate damage is physical, so a Cold mod stays
Cold), Thrax Lv 300 SP, 120 s, 150 runs, seed 7, rank 5:

| | kill score |
|---|---|
| no arcane | 2.722 |
| Frostbite, before | 2.955 (1.085x) |
| Frostbite, after | 5.585 (2.051x) |

The 1.085x is exactly the seeded window and nothing else, which is the
signature of the bug.

NOT a golden-value change: no golden test covered this arcane, and no in-game
measurement is claimed here — the fix makes the sim do what the arcane's own
card says. `every_on_status_trigger_is_fired_somewhere` now fails if any
on-status trigger is left unwired.

Separately, and NOT a bug: on the Torid the same arcane stays at ~1.1x however
it is built, because the weapon's innate Toxin absorbs any Cold mod (into Viral
alone, or into Gas + Magnetic alongside other elements). Cold never survives as
Cold on that weapon, so Frostbite has no trigger there. Primary Bulwark is
`kind: unmodeled` — it scales with the WARFRAME's armour, which a weapon
calculator has no model of — so it reports exactly 1.00x and no comparison
against it means anything.

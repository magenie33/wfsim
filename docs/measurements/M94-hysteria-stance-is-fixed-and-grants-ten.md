# M94 — Valkyr Talons: Hysteria is fixed in the stance slot, and its slot doubles it ✅ (owner, 2026-09-14)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

On **Valkyr Talons** in the arsenal:

- the **Hysteria** stance is equipped and **cannot be removed**;
- the card grants **5** capacity, and the stance slot's polarity **matches** it,
  so the grant doubles to **10** and the weapon's **60** becomes **70**;
- the stance slot **cannot be polarized with Forma**.

### What it settles

Neither the wiki's `Module:Mods/data` nor DE's export carries a Hysteria stance
card, so no source stated its grant or its polarity. W`Exalted_Weapon` says every
melee Exalted weapon has "a Zenurik stance polarity and associated stance mod
that cannot be removed", and W`Valkyr_Talons`' patch history removed Forma from
the Exalted stance slot. The grant is the ordinary stance rule
(`rules::capacity::stance_capacity`: 5, doubled on a matching slot), so the card is Zenurik.

### Where it is read

`data/mods/valkyr_talons/hysteria.yaml` (Zenurik), `fixed_stance:` on
`data/weapons/melee/valkyr_talons.yaml`, `board::builds::validate_with` (a build without
it is refused), and the builder's stance slot, which seats it and offers neither
removal nor a polarity. `a_fixed_stance_is_required_and_grants_its_capacity`
holds the 10.

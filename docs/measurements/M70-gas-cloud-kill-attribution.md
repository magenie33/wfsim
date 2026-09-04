# M70 — a Gas cloud's kill is the WEAPON's kill ✅ (owner, 2026-09-01)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**A body that dies to Gas damage is credited to the weapon that applied the
Gas, not to the Gas effect.** So every on-kill grant fires off a cloud tick:
the Galvanized family's stacks, the Merciless-family arcanes, an `On Kill` card
on an Incarnon evolution, and an Incarnon gauge fed by kills rather than by
hits.

**THE WIKI DOES NOT COVER THIS.** `Damage/Gas_Damage` says nothing about who a
kill belongs to, so this entry is the source and the engine's agreement rests
on it rather than on a page.

**Gas is the whole of the claim.** The cloud is the reason it is worth stating
at all — it has a radius, it registers body parts by itself, and it keeps
ticking on the bodies around a host that has already died, which makes "an
entity that kills on its own account" the natural reading of it. It is not the
reading the game takes.

**The engine already credited it**, so no published number moves: on-kill
stacks are read off `RunResult::kills` and the two hand-wired families
(`GalStacks::bump_on_kill`, `ArcRuntime::on_kill`) are bumped in
`process_ticks`. What this adds is the source under that behaviour and a test
that refuses the carve-out — `a_gas_cloud_kill_earns_the_on_kill_stacks`, whose
one shot deals 4 into 10 health and then reloads for longer than the fight
lasts, so every kill in it is a cloud tick's.

### What this does NOT settle

- **The Electricity chain and a weapon's lingering cloud** (the Torid's). They
  are the same shape — damage settled by something standing apart from the
  shot — and the engine credits them on the general status-kill rule (the Lato
  Incarnon's *"Kills from status effects can also trigger the effect"*). This
  entry is about Gas and says nothing about either.
- **A kill on a body the shot never touched.** The DoT path credits every body
  it ticks on, but `spread_hit`, the Blast area hit and the syndicate radial
  reach a neighbour without bumping the two hand-wired families at all — so a
  neighbour killed by a chain hop earns no Galvanized stack while the same
  neighbour killed by the cloud does. That is a bug in the engine, not a
  question about the game.

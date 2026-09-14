# M93 — an Antique mod's "unique School" bonus counts the OTHER schools seated ✅ (owner, 2026-09-14)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

On a Tektolyst Artifact, a card whose second line pays "for each Mod from a
unique School" (Ubri-Kaneph, Metem-Hakh, Metem-Erun, Empazu-Shol, Esti Vel-Ikha,
Lashta-Vak) gains that bonus once for **each school seated that is not the
card's own**. A second card of its own school adds nothing.

### What it settles

W`Ubri-Kaneph` and its siblings say "a bonus for each mod from a unique Focus
school equipped on the Artifact" and never say whether the card's own school is
one of them.

Two cards of one other school count as ONE school: the card says "unique".

### Where it is read

`engine::warframes_data::ArtifactMod::bonus_count`, served by
`/api/operator/panel`. `the_artifact_seats_five_known_mods_once_and_one_arcane`
holds it with two Madurai, two Vazarin and one Naramon card.

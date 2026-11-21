# Secondary Irradiate

`rules::perks::secondary_irradiate` implements the recovered script counter.
`DamageData::GetNumHits()` reads the independent unsigned count at +0x244:
construction sets one, assignment preserves it, a fresh derived packet resets
it, and compatible queued notifications add it with 32-bit wrapping. The
count-only merge API requires a supplied compatibility verdict. It does not
implement the native packet comparator or merge damage and status payloads.

The script state keeps the original owner keys:
`RadialDamageOnMaxRadStackHit_WeaponType`,
`RadialDamageOnMaxRadStackHit_FrameHitCounter`, and
`RadialDamageOnMaxRadStackHit_IsFullyCharged`. Victim ids index separate
counters. Weapon type and charge state are shared by the source owner. The
keyed OnDamaged listener belongs to the victim and services multiple owners.

Activation needs ten Radiation stacks, a valid weapon, source proc type 30,
and an ordinary hit that is not the arcane's intermediate object. It retains
any accumulated count, updates the owner weapon type, resets shared charge to
nil, registers the listener, and returns a suspended job with the original
query context. The installed collector checks valid source, intermediate
exclusion and supplied `IsA(stored_type)` independently. It does not repeat the
activation hit-class exclusions. Below ten Radiation it clears that victim's
count before changing charge state; otherwise it sets charge and adds NumHits.

The caller supplies the resumption boundary and original source/victim validity.
An invalid context returns before query and consumption. A valid context queries
the original projectile/damage data using current shared charge, reads the count,
and clears it before testing the multiplied coefficient. Query occurs even for
zero count or a nonpositive coefficient. Only the first resumed job can spend a
given accumulated count. The query result is not the latest notification's seed.

## Scene binding

`fight::irradiate` uses this state machine for the scene's emitted direct-hit
submissions: aimed hits, chains, tendrils, ricochets, contact and punch through.
Each submitted hit supplies a fresh count of one and resumes immediately. This
is an explicit scene adapter, not a recovered native Sleep(0) schedule. A ready
echo retains its original factors until the scene can borrow all recipients.
Echoes originate from the actual struck body, exclude that body, and can reach
the aimed body when another body triggered them. An echo cannot trigger itself.
The scene invalidates its listener projection on body-generation changes.

In a posed scene the echo originates at the victim's root and checks the
declared radius against each physical actor's overlap shape. Each accepted
recipient uses the existing full-strength echo settlement. The originating
victim is excluded before the overlap.

The scene's resolved loadout supplies a logical type relation and uncharged query
context. Native GetType/IsA ownership, charged-getter meaning, callback lifetime,
packet producer selection and the complete collision/update/Sleep(0) window are
unbound. Radial, script and DoT notifications are accepted by the generic
collector but are not fabricated by this scene adapter. A new producer must
supply actual notifications, compatibility and resumption boundaries rather
than infer multiplicity from weapon name, AoE presence, pellets or damage pops.

Laetum Incarnon's M59 multiplier remains an explicit recipient calibration in
this adapter, separate from the queried seed and counted coefficient. It does
not set NumHits to two. The base/incarnon direct seeds remain
separate from the explosive seed; no direct-plus-explosion seed is introduced.
Native projectile selection uses mAttackData, but the full Laetum producer and
scheduler are unbound. Existing recipient damage, crit, proc and faction
projection remains calibrated scene behavior; this change does not certify it
as a full native echo recipient implementation.

Arcane payload merging preserves the complete echo share/radius/Radiation gate
when two slots are equipped. The catalog has one echo source, so no combination
rule for multiple independent echo sources is inferred.

## Evidence

The reviewed source is batch
`20261115T000143Z-secondary-irradiate-general-rule`, manifest SHA-256
`d0266d52e23aaece03c1654190c2a7f75c252b67754fe22c3116c9a1a970176f`,
client 2026.10.28.09.54, executable SHA-256
`dae100202115919c89440c9ef320f691113ee8dcaf01205cbd180072c35aabdb`.
`scripts/import_irradiate_count_fixture.py <batch-directory>` verifies every
outer file and archive member before importing the 14 native count vectors.
The original replay executed 816 preserved addresses. Lua API, notification
capture and allocation adapters do not supply grouping, counts or damage.
The fixture states the adapters and unreplayed boundaries explicitly. Rust
tests compare the count projection using supplied compatibility, not a claimed
re-execution of the full native comparator or queue drain. State-machine tests
cover multiple jobs/victims/owners, shared charge, filters, invalid context,
query ordering and consumption. This source contains no new in-game damage
measurement; golden numbers remain governed by `docs/MEASUREMENTS.md`.

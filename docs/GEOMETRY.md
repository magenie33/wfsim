# Explicit 3D combat

An optional `scene_3d` request drives the shared simulator and optimizer with
supplied player and enemy models. Without it, the planar policy in MECHANICS
§11 applies. This is an experimental scene adapter, not a calibrated replacement
for the live game's pose, emission, world-query and explosion providers.

## Aim and actors

`headshot_pct` is reticle intent in this scene. At 100 the player keeps aiming
at the selected head proxy and completely compensates recoil. Intrinsic spread,
camera/muzzle parallax, homing and intervening bodies still determine contact.
Only an actual head contact receives head damage, head triggers and weakpoint
charge. `gauge_charging_headshots` selects head intent while charging; it does
not bypass collision. Saved field names retain their wire spelling.

The player supplies avatar world transform and controller dimensions, current
avatar-local bone transforms, camera and muzzle mounts, and the independent
reference-point input used by the native eye-basis projection. Each mount is
`world * posed bone * local attachment`. Physical-controller height does not
stand in for head or muzzle height. Enemy models supply their world transform,
posed bone/attachment proxies, acquisition anchor and steering center. Enemy
order matches the arena: aimed enemy first, then `formation`.

The scene supplies its camera-ray result to the native player eye projection
(`0x149c340`). The projection uses binary32, extends the target by camera
forward when the reference projects beyond the target, and uses avatar forward
for the coincident fallback. The firing origin is independent of the eye basis.

The optional `player.equipped_fire` describes installed weapon attachments
and their round order. Each attachment supplies its player-bone `mount`,
weapon-local posed `bones`, starting `initial_cursor` and ordered firing
`profiles`. Each profile contains a list of firing positions with a weapon
bone index and local transform. The fire origin combines the mounted weapon
transform, the current weapon bone and the local firing transform. A missing
bone uses identity. A missing mode selects profile zero. An empty profile
array uses the mounted weapon transform.

The supplied `round_order` repeats across ammo rounds. Each entry selects an
`attachment` and `mode`. The read at `0x56cde0` leaves its cursor unchanged.
An advance at `0xde85d0` wraps the cursor of the selected attachment when
the profile has at least two positions. On a supplied mode change the scene
indexes the retained cursor modulo the number of positions in that profile.
`refresh_per_iteration` advances before each additional physical round in the
same emission. `advance_after_round` advances after the last physical round.
Additional hit instances that ride the same physical round share its origin.
Each simulation starts from the supplied cursor values. Omit equipment to
use the single muzzle mount. Live hand order, pose installation, cursor reset
writers and spread-state producers remain calibration inputs.

The request is rejected if a model is missing, a transform is nonfinite,
singular or nonuniformly scaled, controller/world scales disagree, or a selected
mount/proxy is absent. The resolved faction bracket currently requires one combat faction per scene. The prepared scene is immutable. The shared `parse_fight`
projects its actor roots into the arena for existing target/status consumers;
a search's replay retains the complete original scene.

`placement.kind` is `supplied`, `contact` or `center_distance` with `distance`.
Automatic placement translates the entire formation along its horizontal aim
axis. Contact supports upright controllers on one floor with overlapping inner
segments: the float32 installed radii sum plus the **moving player's** skin.
Height enters the capsule-segment overlap check. Other contact arrangements
need supplied positions; this is not a general character-controller solver.

## Collision and settlement

Each physical round samples the weapon's resolved intrinsic spread once in 3D.
Multishot instances share its physical path; each receiving body independently
rolls the round's multishot share. The path is queried against every enemy's
posed sphere/capsule proxies, ordered by nearest contact. Range, part, flight
length and arrival time belong to that contact. Every receiving body uses the
ordinary pellet damage, critical, status, buff and ledger code with its own
state. A penetrated body independently resolves its part.

The analytic query uses binary64 geometry. Y is up; transforms are row-major
3x4 matrices applied to column vectors. Capsule height describes its inner
local-Y segment and excludes its caps. The supplied world scale is applied
once. AABB pruning encloses the posed primitives. Ray enable and part exclusion
are respected by the component query; contact is strictly before the ray limit.

The ordinary-gun adapter supplies the body's nearest entry and farthest proxy
exit to the trace-gun fragment. Laetum base fire selects `projectile_reverse_query`
and uses the kinematic route at `0xa2aa00`. In the supplied straight flight,
the entry point is the reverse anchor. The ray starts ahead of it by
`f32(remaining * 1.1)`. The scene intersects that segment with the matching
posed collider, and the native-order consumer selects the last matching
handle and subtracts the binary32 distance from the remaining budget.
Equality permits continuation. A zero-length crossing advances the shot by
0.01 units without spending budget. A stopped shot retains its entry as the
end and its remaining budget. Forward sweep width, the active native entry
handle, reverse world filtering and terrain remain calibration boundaries.
The pierced enemy settles its own contact through the ordinary damage path.

Guided impact uses the published projectile translation for the blast origin.
The arming accumulator in the supplied first-impact route includes the
attempted substep's flight distance. It must exceed 0.25 for radial dispatch.
The direct impact still resolves at equality. On an armed impact, candidate
membership uses sphere overlap against the supplied installed
`radial_proxy` or the detailed posed shapes when that field is omitted.
`radial_proxy.local` is relative to the avatar and can supply the installed
collision scale independently of the visible mesh. Recipients are processed
by nearest point of their candidate AABB, keeping the scene order for equal
distances. Each owner is processed once, and the nearest detailed attachment
gives its radial contact part and falloff distance. Other radial projectiles
use the impact point and distance to the closest detailed surface.

The damage path retains the measured weak-point conditions, per-instance
settlement and independent weak-point triggers across pierced bodies.
[M60](measurements/M60-headshot-bonuses-add.md),
[M62](measurements/M62-volley-settles-per-pellet.md),
[M103](measurements/M103-latron-bounces-punch-through-and-the-gauge.md),
[M123](measurements/M123-acuity-is-three-point-four-nine-eight.md).
The native replay captures radial parts at the controller input. Radial damage
still follows the existing area damage rules. Live filter enrollment, cover
and damage acceptance are open. Status areas retain their planar policy.

The scene refuses delivery routes it does not implement: continuous beams,
ricochets, melee, blobs, orbs, lingering fields, lock-on, tendrils, attractors,
reload grenades, corpse/spawn/dagger routes, extra ability actors, ability grants,
frame casting and echo/influence arcanes. Guided-projectile penetration and speed
modifiers are also refused until their native consumers are bound. It does not
silently run those through planar direct collision. No production default or board ruler selects this scene.

## Homing

The Akarius entries carry the source Glaive inputs: initial speed 40, maximum
80, acceleration 40, turn duration 0.079999998, seek range 30, cone 10 degrees,
strict flight-distance-greater-than-zero acquisition gate, and blade radius
0.02. These fields are consumed by the explicit scene. The default planar
scene retains its declared projectile-flight convention.

The numerical step retains the quaternion, three angular velocities and bank
state. It uses the native look-at, Euler/quaternion conversions, angle wrapping,
polynomial sine/cosine and CRT asin/atan2 consumers, including their original
binary32/binary64 boundaries and atan2 table. Turning duration is multiplied by
five without a retained target and floored at binary32(1e-6). The original
0.235/0.48 encodings are `0x3e70a3d7`/`0x3ef5c28f`. Bank steering uses the
supplied world quaternion separately from the retained rotation. Flight distance
accumulates bounded speed times the time step, independently of rounded position.

The default numerical route agrees bit-for-bit with 429 retained native steps
across 60 continuous supplied-state sequences and 120 additional 3D controls.
The comparison includes position, velocity, quaternion, all angular velocities,
bank and flight distance. Another 765 angular controls and 4,931 distinct math
controls qualify the numerical stages. The state domain is the default clock,
state 0, no gravity, an empty native +1a10 reference, and empty-world movement. Other
state/clock branches remain unbound. A supplied update owns its world rotation
until it completes. Numerical steering retains its own substep quaternion.
The scene's supplied-direction initializer and target installation are not
native emission producers.

These original-byte controls execute under the pinned Unicorn environment.
The isolated i7-14700KF hardware controls produce different RSQRTPS and
look-at outputs. A supplied sphere ray at the exclusive float32 limit
`0x40600001` hits with the Unicorn direction and misses with the hardware
direction. Real game hardware/profile binding remains open.

The ordinary update refreshes acquisition once before its fixed-substep loop.
It retains an eligible installed target. The empty-history branch tests
candidate-root range and ranks eligible candidates by candidate-anchor distance
divided by cone dot. A smaller visible score can replace the current winner.
Equal scores keep the earlier enumerated actor. The scene supplies the ordered
list, eligibility and separate steering center. Acquisition visibility uses
posed geometry by default. `clear_homing_visibility` supplies the clear query
result used in the native open-grid replay. Flight still collides with bodies.

`frame_seconds` is the fixed substep duration. Each `guided_updates` entry
specifies `substeps` and whether to `publish_transform` after that update.
Subsequent updates have one substep and publish their completed translation.
An impact during an update uses the previous published translation, initially
the supplied muzzle position. A projectile without an impact ends without a
radial event. Live update cadence, launch state and alternate ownership
predicates remain inputs for calibration.

The nonsweep movement branch (`0x1246b90`) queries the center, positive side
and negative side segments in order. They share the retained start; the
center endpoint uses 1.2 times the physical displacement, and side endpoints
add or subtract blade radius along the supplied world quaternion's right
axis. Empty movement advances by the physical displacement. The record
consumer considers backend order, preacceptance and the first eligible record's
normal. A back-facing result advances to another segment. For modeled combat
the posed-body adapter supplies nearest-actor ordering, an opposing normal and
settlement at the supplied step boundary. Its binary64 proxy arithmetic can
choose a different member of an original nearest set. Real PhysX records,
world surfaces, impact continuation, contact timing and dynamic poses remain
unbound.

## Evidence and verification

Client build 2026.10.28.09.54, executable SHA-256
`dae100202115919c89440c9ef320f691113ee8dcaf01205cbd180072c35aabdb`.

- `20261116T084610Z-enemy-3d-gun-foundation`, manifest SHA-256
  `86a258275bdce5cf21c5bee885cc6768a57066c20efcb7bb5ff786a1f8c26374`:
  24 proxies, eight supplied world/pose caches, 618 ray cases and 121
  instruction-exact float32 penetration fragments. Position tolerance is
  0.0003 and the supplied tied-nearest set, not inflated hitboxes.
- `20261116T094904Z-shooter-3d-origins`, manifest SHA-256
  `c938a7e8ef08d58fba86bd3c1c8fea18062015000bee7cc8fed9c6e521b0dc13`:
  83 Excalibur source-default bone transforms and controller dimensions.
  Source-default pose is not an equipped firing animation.
- `20261117T083330Z-native-contact-movement`: capsule construction and
  conditional upright contact. Its broader native movement cases are not
  replaced by the supported axial contact condition.
- `20261117T114909Z-akarius-homing-close-range`: steering inputs and the 36
  exported empty-world motion sequences, retained in `native-motion.json`.
- `20261117T132810Z-reticle-3d-homing-materials`: full reticle-to-impact
  verification and execution of its complete acceptance matrix are false.
- `20261118T083810Z-native-3d-homing-replay-retention`, manifest SHA-256
  `5f57e9c7c377f16b7571b2900a6f7ab8f9c76d9fa584e316b0c16fb7eb72aea3`:
  full matching executable/modules, original RIP data and call-state pages.
  `native-flight.json` retains numerical states from the original 705-call
  capture. `native-steering.json` adds observed native math boundaries and
  explicitly supplied 3D controls. `rotation_tables.rs` is imported from PE
  RVA `0x241ee70`; its hash is pinned in the fixture. No live acceptance trace
  or independent game measurement is supplied by this batch.
  Its 108 original attachment controls verify Akarius and Akarius Prime's
  two five-position profiles, Laetum's base muzzle bone and the Incarnon
  attachment's local offset fallback. `scripts/import_native_attachments.py`
  reads the pinned source poses, attachment declarations and read/advance
  results into `native-attachments.json`.
- `20261118T145138Z-native-homing-ownership`: 15 supplied-state selection
  controls, including eligibility, competing scores and enumeration order.
  The importer executes one additional original-byte control where the
  winning target has the larger Euclidean distance.
- `20261119T043220Z-glaive-motion-queries`: 68 original descriptor and
  backend-record controls, including nine first impact-call boundaries.
- `20261119T055143Z-native-3d-answers-hardware`: 38 supplied-state flights,
  3,099 ordered segment descriptors, two full 25-actor scenes and hardware
  controls. The portable scene compares contact step, actor and point against
  the supplied-state original replay. It preserves the supplied nearest-proxy
  tolerance set and checks point error below 0.0003 source units.
- `20261118T145815Z-akarius-radial-dispatch`: corrected impacted-handle ABI
  and separately observed transform translation at the radial submission.
- `20261120T041422Z-ahom-runtime-first-native-slices`: projectile reverse
  ray, handle-order, budget and continuation controls.
- `20261120T063733Z-ahom-akarius-grid-clock` and
  `20261120T064738Z-ahom-akarius-grid-recipients`: supplied 25-actor tests
  at two collision scales. The receiver checks acquisition, direct actor
  and part, blast center, ordered radial owners and part indices across all
  six combinations.

`scripts/import_native_flight.py` imports the original captured state pages.
`scripts/import_native_steering.py` executes the matching original consumers
through the retained research scripts. The importer pins the executable and
original result index, reruns the 123 source controls, then adds explicit
boundary/3D inputs. Reproduction uses the batch's Python3.14.0 and locked
dependencies; `--atan-table` writes the production table from the original PE.
Expected states never come from the Rust implementation.
`scripts/import_native_queries.py` projects the source-pinned eye, selector
and movement results into `native-queries.json`.

```sh
cargo test -p wfsim-engine --lib rules::geometry
cargo test -p wfsim-engine --lib scene::tests
cargo test -p wfsim-webapi --lib scene_tests
cargo run --release -p wfsim-webapi --example scene_fights -- fights.json
cargo run --release -p wfsim-webapi --example scene_fights -- equipped-fights.json --equipped
cargo run --release -p wfsim-webapi --example geometry_builds -- calls.json builds.json
cargo run --release -p wfsim-webapi --example scene_controls -- calls.json controls.json
```

`scene_fights` runs 5x5 supplied models at 3-unit spacing for both source enemy
scales, contact and named center separation 5, with real intrinsic spread.
It retains every request, simulation and combat record. Two explicit muzzle
controls are used: the native empty-world trace's `(0,1.5,0.3)` and a muzzle
placed on the camera ray. The latter isolates homing from parallax; neither is
a claim about an equipped Akarius/Laetum animation or attachment writer.
The `--equipped` control composes the source-default Excalibur weapon-hand
bones with each weapon's source firing positions. It supplies right then left
for the Akarius Prime burst and advances only the hand used.

The `/api/log` response carries `scene_3d.flights`: shot/round, camera, muzzle,
aim anchor, the eye projection's basis origin and target, launch direction,
selected homing target and contacts with part, entry/exit, travel time,
thickness and remaining budget. When equipped, the trace includes the chosen
attachment, profile and cursor in `fire_origin`.
Guided flights include the submitted `blast_center` and `radial_impacts` when
present. Each radial point is the recipient's selected attachment.
This diagnostic is bounded separately by the requested record limit and reports
its dropped count. Damage stays in the existing combat stream and ledger.

The build harness exhausts each declared scope with two reference seeds, reports
settling/rank/list recall separately and asserts ranked replay score, DPS and
kills exactly. Existing `build-calls.json` without a scene is an aim-weight
sensitivity experiment; it does not measure geometric headshot probability.
Full gameplay calibration still needs live equipped poses/origins, world-query
bindings and independent in-game measurements. No golden value is changed by
these supplied-state experiments.

`scene_controls` projects every actor root from the prepared engine scene into
a planar control request. It preserves the complete formation, including bodies
that took zero damage. A simulation report lists damaged bodies and cannot
serve as the formation source. The explicit scene remains the authority.

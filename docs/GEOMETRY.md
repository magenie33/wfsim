# Explicit 3D geometry

`engine::rules::geometry` provides an explicit-pose geometric reference.
It is exercised by native component fixtures and `geometry_probe`, and is
not installed in the production fight. The fight's scene policy remains
MECHANICS §11. Native component agreement is not gameplay calibration.

## Body queries

Inputs are the owner identity, current proxy cache, world transforms, ray,
maximum/current-best parameters, excluded-part mask and outer refinement gate.
Each proxy keeps its cache index, bone token and native part id separately.
Ray enable and part exclusion precede intersection. The nearest candidate must
be strictly before both limits; equality does not hit. Missing animation or an
empty body list can return native acceptance without establishing contact.

Y is up in these client coordinates. Transforms are row-major 3x4 matrices
applied to column vectors. A capsule's inner segment lies on local Y and its
height excludes its caps. Spheres and capsules are distinct primitives.
The caller supplies world * current bone * local attachment. No mesh scale,
pose, physical-controller radius or rendering triangle is inferred.

The analytic reference uses binary64 geometry. Imported source float32 values
retain their represented values, but this is not an instruction-exact native
float32 port. The exported source-default queries permit 0.0003 coordinate
units of positional error and the recorded tied-nearest set. Those tolerances
are test comparisons, not expanded hitboxes or relaxed endpoint predicates.
Degenerate transforms/directions produce no contact.

## Penetration fragment

`geometry::penetration` consumes supplied reverse-hit records and their
reference identities. It selects the last matching reference, computes the
entry/exit length and accumulated budget in the recovered float32 order,
and accepts equality. A refused crossing stores the maximum budget; a missing
match preserves the prior accumulation. Infinite traversal keeps accumulating.
The fragment does not generate the reverse query or sum overlapping proxies
as separate bodies. It is a trace-gun fragment, not a projectile/beam rule.

## Evidence and verification

Client build 2026.10.28.09.54, executable SHA-256
`dae100202115919c89440c9ef320f691113ee8dcaf01205cbd180072c35aabdb`.
Batch `20261116T084610Z-enemy-3d-gun-foundation`, manifest SHA-256
`86a258275bdce5cf21c5bee885cc6768a57066c20efcb7bb5ff786a1f8c26374`.
The public fixture retains the consumed source hashes, 24 declared proxies,
eight supplied pose/world caches, 618 ray cases and 121 penetration cases.
Bone tokens are the fixture's supplied identity labels, not live interned ids.

```sh
cargo test -p wfsim-engine --lib rules::geometry
cargo run --release -p wfsim-engine --example geometry_probe -- output.json
cargo run --release -p wfsim-engine --example homing_probe -- homing.json
```

The probe supplies a head-directed launch, muzzle, pose and scale explicitly.
It calls the build resolver and spread sampler, then queries the posed body.
Both supplied yaw orientations are retained; a back proxy intercepting a
head-directed ray from behind is not discarded to manufacture a head hit.
Its head/body/miss counts are conditional geometry diagnostics. They do not
establish native screen-reticle aim, actual equipped muzzle positions, Akarius
homing or gameplay headshot probabilities. The old planar hit count is retained
under its own geometry, including that model's independent headshot input.

`homing_probe` queries segment rays along the 36 recorded native empty-world
motion sequences from batch `20261117T114909Z-akarius-homing-close-range`.
The source member hash and the supplied pose/scale/yaw remain in its output.
These are component overlays, not an implementation of native projectile
sweeps, emission, live collision or explosion dispatch.

## Build sensitivity

The public request fixture fixes six cards, searches two more from six cards,
and compares 0% versus 100% configured headshot weights. It covers both pistols'
base modes and Laetum's complete gauge cycle. Enemy, duration, ammunition,
center coordinates, seed and default evolution choices are explicit inputs.
This is a constrained sensitivity experiment; geometric diagnostics are not
silently converted into gameplay headshot probabilities.

```sh
cargo run --release -p wfsim-webapi --example geometry_builds -- tests/fixtures/geometry/build-calls.json build-comparison.json
```

The harness retains independent two-seed exhaustive reference results and each
ranked build's simulator replay. It requires exact replay score, DPS and kill
count agreement. Reference settling, answer rank and list recall remain
separate reported results; a correct winner does not certify the entire list.

## Connecting the fight

The shared arena must own the explicit scene before build, simulate and solve
consume it. Keep placement/controller contact, posed body proxies, screen aim,
selected-hand muzzle, emission/spread, flight, first contact and explosion as
separate stages. Preserve frozen request names through adapters.

1. Bind the current pose/scale and world-query inputs. Replay the existing
   native contact/movement cases without replacing them with one radius sum.
2. Bind camera/target/reference-point providers and current attachment tables.
   Port the 1,476 shooter cases, preserving eye basis versus returned origin.
3. Bind both weapons' emission, spread state and seed ownership. A reticle on
   the head does not prescribe a muzzle-to-head velocity.
4. Port native Akarius steering with retained angular state and verified update
   order. Its 123 bounded controls do not establish its first live collision.
5. Bind projectile sweep/filter/overlap, damage dispatch and explosion center,
   then drive the existing damage/status/Incarnon path from those events.
6. Compare complete fixed-state traces, calibrate against game measurements,
   then compare paired builds and measure scene/pose/pellet costs.

The reticle contract in batch
`20261117T132810Z-reticle-3d-homing-materials` requires both Akarius variants,
both source enemies, physical contact and 5 m named center separation, both
hands, burst order, fixed seeds and intrinsic spread. Its
`full_reticle_to_impact_verified` and `acceptance_cases_executed_in_this_batch`
are false. Launch-time target installation, live Akarius muzzle/pose, projectile
sweep and complete explosion dispatch remain unbound. No universal head/body
outcome follows from distance alone.

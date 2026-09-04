# M47 — a body is 0.2 m across the floor, measured by walking into one ✅ (owner, 2026-08-16)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

Walking into an enemy stops at **0.4 m centre to centre**. Two bodies of the
same size touching at 0.4 m makes each of them **0.2 m**, and that is the whole
derivation — the closest approach IS twice the radius, so the one quantity a
player can read off the game gives it directly.

`space::BODY_RADIUS_M` is 0.2 and `CONTACT_RANGE_M` is 2r = 0.4.

**IT REPLACES A GUESS OF 0.25 m** that shipped for one day. That number came
from taking the circle of the same AREA as a 0.6 x 1.8 m humanoid silhouette —
an attempt to put a body's HEIGHT back into a flat model, on the reasoning that
a real spread cone spends half its deviation vertically. It was wrong twice:
the plane is the model (owner, 2026-08-15), and `headshot_pct` already answers
where on a body a landed pellet went. The owner's original 0.2 m was right and
now has a measurement under it rather than a shrug.

**WHAT IT MOVES.** Nothing at contact, and everything past it:

- CONTACT IS INVARIANT under the radius. The hit test at contact is
  `r / 2r = 0.5` for any r, so both boards, every golden value and the two
  entries whose aimed cone is wide enough to miss at contact (the Mandonel's
  uncharged 60 degrees, the Cryotra's 40) are exactly where they were.
  `one_fight` reports every answer unchanged.
- BEYOND CONTACT a smaller body is a harder target. The same 2 degree cone that
  missed a 0.25 m body past about 7 m misses a 0.2 m one past about 5.7 m.

### AMENDED 2026-08-20 — the radius is 0.25 after all, and there is only one number

The owner: the Tenno's radius and an enemy's are both **0.25 m**, and
`BODY_RADIUS_M` and `BODY_MATERIAL_M` *"就应该是一个数字"* — should be one number.
They now are: the material is `2r`, the diameter, because a body is a circle.

**THE PENETRATION TABLE IS WHAT DECIDES IT, and it was in the repo the whole
time.** `a_body_costs_what_the_wiki_table_says` brackets a humanoid's material to
`(0.4, 0.5]` across thirteen published cells — 0.4 m fails on three independent
mods, 0.5 m works on Vigilante Offense. Under one constant the material IS the
diameter, so 0.2 m of radius gives 0.4 and is **excluded by that table**, while
0.25 gives exactly 0.5. The table was being read as evidence about a separate
constant when it is evidence about this one; that reading is what forced the
split in the first place.

**WHAT THIS MEASUREMENT ASSUMED.** The derivation above is "two bodies of the
same size touching at 0.4 m makes each of them 0.2 m" — which requires the walk-in
stop distance to be exactly the sum of two radii, with no push-out margin between
the capsules. Nothing measured that step. Two independent sources now say 0.25
against one derivation that needed an assumption.

**WHAT IT MOVES: still nothing at contact.** `one_fight` reports every answer
unchanged on all three shapes and every golden value holds, for the reason this
entry already gives — the hit test at contact is `r / 2r` for any radius. Past
contact the effect is this entry's own arithmetic read the other way: a 2 degree
cone reaches a body to about 7 m again rather than 5.7 m.

**STILL OPEN: whether the HIT TEST should read this radius.** What is measured
is how much FLOOR a body occupies — which is what decides where two of them can
stand. That the same number decides whether a pellet reaches one is the model's
choice, and DE publishes nothing to check it against: the wiki's `Area of
Effect` gives zone shapes and never says whether a radius is measured to a
body's centre or its surface, `Hit Mechanic` covers only the player's side, and
`Line of Sight` describes an enemy as three rays to head, torso and feet — a
vertical segment with no width at all.

The experiment that would settle it is unchanged: stand a known distance from
one stationary enemy, fire a counted number of pellets from a weapon of known
spread (the per-attack `MinSpread` from the wiki's weapon module), and count
what lands. Two ranges and two spreads over-determine it.

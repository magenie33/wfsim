# M31 — a riven's two elements enter the hierarchy backwards, and a combined element may block the chain (2026-08-07)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

From "带元素紫卡的最终元素适配可能有点问题" (owner, 2026-08-07), with one
weapon, one riven and two slot arrangements:

- riven card, top to bottom: **Multishot 87.9 · Toxin 71.2 · Electricity 72.9 ·
  Crit Damage −55.3** (owner, asked and answered: 先毒再电，从上往下)
- **A** — Magnetic / Cold / riven / Electricity → in game **磁力 / 毒 / 辐射**
- **B** — the Cold and the Magnetic swapped → in game **腐蚀 / 冰 / 辐射**

### What A settles

The wiki says it in the Damage page's own words: **"the hierarchy priority will
be given to the LAST elemental stat listed on the Riven mod"**, worked through
a riven with "+100% Electricity damage first and +90% Toxin damage last" where
the Toxin combines UP and the Electricity down. So a mod's own elements enter
the hierarchy in REVERSE of how its card prints them, and the engine was
entering them in print order.

The owner's card prints Toxin first and Electricity last, so the Electricity is
the one that reaches up to the Cold above it:

| | before | after |
|---|---|---|
| A | Viral + Electricity + Magnetic + Radiation | **Magnetic + Toxin + Radiation** |

which is his 磁力/毒/辐射 exactly. Only a riven can carry two elemental stats —
no mod under `data/mods/` has more than one, and one element reversed is itself
— so this changes the reading of riven builds and nothing else; the board did
not drift and no row on it carries a riven. The wiki's other half needed no
code: "if no other elemental damage mods are present, the elements on the Riven
mod will combine with itself" — reversed or not the pair stays adjacent, and
`/api/simulate` on the riven alone returns Corrosive.

### What B does NOT settle — the open question

Under the model in MECHANICS §3, **A and B are the same build**. Magnetic
Strafe grants an already-combined element, rule 7 keeps it outside the primary
hierarchy, and a thing outside the hierarchy cannot change where the Cold sits
relative to the riven. The engine returns Magnetic + Toxin + Radiation for
both. The game did not.

One model explains both readings, and it is a small change to rule 7: **a
combined element occupies its slot in the hierarchy and FLUSHES the pending
primary above it** — it does not combine, but a primary above it can no longer
reach a primary below it.

| | order | walk | result |
|---|---|---|---|
| A | Magnetic(c), Cold, Elec, Toxin | Magnetic passes · Cold+Elec = Magnetic · Toxin alone | 磁力/毒 ✓ |
| B | Cold, Magnetic(c), Elec, Toxin | Cold flushed pure · Elec+Toxin = Corrosive | 腐蚀/冰(/磁力) ✓ |

It is plausible as an implementation — one ordered list of every elemental
entry, walked once, keeping at most one primary pending — and it is
UNVERIFIED. It also predicts a Magnetic in B that the owner's list does not
name, which is either the list being partial (both arrangements carry a
Magnetic mod, so it is in both) or the model being wrong.

The engine is NOT changed on this. A hypothesis that fits one report is not a
measurement, and this one rewrites how every build with a 60/60 combined-element
mod reads.

### The experiment that settles it

Phantasma Prime, three mods, nothing else, in this slot order:

**Cold · Magnetic Strafe · Electricity**

| model | panel reads |
|---|---|
| today's engine | Magnetic + Radiation |
| the flush model | Cold + Electricity + Magnetic + Radiation |

There is no overlap, so one arsenal screenshot decides it. The control is the
same three mods as **Magnetic Strafe · Cold · Electricity**, which both models
read as Magnetic + Radiation.

# M120 — Secondary Fortifier is ×8, capped by the Overguard left, and checked as each instance lands ✅ (owner, 2026-10-22)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

## The readings

All with a secondary at 220 base carrying Secondary Fortifier at max rank
("x8 Extra").

- **The multiplier.** A body shot reads 512 and a 21× hit 10752 on a target
  without Overguard; the same two on Overguard read **4096** and **86016**.
  Both exactly ×8.
- **A hit that cannot break the pool on its own.** With 80000-odd of fortified
  damage in hand and less Overguard than that left, the pop reads exactly the
  Overguard left (one read 20000-odd), and the shield and health bars do not
  move.
- **A hit that breaks it on its own.** A hit worth 1000 into 500 of Overguard:
  500 to the Overguard, 500 on into shield and health — what it does without
  the arcane.
- **A Heat tick** (forced Heat, 200%): the hit reads 258048 = 8 × 32256; its
  ticks read 120000-odd (8 × 16128) while the Overguard holds and **16130**
  once it is gone. With the Overguard up throughout, 12288 → 6156 and
  27079 → 13551, the tick half its hit in both columns.
- **Xata's Whisper** at 10.4% (40% strength): 160 → 17 on health, **1280 → 200**
  on Overguard. 200 = 160 × 0.104 × 1.5 (Void's Overguard column) × 8: the
  extra hit is ×8 once, not ×64.

## The rule

The arcane is a property of the Overguard while it stands, like a status on the
target: every instance the secondary deals — hit, tick, extra hit, explosion, a
body it punched through — asks the body it lands on, at that moment.

Let `D` be what the instance deals to that body's Overguard without the arcane,
`G` the Overguard left:

| | Overguard takes | carries on to shield and health |
| --- | --- | --- |
| no Overguard | — | `D` |
| `D < G` | `min(8D, G)` | nothing |
| `D ≥ G` | `G` | `D − G`, no bonus |

So over one pool the arcane's extra damage is `G` minus what the hits put into
it at their own value — bounded by the pool, about `7G/8` at most — and none of
it ever reaches health.

## What it overturns

- **M38's ×9.** "x8 Extra" is the total, as on every other "xX" card; the
  stored bonus is 7 at max rank.
- **M82's unfortified carry.** The old model multiplied the whole instance and
  carried `D − G/8` past a broken pool; the carry is `D − G`, and only from an
  instance that breaks the pool without the arcane.
- **The bonus followed the aimed body.** It was multiplied into the shot when
  the aimed body had Overguard, so a body behind it or in the blast took ×9
  whatever its own pool. Each body now asks its own.
- **Only the secondary's own damage.** A Warframe does not carry the arcane,
  so none of its damage takes it; M38's reading that a Warframe's inherited
  Heat would is withdrawn.

## In the engine

`fight::target_state::apply` decides it, on every call site, from the body's
own Overguard; nothing multiplies it into `raw` beforehand. The combat record
shows it as the Overguard row's `Secondary Fortifier` factor, 1 when the
instance broke the pool alone. The fight's raw total no longer includes it; the
damage the target took does.

Pinned by `m120_fortifier_capped` and
`warframe_ability::m120_xatas_whisper_takes_fortifier_once`.

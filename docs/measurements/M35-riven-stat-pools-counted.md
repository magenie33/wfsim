# M35 — which riven stats a weapon can roll is not derivable, so it was counted (2026-08-08)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Reported** (owner, 2026-08-08, relaying a player): *"紫卡负面没投射速度。我们的
紫卡解析，还应该考虑灵化情况"* and then *"这个是盗贼的紫卡哈，灵化吃这个，所以就
可以装备"* — a real Furis riven carries Projectile Speed, the editor would not
offer it, and the reason given is that the Incarnon form uses it.

### What the editor was doing

`rivens_data::excluded_for` derived a weapon's rollable pool from two rules: the
wiki's *"weapons without more than 25% of a physical damage type usually cannot
roll that respective attribute… Exceptions exist on a case by case basis"*, and
"a stat the weapon does not have" (no ammo pool, nothing that flies, a sentinel
weapon nobody aims). GAUGE-SWITCHED forms were excluded from both, on the
argument that an Incarnon form is paid for with evolutions while a riven's pool
is fixed when it drops.

### The measurement

A survey of every riven family in the roster, from warframe.market's public
auction search — 26 families, up to 500 live listings each, ~12 000 real cards
(`scripts/survey_riven_pools.py`, output `data/rivens/pools.yaml`). A riven
carries 2-3 of ~24 class stats, so a stat that CAN roll appears in roughly 55 of
500 listings. Measured, a stat that rolls landed at **30-70** and a stat that
does not at **0-4**. Nothing real came near the floor, so the verdict is
three-way: rollable, never, or unclear — and unclear falls back to the rules.

**The derivation was wrong in both directions, on six of 26 families:**

| family | rules said | 500 cards say |
|---|---|---|
| Ocucor | no Impact/Puncture/Slash (9% Puncture, 91% Radiation) | all three roll (49/46/39) |
| Phantasma | Projectile Speed rolls (the plasma bomb flies at 25 m/s) | 0 of 500 |
| Phantasma | Zoom rolls | 0 of 500 — it has no scope, and no field says so |
| Boar | Zoom rolls | 1 of 500 |
| Phenmor | Puncture rolls (30%, over the line) | 0 of 500 |
| Karak Wraith | no Slash (7.75 of 31 is EXACTLY 25%, not "more than") | 45 of 424 |
| Sicarus | Puncture and Slash roll (Sicarus Prime is 30% each) | 0 and 0 |

Two entries at exactly 25% settle nothing between them: Karak Wraith's Slash is
25.00% and rolls, Vasto's Impact is 25.00% and does not. There is no threshold
that fits both, which is the point — DE's table is not a formula.

### What was decided

Three sources, in order: **a real card** → **a count over live listings** →
**the derivation**.

**WHICH FILE DECIDES:** the RULES do. `data/rivens/exceptions.yaml` overrides
them per family with each entry naming its evidence, and `data/rivens/pools.yaml`
— the survey — is read by a TEST and by nothing else (owner: "抓取只是来当验证
才对"; DATA_SOURCES §"Riven pools"). Everything below is what the survey FOUND,
and every finding became an exception entry carrying its count.

Why the survey does not decide: a re-run of the scrape came back "nothing rolls
anything" for all 26 families. Data the engine reads at calculation time would
have emptied every pool in the app without failing anything.

### On the Incarnon question specifically

**Counting the Incarnon form would have been wrong.** The Latron, Lex and Atomos
Incarnon forms each fire a literal travelling projectile, and their families show
**0, 4 and 0** Projectile Speed listings out of 500. The gauge-switched form stays
out of the derivation — but now because it was counted, not because of an
argument about when rivens drop.

The Furis is the mirror case and is why the exception list exists: hit-scan in BOTH
forms, 13 of 500 in the survey's unclear band, and a player's card carries it.
The likely reason is that DE's Incarnon form is a projectile internally whatever
the beam looks like — the wiki's Condition Overload catalog row for it reads
"Furis | Incarnon Mode | **Projectile**", and the Ocucor, the same held 12-tick
beam shape and hit-scan in our data, rolls Projectile Speed on 47 of 500 cards.
That is an explanation, not a rule: the Phantasma's bomb genuinely flies and
still rolls none.

### The negative half of the report

Projectile Speed **can** be the malus — the wiki's positive-only list is the four
elements plus Punch Through, and the survey finds negative Projectile Speed on
real cards. `data/rivens/*.yaml` already had it right (`malus: true`); it was
missing from the negative slot only because it was missing from the stat list
entirely.

### What would falsify it

Any in-game card carrying a stat this file marks `never`. Absence in 500 listings
is strong evidence and not a proof — one card beats the count, which is exactly
what `exceptions.yaml` is for.

# M64 — a Tome's meter is a clock you spend, and a kill leaves ammo on it ✅ (owner, 2026-08-28)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

The Grimoire's alt fire has never been fireable at will, and the roster said so
in the loudest admission it had: *"everything below for this form is the ceiling
rather than the average, and by a wide margin"*. This is that sentence replaced
by a number.

The page, verbatim:

> Requires a fully filled meter beneath the reticle in order to fire. The meter
> takes 45 seconds to completely recharge. Hitting enemies with the primary fire
> reduces recharge time by 1 second per hit. Picking up secondary or universal
> ammo reduces recharge time by 10 seconds. … Radial damage does not count an
> additional hit. Multishot will count as an additional hit.

### It is a third kind of gate, and it gets its own type

This roster already had two: a MAGAZINE you spend and reload, and an INCARNON
GAUGE you fill with hits. A meter is neither — it fills with TIME. The owner's
call on where to put it (2026-08-28):

> 这个机制目前不多的，你完全可以单独一个类型，等我们真的全部做完，再思考可不可
> 以重构为一种类型

So `MeterSpec` is its own type rather than a bent `GaugeSpec`. One weapon has it;
guessing the shared shape from one is how a wrong abstraction gets built.

WHAT IT DID NOT NEED WAS NEW MODE MACHINERY. `play_modes` reads "does entering
this form cost something you must earn" off the ENTRY rather than off its name,
which was written for the Mausolon and says so in its own comment (owner,
2026-08-07). Declaring a meter therefore moves the form from a sustainable
`alternate` a ruler may rank to a `transformed` mode showing the form's own
numbers, and adds the `cycle` that is how a Tome is played — none of it decided
here. `WeaponSpec::has_gauge` is the one question and now knows both gates.

### The cycle is not a transformation

An Incarnon cycle puts the weapon in the other form. A Tome never leaves its
primary fire — it THROWS the other form's orb, and the orb has been an entity
rather than a state since M63. So `tome_cycle_from_panels` is the base form's
params with the orb and the meter laid on top, and nothing in the shot loop has
to switch.

One thing had to move for it: `unaimed_headshot_chance` was on the ATTACK, and a
cycle fires two forms at once that disagree about it — you POINT the primary
fire and the orb picks its own body. It rides `ResolvedOrb` now, declared once in
the yaml and read only by the strikes.

### What a kill leaves, and why no enemy needed a drop table

The owner asked how the ten catalogued enemies implement ammo drops. They do
not, and they do not have to:

> *"Chance to drop Primary or Secondary Ammo scales with squad size"* — solo 45%
> (60% in Landscapes) … *"Eximus are guaranteed to drop either a Primary or
> Secondary Ammo, each having the same chance of dropping. This does not
> overwrite the enemies normal chance of dropping an Ammo pickup."*
> (wiki `Pickups`)

Ammo is a property of the SQUAD and the place, not of the body — a Lancer and a
Crewman drop it at the same rate. `engine::ammo` is that table; the only
per-enemy term is the Eximus guarantee, which is ADDITIONAL to the ordinary roll
(1.45 expected pickups solo, not 1.0) and which this engine already knows.

Only SECONDARY counts for a tome's meter, and universal packs are placed in a
Simulacrum rather than dropped, so a kill contributes through half its roll:
`0.45 × 0.5 × 10 = 2.25` seconds a kill.

EVERY DROP ARRIVES INSTANTLY (owner: *"我们的场景就假设怪物死掉以后所有的pickup
立刻马上到"*) — no vacuum radius, no walking back. And INFINITE AMMO does not
remove it: the house rule is about the reserve, a real fight is under its cap
almost always, and the pack is on the floor either way (owner, 2026-08-28).

### One orb, and a throw costs an animation

Two corrections that arrived after the meter did (owner, 2026-08-28):

> 同一时间只能有一个球，如果在前一个球存在的期间，再放，原来的球立刻消失。并且这
> 个球是有一个前摇时间的（类似投掷类武器那样），这个前摇时间是可以被fire rate降低）
>
> 主要应该是点击以后0.1s后射出去，间隔反正完美对应射击rate，次要是0.15s后射出去，
> 接着0.85s硬直，才可以继续主要模式。射速mod可以加速这两个动作

**One orb at a time.** A new throw makes the old one vanish — no detonation, no
strikes it had left. In the cycle this is free, because the meter puts throws
tens of seconds apart and a fuse is six; where it bites is anything that throws
faster than the fuse, where six strikes an orb becomes one.

**A throw is 0.15 s of wind-up and 0.85 s of recovery**, both shortened by fire
rate, and their sum is this form's listed fire rate of 1 — the animation IS the
cadence, which is the same fact the module states twice. In the cycle it is the
only price beyond the meter: the primary fire stops for a second every time an
orb goes out, measured at 256 pellets against 271.

**And the primary's own 0.1 s wind-up is modelled too**, which it nearly was
not. It was written off here as latency on the reasoning that the interval
*"corresponds exactly to the fire rate"*, so a sustained engagement fires the
same rounds and the mean does not move. The owner's answer:

> 为啥不建模啊，其他的枪械类武器都是0s子弹出膛，但是这个是0.1s啊，不也是变量吗 …
> 我们要严谨肯定要建模的

He is right, and the reason is the one this app is built on: the combat record's
claim is that a row can be laid beside a recording and checked number for
number, and a stream whose every timestamp is 0.1 s early fails that test. It
also reaches time-to-first-kill and the opening of the DPS curve — the two
figures a short engagement is read by — and it is a VARIABLE, shortened by fire
rate, not a constant to be waved away.

The implementation is one line, because the cadence being exact is what makes it
one: shot `k` lands at `windup + k / rate`, so the engagement STARTS at the
wind-up rather than each shot being delayed one at a time. Coming back from a
throw costs it again — an interval only corresponds to the fire rate while you
are holding the trigger down.

`a_round_leaves_after_the_windup_and_the_interval_is_still_the_rates` asserts the
TIMES rather than a total, which is the whole point: at 2/s it pins
`0.0, 0.5, 1.0 …` against `0.1, 0.6, 1.1 …`, and no aggregate can tell those
apart.

### Two modes, and `transformed` is not one of them

> transformed注意一点，不能套用！！！这个是灵化模式专属的 … 这本书我们应该有2个
> mode，一个是只使用主要射击模式，另外一个是使用主要射击，次要槽满了，再使用次
> 要，然后继续使用主要射击，就这两种

`Transformed` is a state you are IN — an Incarnon window, a form that fires its
own magazine for a few seconds — and the builder shows its numbers because
"while you are in it" is a real thing to ask. A metered form is not a state: you
throw one orb and you are back on the primary before it lands. So `play_modes`
emits only the CYCLE for it, and a Tome has exactly two ways to be played:

* `base` — the primary fire alone
* `cycle` — the primary fire, an orb whenever the meter fills, then the primary
  again

### What it is worth

Solo, unmodded, 180 s, the neutral Tenno:

| mode | what it means | DPS | orbs |
| --- | --- | --- | --- |
| `base` | the primary fire alone | 1,419 | — |
| `cycle` | the weapon | **1,795** | **10** |

The cycle throws ten orbs where the clock alone would give four: the primary's
hits take the 45 second meter down to about 18. Against a killable target the
ammo term shows on top of that — 43 kills bought two more orbs. The throw
animation is the other direction, and visible: 256 primary pellets rather than
the 271 the base form fires.

For scale, what this replaced: the alt fire used to be simulated as though you
could throw an orb every second forever, and reported **8,113 DPS**.

### Still not modelled, and one of them on purpose

* **Health and energy orbs.** The same page lists them and publishes no drop
  chance for either, and they would pay nothing here — this arena has no ability
  economy and the player has no health.
* **Resources.** The owner asked (*"甚至可不可以模拟素材掉落啊"*). A per-enemy
  table, and it feeds none of BUILD, SIMULATE or SOLVE — a farming calculator is
  a different product, which is the rule AGENTS.md states for anything new.
* **Heavy ammo**, the one ammo kind that IS per enemy (5.01% on specific heavy
  units). No Arch-Gun here reads a pickup yet.

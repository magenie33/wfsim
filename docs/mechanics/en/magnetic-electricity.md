# [Warframe] Magnetic & Electricity Status Mechanics & Calculation Guide

> **Author & Attribution**: WFSim | [wfsim.app](https://wfsim.app)  
> **Scope**: Calculation rules for Warframe **Magnetic** and **Electricity** status effects (plus **Gas**, which shares the same radial pool model), native weapon weakpoint tiers, and Simulacrum-calibrated numeric examples.

---

## 1. Why Cover Magnetic and Electricity Together?

In Warframe's elemental system, **Electricity** is a primary element and **Magnetic** (Cold + Electricity) is a combined element — yet under current game mechanics, the two are tightly coupled at the engine level:

1. **Breaking Shields or Overguard under Magnetic spawns Electricity directly**: It deals an immediate one-shot pure Electricity break packet **and forces an ordinary Electricity status proc**.
2. **Magnetic break procs share the exact same Electricity pool**: The forced Electricity proc joins the target's existing Electricity consolidated pool, sharing its clock and accumulator — and **rewriting the entire pool's body-part landing multiplier and headshot-kill classification**.
3. **Electricity and Gas share the same "second headshot landing layer"**: They are the only two status DoTs in the game whose ticks themselves *land on a specific body part*. Mastering Electricity's pool and landing-anchor rules explains Gas at the same time.

```mermaid
flowchart LR
    A["Weapon Hit Procs Electricity"] --> B["Snapshot Stack Seed S(i) (Base × Crit × Hit Part × Faction × Status Dmg)"]
    M["Shield / Overguard Breaks under Magnetic"] --> M1["One-Shot Electric Break Packet (Max Pool × 0.5% × Stacks, Native Weakness Norm. × Status Dmg)"]
    M1 --> M2["Forced 1 Stack Ordinary Electricity (Body Landing, 2nd Status Dmg → S²)"]
    B --> P["Target's Single Electricity Merged Pool (1 Tick/s, Independent 6s Timers, No Refresh)"]
    M2 --> P
    P --> R["Accumulator = 1 + Σ S(i) → max(1, 0.5 × Acc) × (1 + Elec%) × Faction × Landing L"]
    R --> T["3m Tesla Arc to Host & Neighbors (Whole Pool's L & Headshot Kill = Latest Stack's Part)"]
```

---

## 2. Core Mechanic I: Electricity Status (Tesla Chain) Full Calculation

### 1. Base Properties & the "Third DoT Resolution Model"

Warframe's damage-over-time statuses follow **three** distinct engine architectures; Electricity (and Gas) use the third:

* **Model 1 (Independent ticks, independent timers)**: Slash (Bleed), Toxin. Each stack displays its own tick number and expires independently after 6 s.
* **Model 2 (Merged tick, refreshes all timers)**: Heat. All stacks merge into one tick per second, and every new stack **refreshes the duration of all existing stacks**.
* **Model 3 (Merged tick, independent timers — NO refresh)**: **Electricity** and **Gas**. All active stacks on a target merge into a single damage number ticking once per second, **yet each stack keeps its own independent 6-second timer and never refreshes older stacks**.

Five hard parameters of Electricity:
* **Base Duration**: **6 seconds** (scales with Status Duration).
* **0-Second Delay (Instant First Tick)**: Tick 1 fires the instant the proc is applied (`t = 0 s`), then once per second (`t = 0, 1, 2, 3, 4, 5 s` — **6 ticks** at base duration; unlike Slash/Heat/Toxin, there is no 1-second wait, and even extremely negative duration still fires the `t = 0` tick).
* **Stack Cap**: **Unlimited**.
* **Area Propagation**: Every tick arcs to all enemies within a **3-meter radius** of the host.
* **Stun Crowd Control (verified in-game)**: Only the directly afflicted host suffers the ~**3-second** electric stun (neighbors hit by the 3 m arc take damage only, no stun). Furthermore, **the electric stun cannot be refreshed while running** — it must finish completely before a new Electricity proc can stun again (unaffected by Status Duration), unlike Heat's continuously refreshed panic.

---

### 2. Exact Electricity Damage Formula

#### (1) Single-Stack Seed (Snapshotted Individually)
Each Electricity proc creates an independent damage seed inside the merged pool, locking in the triggering hit's offensive multipliers:
```text
Stack Seed S(i) = Modded Base Damage × Crit Multiplier × Hit-Part Multiplier × Faction Multiplier × Status Damage Multiplier
```
* **Modded Base Damage** includes base damage and base-damage modifiers only (no elemental mod damage, and no `1/32` direct-hit quantization; beam weapons also multiply by merged multishot `M` and current ramp-up multiplier).
* **Hit-Part Multiplier**: `1` on body; `3 × (1 + Headshot Damage%)` on standard heads. Headshot crits fold `1 + Crit Tier × (2 × Crit Damage - 1)` into the seed.

#### (2) Exact Consolidated Pool Tick Formula
On each tick, the engine sums all living seeds in the pool starting from an initial accumulator value of **`1`**:
```text
Pool Accumulator = 1 + Σ S(i)                       ← +1 added once for the entire pool
Base Tick Damage = max(1, 0.5 × Pool Accumulator) × (1 + Electricity Mod%) × Faction Multiplier
Final Tick Damage = Base Tick Damage × Landing Multiplier L × Live Armor / Resistances / Viral
```
* **Electricity Mod Scaling**: Electricity ticks scale directly with `(1 + Electricity Mod%)`.
* **Faction Double-Dip**: Faction multipliers apply once inside each seed `S(i)` and a second time on the pooled tick output (quadratic scaling, with the `+1` initializer receiving only the outer pass).
* **Status Damage**: Applies on the seed side (does not scale the `+1` initializer).

---

### 3. Second Headshot Landing Layer `L` & Latest-Application Anchor

Electricity and Gas ticks carry an additional body-part landing multiplier (verified in-game):

#### (1) Why Electricity Headshots Outscale Toxin/Heat by an Extra Layer
* Toxin, Heat, Slash, and Blast ticks do **not** land on a specific body part — headshot damage applies only once inside the initial seed `S(i)`.
* **Only Electricity and Gas** ticks themselves **land on a body part**.
* When the pool's tick lands on the **head**, the engine multiplies the **entire consolidated tick (including the `+1` initializer)** by a second headshot-damage factor (Deadhead arcanes, Incarnon evolution headshot bonuses, etc.; excludes weakpoint-acuity mods):
  ```text
  Head Landing Multiplier L = 1 + Headshot Damage%    (L = 1.0 on body)
  ```
* **Numeric Example**: Suppose a weapon has `+80%` headshot damage (e.g. +50% Evolution + 30% Secondary Deadhead):
  * Standard headshot hit multiplier = `3 × (1 + 0.8) =` **5.4×**
  * **Toxin / Heat headshot** vs. body: **5.4×**
  * **Electricity (and Gas) headshot** vs. body: `5.4 × (1 + 0.8) =` **9.72×** (~**9.75×** including the `+1` initializer) — headshot damage applies twice on Electricity and Gas.

#### (2) "The Latest Application Anchors the Entire Pool's `L` and Headshot-Kill Credit"
When a target holds both body-applied and head-applied Electricity stacks, two rules govern the pool:
1. **Every old seed stays intact**: Earlier headshot (or headshot-crit) seeds are never overwritten or shrunk by later body hits.
2. **However, the entire pool's landing multiplier `L` — and whether a lethal DoT tick counts as a "headshot kill" — is determined solely by the body part of the most recently applied Electricity stack.**
   * Apply 10 head Electricity procs, then accidentally land 1 body Electricity proc → **the entire pool instantly drops to body landing (`L = 1.0`)**, and if the tick kills the target, **it does NOT count as a headshot kill**.
   * Conversely, apply 10 body Electricity procs, then finish with 1 head Electricity proc → **the entire pool (including all 10 earlier body seeds) switches to head landing (`L = 1 + Headshot Damage%`)**, and a lethal tick **counts as a headshot kill**.

#### (3) How Does the 3 m Arc Hit Neighbors?
* When a host's head-seeded Electricity arcs to neighbors within 3 m, **the arc carries the host's full headshot seed** (neighbors receive the `5.4×` head-seeded arc).
* Whether the arc also triggers the second head landing multiplier `L` on a neighbor depends on geometry. Across 189 in-game trials at varied positions, the arc landed on a neighbor's head **~5.3% of the time (10/189)**, and on the neighbor's body (`L = 1.0`) the remaining ~94.7%.

---

## 3. Comparison: Gas vs. Electricity

Gas (Heat + Toxin) belongs to the exact same radial-pool + body-part-landing family as Electricity. Remember **4 similarities** and **4 differences**:

* **4 Identical Rules**:
  1. **0-second delay, instant first tick** (`0..5 s`, 6 ticks at base duration);
  2. **Merged tick, independent timers (no refresh)** with the `+1` accumulator;
  3. **Same second landing multiplier `L`**, governed by the exact same **"latest Gas stack decides the whole pool's `L` and headshot-kill credit"** rule (verified in-game);
  4. Gas cloud kills are credited to the applying weapon (verified in-game).
* **4 Key Differences**:
  1. **No elemental mod scaling**: Electricity ticks scale with `(1 + Electricity%)`; Gas is a combined element and **scales with neither Heat nor Toxin mods**.
  2. **Stack cap & growing radius**: Electricity has **no stack cap and a fixed 3 m radius**; Gas caps at **10 stacks**, starting at 3 m and expanding by `+0.3 m` per additional stack up to **6 m**.
  3. **Persists after host death**: Electricity arcs vanish immediately when the host dies; **Gas clouds remain anchored in space and finish ticking their remaining duration**, damaging enemies who walk through.
  4. **No stun**: Electricity stuns its host for ~3 s; Gas has no crowd-control effect.

---

## 4. Core Mechanic II: Magnetic Status (Disrupt) & Magnetic Break Procs

Magnetic (Cold + Electricity) must be calculated in two distinct phases: **before the break** and **at the moment of break**.

```mermaid
flowchart LR
    A["Target Has N Magnetic Stacks (1–10)<br/>+100% to +325% vs. Shields & Overguard"] -->|"Weapon B Breaks Shield or Overguard"| B["Snapshot Stack Count N & Max Pool<br/>Q = trunc(Max Pool × 0.005 × N)"]
    B --> C["Native Weakness Norm. × Status Dmg S × Proxy A<br/>Norm = round(Q ÷ (1 + max(W,0) × V))<br/>Break Seed = Norm × S (1st) × A × Final Buff"]
    C --> D1["Stage 1: One-Shot Pure Electric Break Packet<br/>Break Seed × Faction × Armor / Resistances<br/>(Body hit; still deals damage if Electric-immune)"]
    C --> D2["Stage 2: Forced 1 Stack Ordinary Electricity<br/>Proc Seed = Break Seed × S (2nd) × Faction<br/>(S² Status Dmg double-dip; uses Weapon B Elec%/Dur/Faction)"]
    D2 --> E["Joins Ordinary Electricity Pool as Body Hit<br/>(Sets pool to body; a later head Electric proc restores head landing)"]
```

### 1. Phase 1: Before the Break (Magnetic Disrupt Stacks)

* **Duration & Cap**: **6 seconds** per stack, capped at **10 stacks** (shared FIFO slots across all sources; new stacks beyond 10 replace the oldest).
* **Bonus Damage to Shields & Overguard**:
  * 1st stack: **+100%** (`2.00×`)
  * Each additional stack: **+25%**
  * Max **10 stacks**: `100% + 9 × 25% =` **+325%** (**4.25×**)
* **Secondary Effect**: Prevents natural shield regeneration.
* *Boundary*: Magnetic's damage amplification applies strictly to **Shields** and **Overguard** — never Health.

---

### 2. Phase 2: The Break Moment — Two-Stage Magnetic Break Formula

The instant a target carrying `N` Magnetic stacks (`1 ≤ N ≤ 10`) has its **Shield** or **Overguard** broken, the game **snapshots `N`** (later Magnetic decay or stacks do not alter the break) and runs **two consecutive stages**:

1. **Stage 1: One-Shot Pure Electricity Break Damage Packet** (fixed body hit, cannot crit, ignores headshot bonuses, ignores `+Magnetic%`; even if the target is immune to Electricity *procs*, Stage 1 direct damage still lands).
2. **Stage 2: Forced 1 Stack of Ordinary Electricity Status** (pushed straight into the target's ordinary Electricity merged pool: 6 s duration, 0 s first tick, 3 m arc).

#### Attribution Rule: All Modifiers Come from the Breaking Weapon
Even if Weapon A stacked 10 Magnetic procs beforehand, whichever **Weapon B lands the breaking blow** supplies the entire modifier context for both stages — **native weakpoint bonus `W`, Electricity mod%, Status Damage, Status Duration, and Faction multipliers** all come from Weapon B.

#### Exact 4-Step Calculation

* **Step 1: Raw Pool Source `Q`**  
  The effective base coefficient is `0.005` (**0.5%** of maximum Shield or Overguard per Magnetic stack — **5%** at 10 stacks), truncated to an integer:
  ```text
  Q = trunc( Max Shield or Overguard × 0.005 × Magnetic Stacks N )
  ```
* **Step 2: Native Weakness Normalization & Target Proxy Multiplier**  
  The game selects the target's first **strictly greatest positive native weakness proxy**. If that proxy carries a generic damage multiplier `A`, it first divides `Q` by **`1 + max(W, 0) × V`** using the **breaking weapon's innate weakpoint bonus `W`**, rounds half-up, and then multiplies by Status Damage `S` (1st pass), proxy multiplier `A`, and any final buff:
  ```text
  Normalized Source = round( Q ÷ (1 + max(W, 0) × V) )
  Break Base Seed   = Normalized Source × Status Damage S × Proxy Multiplier A × Final Buff (e.g. Eclipse)
  ```
  * *Boundary*: If the target has no positive weakness proxy (e.g. Acolytes with `V = 0`) or its winning proxy has no generic multiplier `A` (e.g. Techrot Babau), **both normalization and `A` are skipped**: `Break Base Seed = Q × S × Final Buff`, making all `W` tiers deal identical break damage.
* **Step 3: Stage 1 — One-Shot Pure Electricity Break Packet**
  ```text
  Stage 1 Direct Packet = Break Base Seed × Faction Multiplier × Live Armor / Resistances
  ```
  * **Cascading Breaks**: If this direct packet breaks the next protective layer underneath (e.g. breaking Overguard triggers a Stage 1 packet that immediately shatters Shields below it), it **cascades into a second Magnetic break proc**.
* **Step 4: Stage 2 — Forced Ordinary Electricity Proc (Status Damage `S²`)**  
  The break packet forces 1 stack of ordinary Electricity into the shared Electricity pool:
  ```text
  Forced Electricity Seed = Break Base Seed × Status Damage S × Faction Multiplier
  ```
  * **Status Damage Double-Dip (`S²`)**: `Break Base Seed` already received Status Damage `S` in Step 2, and the forced Electricity child proc multiplies by `S` a **second time** in Step 4 — scaling the resulting Electricity DoT ticks by **`S²`**. At `+90%` Status Damage (`S = 1.9`), the ticks scale by `1.9² =` **3.61×**.
  * **Everything else follows ordinary Electricity**: Uses Weapon B's **`1 + Electricity%`**, **Status Duration**, **Faction double-dip**, and **3 m area chain**; **ignores `+Magnetic%`**.

---

### 3. Understanding Native Weakpoint Bonus `W`: All 5 Weapon Tiers & Representative Weapons

#### (1) What Exactly Is the Innate Weakpoint Bonus `W`?
* **`W` is an innate hidden property built into each weapon's base profile** — completely independent of equipped headshot mods, Deadhead arcanes, or Incarnon headshot evolutions (stacking headshot damage never changes `W`).
* **Incarnon forms and alt-fires inherit their base weapon's `W`**: e.g. Torid & Torid Incarnon are both `W = 0.2`; Laetum & Laetum Incarnon are both `W = 0.5`; Miter & Miter Incarnon are both `W = 1.0`.
* **Why does a LOWER `W` deal MORE Magnetic break damage?**  
  In Step 2, `W` sits in the **denominator** `1 + max(W, 0) × V`, followed by multiplying by the target's generic proxy multiplier `A`. Against standard humanoids (`V = 1.0, A = 2.0`), the net multiplier is **`2 ÷ (1 + W)`**. Thus a `W = 0` weapon gets the full `2.00 × Q`, whereas a `W = 1.0` weapon gets divided by `2` and multiplied back by `2`, ending at `1.00 × Q` — half of a `W = 0` weapon.

#### (2) Complete Table of All 5 `W` Tiers in Warframe (WFSim Calibrated Reference)

| Native `W` | Humanoid Net Mult `2÷(1+W)` | 10-Stack Equiv. % of Max Pool | Weapon Class Pattern | Representative Weapons |
| :---: | :---: | :---: | --- | --- |
| **`0.0`** | **`2.00 × Q`** | **10.00%** | **All Melee weapons**, most Archguns & companion claws; plus non-weakpoint beams, wave guns, launchers, and thrown/bubble secondaries | **Melee**: Every melee weapon (Glaive Prime, Xoris, Harmony, Hate, Okina Prime, etc.)<br>**Primary**: Amprex, Ignis Wraith, Tenet Arca Plasmor, Kuva Ogris, Kuva Tonkor, Tenet Envoy, Stahlta, Scourge Prime<br>**Secondary**: Kuva Nukor, Atomos, Gammacor / Synoid Gammacor, Kompressa Prime, Spectra Vandal, Coda Pox, Prisma Angstrum<br>**Archgun**: Grattler, Imperator Vandal, Kuva Ayanga, Corvas Prime |
| **`0.2`** | **`1.67 × Q`** | **8.33%** | Most automatic rifles, shotguns, full-auto pistols, Sentinel guns, and several beam/AoE weapons | **Primary**: Torid (incl. Incarnon), Tenet Glaxion, Phantasma Prime, Boar Prime, Strun Prime, Felarx, Cedo Prime, Kuva Sobek, Kuva Bramma, Kuva Zarr, Kuva Chakkhurr, Braton Prime, Boltor Prime, Soma Prime<br>**Secondary**: Tenet Cycron, Ocucor, Furis, Dual Toxocyst, Epitaph / Epitaph Prime<br>**Sentinel**: Verglas Prime, Helstrum |
| **`0.4`** | **`1.43 × Q`** | **7.14%** | **Exactly 6 classic sniper rifles** | Vectis / Vectis Prime, Vulkar / Vulkar Wraith, Snipetron / Snipetron Vandal |
| **`0.5`** | **`1.33 × Q`** | **6.67%** | Semi-auto / burst rifles, precision pistols, most throwing knives, and several Incarnon guns | **Primary**: Phenmor, Burston Prime, Latron Prime, Sybaris Prime, Tiberon Prime, Lenz / Prisma Lenz, Ferrox<br>**Secondary**: Laetum (incl. Incarnon), Lex Prime, Sicarus Prime, Knell Prime, Tenet Plinx, Coda Catabolyst, Athodai Prime, Grimoire, Marelok<br>**Archgun**: Mausolon, Cyngas |
| **`1.0`** | **`1.00 × Q`** | **5.00%** | Traditional bows, high-tier snipers / rail cannons, and select explosive outliers (half of `W = 0`) | **Primary**: Lanka, Rubico / Rubico Prime, Perigale Prime, Opticor / Opticor Vandal, Nataruk, Dread, Paris Prime, Cernos Prime, Daikyu Prime, Cinta, Miter (incl. Incarnon), Acceltra / Acceltra Prime<br>**Secondary**: Akarius / Akarius Prime |

#### (3) How Target Weakness Proxies `(V, A)` Change the Outcome
Besides the weapon's `W`, the enemy's highest positive weakness proxy `(V, A)` shapes Step 2:
* **Standard Humanoids** (Corrupted Heavy Gunner, Corrupted Crewman, Thrax Centurion, Legacyte, etc.): Head proxy **`V = 1.0, A = 2.0`** → net `2 ÷ (1 + W)` (full 2× spread between `W = 0` and `W = 1.0`).
* **Demolisher Devourer**: Winning proxy **`V = 0.05, A = 2.0`** → net `2 ÷ (1 + 0.05W)` (denominator stays `1.00–1.05`, so all 5 `W` tiers stay close to `2.00 × Q`).
* **Techrot Babau** (`V = 1.0` with no generic `A`) and **Acolytes** (no positive weakness proxy, `V = 0`): Normalization and `A` are skipped → all weapons receive a flat **`1.00 × Q`**, regardless of `W`.

---

### 4. Two-Way Interaction Between Magnetic Break Procs & Headshot Electricity Pools

Because the forced Electricity proc from a Magnetic break **always enters the shared Electricity pool as a body hit**, two interactions follow:

1. **Breaking drops an existing head-anchored pool back to body**: If you built a head-anchored Electricity pool (`L = 1.8`) before the break, the forced body proc becomes the latest stack at the break instant → **temporarily pulling the whole pool down to `L = 1.0`** (old head seeds remain intact).
2. **A post-break headshot Electricity proc moves the Magnetic break seed onto head landing:** If you proc ordinary Electricity on the target's head before the next tick, the pool flips back to `L = 1.8` — **multiplying even the huge Shield/Overguard-derived Magnetic break seed by `1 + Headshot Damage%`.**

---

## 5. Four Measurement-Calibrated Worked Examples

### Example 1: Electricity (& Gas) Mixed Hit-Order Walkthrough (In-Game Verified)

* **Setup**: Laetum base form (modded base `160`, quantized yellow-crit mult `c ≈ 2.2037`), `+80%` headshot damage (head seed part mult `P = 3 × 1.8 = 5.4`, second landing mult `L = 1.8`), `+200%` Electricity (`×3`), unarmored target.
* **Per-Stack Contribution (after `× 0.5`)**:
  * Body non-crit: `0.5 × 160 =` **80**
  * Head non-crit: `0.5 × 160 × 5.4 =` **432**
  * Head yellow crit: `0.5 × 160 × 5.4 × (2 × 2.2037) =` **1,903.97**
  * Pool `+1` initializer: `0.5 × 1 =` **0.5** (once for the whole pool)

| Sequence of Electricity Procs | Exact Pooled Tick Formula | Displayed Tick | Counts as Headshot Kill? |
| --- | --- | :---: | :---: |
| **Head(non-crit) → Head(non-crit) → Body(non-crit)** | `(432 + 432 + 80 + 0.5) × 3 × 1.0` | **2,834** | **No** (last hit = body) |
| **Body(non-crit) → Head(non-crit) → Head(non-crit)** | `(80 + 432 + 432 + 0.5) × 3 × 1.8` | **5,100** | **Yes** (last hit = head) |
| **Head(crit) → Head(non-crit) → Body(non-crit)** | `(1903.97 + 432 + 80 + 0.5) × 3 × 1.0` | **7,249** | **No** (keeps crit seed, loses 1.8× landing) |
| **Body(non-crit) → Body(non-crit) → Head(crit)** | `(80 + 80 + 1903.97 + 0.5) × 3 × 1.8` | **11,148** | **Yes** (whole pool gets 1.8× landing) |

> Compare Rows 1 and 2: identical hit counts (2 heads, 1 body), yet merely changing whether the **final** proc hit the head or body shifts the tick by a full **1.8× (2,834 vs. 5,100)**. Gas follows the exact same math.

---

### Example 2: Magnetic Overguard Break Step-by-Step — Naked, Electricity Mod, `S²` Double-Dip, Armor & Post-Break Head Pull

* **Target**: Level 50 Steel Path Corrupted Heavy Gunner Eximus (armor stripped, max Overguard = **`103,623`**, head proxy `V = 1.0, A = 2.0`) with **10 Magnetic stacks** at break.
* **Breaking Weapon**: **Laetum** (native **`W = 0.5`**, `+80%` headshot damage, `+200%` Magnetic).
* **Step-by-Step Calculation**:
  1. **Step 1 (Raw Source `Q`)**:  
     `Q = trunc(103,623 × 0.005 × 10) = trunc(5,181.15) =` **5,181**
  2. **Step 2 (Native Weakness Normalization — independent of mods)**:  
     `Normalized = round(5,181 ÷ (1 + 0.5 × 1.0)) = round(5,181 ÷ 1.5) =` **3,454**
  3. **Progression as Each Multiplier Is Added**:
     * **① No Electricity mod, no Status Damage, no Faction (baseline control)**:
       * Break Base Seed = `3,454 × 1.0 × 2 =` **6,908** → **Stage 1 One-Shot Break Packet = 6,908**
       * Stage 2 Forced Electricity Tick = `0.5 × (1 + 6,908) = 3,454.5 →` **3,455** (6 ticks = **20,730** total)
       * *(In-game testing records **3,455** on both body and head breaking shots — confirming `+80%` headshot and `+200%` Magnetic do not affect the break packet itself.)*
     * **② Add `+165%` Electricity to the breaking weapon**:
       * Stage 1 Direct Packet (unaffected by Electricity%): still **6,908**
       * Stage 2 Electricity Tick = `0.5 × (1 + 6,908) × (1 + 1.65) = 9,154.43 →` **9,154** (matches in-game measurement **9,154** to the digit)
     * **③ Add `+90%` Status Damage (`S = 1.9`)**:
       * Break Base Seed (1st `S`) = `3,454 × 1.9 × 2 =` **13,125.2** → **Stage 1 Direct Packet = 13,125**
       * Forced Electricity Seed (2nd `S` → **`S²`**) = `13,125.2 × 1.9 =` **24,937.88**
       * Stage 2 Electricity Tick = `0.5 × (1 + 24,937.88) × 2.65 = 33,044.02 →` **33,044** (`1.9² = 3.61×` over 9,154)
     * **④ Add `+55%` Faction Multiplier (`1.55`)**:
       * Stage 1 Direct Packet (1× faction) = `13,125.2 × 1.55 =` **20,344**
       * Forced Electricity Seed (1st faction) = `24,937.88 × 1.55 = 38,653.71`
       * Stage 2 Electricity Tick (2nd faction) = `0.5 × (1 + 38,653.71) × 2.65 × 1.55 = 79,387.12 →` **79,387**
     * **⑤ If target still has `900` Armor** (`DR = 0.9 × √(900/2700) = 51.96%`, multiplier `0.4804`):
       * Stage 1 Direct Packet = `20,344 × 0.4804 =` **9,773**; Stage 2 Body Tick = `79,387 × 0.4804 =` **38,136**.
     * **⑥ Armor-stripped, then land 1 headshot Electricity proc after break (pulls pool to `L = 1.8`)**:
       * The Magnetic break seed's own tick contribution increases to `79,387.12 × 1.8 =` **142,897** (before adding the follow-up headshot's own Electricity seed).

---

### Example 3: All 5 `W` Tiers Breaking the Same Shield Side-by-Side (In-Game Controls)

#### Scenario A: Level 100 Steel Path Corrupted Crewman (Max Shield `19,157`, 10 Magnetic Stacks, `V = 1.0, A = 2.0`)
* **Shared Raw Source `Q`**: `trunc(19,157 × 0.005 × 10) = trunc(957.85) =` **957**
* Comparing all **5 native `W` tiers** both naked and with `+165%` Electricity + `+90%` Status Damage (`S = 1.9`, unarmored):

| Breaking Weapon Tier & Examples | Normalized `round(957÷(1+W))` | Naked Stage 1 Packet (`×2`) | Naked Elec Tick (`0.5×(1+Pkt)`) | +165% Elec +90% Status: Stage 1 | +165% Elec +90% Status: Elec Tick (`S²`) | Ratio vs. `W=1.0` |
| --- | :---: | :---: | :---: | :---: | :---: | :---: |
| **`W = 0.0`**: Kuva Nukor / Glaive Prime | `957 ÷ 1.0 =` **957** | **1,914** | **958** *(meas. 958)* | **3,637** | **9,156** | **2.00×** |
| **`W = 0.2`**: Torid / Tenet Cycron | `957 ÷ 1.2 =` **797*** | **1,594** | **798** | **3,029** | **7,626** | **1.66×** |
| **`W = 0.4`**: Vectis Prime / Vulkar Wraith | `957 ÷ 1.4 =` **684** | **1,368** | **685** | **2,599** | **6,545** | **1.43×** |
| **`W = 0.5`**: Laetum / Burston Prime | `957 ÷ 1.5 =` **638** | **1,276** | **639** | **2,424** | **6,105** | **1.33×** |
| **`W = 1.0`**: Lanka / Rubico Prime / Nataruk | `957 ÷ 2.0 =` **479** | **958** | **480** | **1,820** | **4,584** | **1.00×** |

> *Note: Mathematically `957 ÷ 1.2 = 797.5`, due to single-precision floating-point arithmetic (`1.2` ≈ `1.20000005`), yielding `797.49997`, which rounds cleanly to `797`.

#### Scenario B: Cross-Level, Shield Disruption & 1-Stack In-Game Controls
The same 4-step formula reproduces every cross-level and single-stack control verbatim:
* **Kuva Nukor (`W = 0`), 10 Magnetic stacks, Steel Path Corrupted Crewman**:
  * Level 80 (max Shield `16,128`) → `Q = 806` → tick `0.5 × (1 + 1,612) =` **807** (measured 807)
  * Level 80 + Shield Disruption `-18%` (max Shield `13,225`) → `Q = 661` → tick `0.5 × (1 + 1,322) =` **662** (measured 662)
  * Level 100 (max Shield `19,157`) → `Q = 957` → tick **958** (measured 958)
  * Level 120 (max Shield `21,948`) → `Q = 1,097` → tick **1,098** (measured 1,098)
* **1 Magnetic stack — Gammacor (`W = 0`) vs. Lanka (`W = 1.0`)**:
  * Crewman (max Shield `14,789`, `Q = 73`): Gammacor `73 × 2 = 146` → tick **74**; Lanka `round(73/2) × 2 = 74` → tick **38** (exact match to 74 vs. 38).
  * Elite Crewman (max Shield `18,486`, `Q = 92`): Gammacor tick **93**; Lanka tick **47** (exact match to 93 vs. 47).

---

### Example 4: Comparing Target Weakness Proxies `(V, A)` Under Full Modifiers & 900 Armor

* **Uniform Setup**: **`100,000`** max Shield or Overguard broken at **10 Magnetic stacks** (`Q = 5,000`); breaking weapon carries **`+165%` Electricity**, **`+90%` Status Damage (`S = 1.9`)**, **`+55%` Faction (`1.55`)**, and **`+80%` Headshot Damage (`L = 1.8`)**; exposed Health underneath has **`900` Armor** (`51.96%` DR, `0.480385` health multiplier).
* **Full Walkthrough (Standard Humanoid, `W = 0.0`)**:
  1. Normalized = `5,000 ÷ (1 + 0 × 1) = 5,000`
  2. Break Base Seed = `5,000 × 1.9 × 2 = 19,000`
  3. **Stage 1 Direct Packet (post-armor)** = `19,000 × 1.55 × 0.480385 =` **14,147**
  4. **Stage 2 Electricity Seed** = `19,000 × 1.9 × 1.55 = 55,955`
  5. **Stage 2 Body Tick (post-armor)** = `0.5 × (1 + 55,955) × 2.65 × 1.55 × 0.480385 =` **55,206** (6 ticks = **331,236**)
  6. **After Follow-Up Headshot Electricity Proc (`L = 1.8`, post-armor)** = `55,206 × 1.8 =` **99,370** (~**596k** over 6 ticks from the break seed alone)

Full comparison across **3 target proxy classes** and **3 weapon `W` tiers** (all post-armor):

| Target Class & Proxy `(V, A)` | Weapon `W` | Normalized | Stage 1 Packet (Post-Armor) | Stage 2 Body Tick (Post-Armor) | Post-Break Head-Pull Tick (`L=1.8`, Post-Armor) |
| --- | :---: | :---: | :---: | :---: | :---: |
| **Standard Humanoid (Heavy Gunner / Crewman / Thrax)**<br>`V = 1.0, A = 2.0` | `W = 0.0` | 5,000 | **14,147** | **55,206** | **99,370** |
| Same | `W = 0.5` | 3,333 | **9,431** | **36,800** | **66,241** |
| Same | `W = 1.0` | 2,500 | **7,074** | **27,603** | **49,686** |
| **Demolisher Devourer**<br>`V = 0.05, A = 2.0` | `W = 0.0` | 5,000 | **14,147** | **55,206** | **99,370** |
| Same | `W = 0.5` | 4,878 | **13,802** | **53,859** | **96,946** |
| Same | `W = 1.0` | 4,762 | **13,474** | **52,578** | **94,640** |
| **Techrot Babau / Acolytes**<br>No generic proxy mult (skips norm. & `A`) | Any `W` (0 / 0.5 / 1.0) | 5,000 *(×1)* | **7,074** | **27,603** | **49,686** |

> Summary: ① On standard humanoids, `W = 0` weapons deal **2.0×** the Magnetic break damage of `W = 1.0` weapons; ② On `V = 0.05` targets like Demolisher Devourer, all 5 `W` tiers stay within **5%** of one another; ③ On Techrot Babau or Acolytes (lacking generic `A = 2`), every weapon resolves at a uniform `1.00 × Q`.

---

## 6. Magnetic / Electricity / Gas Quick-Reference Comparison

| Dimension | Magnetic (Disrupt + Break Proc) | Electricity (Tesla Chain) | Gas (Gas Cloud) |
| --- | --- | --- | --- |
| **Elements** | Cold + Electricity | Primary (Electricity) | Heat + Toxin |
| **Base Duration** | 6 s (forced Electricity proc gets its own 6 s) | 6 s | 6 s |
| **First-Tick Delay** | Instant break packet + 0 s forced Electricity tick | **0 s delay** (`0..5 s`, 6 ticks) | **0 s delay** (`0..5 s`, 6 ticks) |
| **Stack Cap** | 10 stacks (FIFO replacement) | **Unlimited** | **10 stacks** |
| **Pool & Refresh Model** | Snapshots count on break; forced proc joins Elec pool | Merged tick, independent timers (no refresh) | Merged tick, independent timers (no refresh) |
| **Elemental Mod Scaling** | Ignores Magnetic%; **scales with breaker's Electricity%** | **Yes** `(1 + Electricity%)` | **No** (ignores Heat & Toxin mods) |
| **Status Damage Scaling** | Packet 1×, forced Electricity proc **double-dips (`S²`)** | 1× (`S`) | 1× (`S`) |
| **Native Weakpoint `W`** | **Inverse normalization**: `2÷(1+W)` on humanoids (lower `W` = higher dmg) | None (uses normal hit part & `L`) | None (uses normal hit part & `L`) |
| **2nd Landing Layer `L`** | Forced proc enters as body; anchored by latest Elec stack | **Yes** (latest stack decides whole pool's `L` & HS kill) | **Yes** (latest stack decides whole pool's `L` & HS kill) |
| **Area Radius** | Forced proc inherits Electricity's 3 m arc | Fixed **3 m** (neighbors keep head seed, ~5.3% head land) | **3 m base**, `+0.3 m`/stack (max **6 m**) |
| **Persists After Death** | Vanishes with Electricity host | Vanishes on host death | **Cloud stays in place and finishes ticking** |
| **CC / Utility** | +100%–+325% vs. Shield/Overguard, blocks shield regen | ~3 s host stun (**cannot refresh while active**) | None |

---

## 7. Common Misconceptions

1. **Myth: "Magnetic break always deals 3%/stack of max Shield/Overguard, identically across all weapons."**  
   → **Reality**: The effective base coefficient is `0.005` (0.5%/stack), normalized by `÷ (1 + max(W, 0) × V)` and scaled by proxy multiplier `A`. Consequently, **lower native `W` yields higher Magnetic break damage** — on humanoids, `W = 0` weapons (all melee, Kuva Nukor, etc.) deal **2×** the break damage of `W = 1.0` weapons (Lanka, Rubico Prime, etc.).
2. **Myth: "Equipping Deadhead or headshot damage increases `W` and hurts Magnetic break damage."**  
   → **Reality**: `W` is an inherent base weapon property completely unaffected by headshot mods, arcanes, or evolutions. Headshot damage never lowers Magnetic breaks — and actually boosts them once a post-break headshot Electricity proc pulls the pool onto head landing `L`.
3. **Myth: "Magnetic break uses the Magnetic applier's mods and scales with `+Magnetic%`."**  
   → **Reality**: Every stat comes from the **weapon that breaks the Shield or Overguard**. The forced Electricity proc ignores `+Magnetic%` and scales with the breaking weapon's **`+Electricity%`**, Status Duration, Faction, and Status Damage.
4. **Myth: "Magnetic break only receives Status Damage once."**  
   → **Reality**: The Stage 1 break packet receives `S` once, and the forced Stage 2 Electricity proc inherits that scaled source and receives `S` a second time — giving **`S²` quadratic scaling** on the DoT ticks.
5. **Myth: "Electricity refreshes duration and stun indefinitely like Heat."**  
   → **Reality**: Electricity merges into one tick number, but **every stack expires on its own 6 s timer without refreshing**, and its ~3 s stun **cannot be refreshed while active**.
6. **Myth: "Hitting body after headshots won't affect earlier headshot Electricity (or Gas) stacks."**  
   → **Reality**: Old headshot seeds remain intact, but **the body part of the latest proc immediately rewrites the entire pool's second landing multiplier `L` and headshot-kill classification** (causing "Head→Head→Body" vs. "Body→Head→Head" to differ by a full `1 + Headshot Damage%`).

---

*Published by **WFSim** — feel free to share with attribution intact. Explore exact weapon mechanics and damage simulation at [wfsim.app](https://wfsim.app).*

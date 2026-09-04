# M49 — the Dual Toxocyst computes CO on a flat 75, and Carnage Reign's +33% is GATED, not dead ✅ (owner, 2026-08-16; resolved 2026-08-26)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

Two findings from one session on 毒囊双枪, and each overturns something this
repo had written down. Galvanized Shot throughout (40% a stack per status type).

**THE READINGS, verbatim:**

> 啥没有，125，250
> 原来是75
> 3层是，+120%
> 3层，2debuff，305（+50base）
>
> 实际是，125*（1+2*1.2*（75/（125）） = 305
>
> 3层，2debuff，315 630
> 3层，1debuff，225 450
> 1层，1debuff，165
>
> 135*（1+2*(0.4*3*（75/135）+0.33)） = 315
> 135*（1+1*(0.4*3*（75/135)） = 225
>
> 2*1.53*75/135==1.33
>
> 大概意思是2个evo实际上计算gunCO的时候，都不考虑灵化带来的加成，而且+60的evo
> 的那个33%也完全不生效。我试过卸下gunCO的mod去打带状态的敌人，也没有加成，伤害
> 和原来一摸一样。并且平常和incarnon都是生效的

(The `1.53` in the last line is a slip for `1.2`; the `1.33` it produces is
right. The `+0.33` inside the 315 line is the term being ruled OUT — carried
through it gives 404, and the reading is 315.)

**ONE EXPRESSION FITS ALL FOUR:**

```
damage = panel + 75 x 0.4 x stacks x types
```

| panel | stacks | types | computed | measured |
|---|---|---|---|---|
| 125 (Fevered Frenzy) | 3 | 2 | 125 + 180 = **305** | 305 |
| 135 (Carnage Reign)  | 3 | 1 | 135 + 90 = **225** | 225 |
| 135 | 1 | 1 | 135 + 30 = **165** | 165 |
| 135 | 3 | 2 | 135 + 180 = **315** | 315 |

### 1. The CO base is the unevolved 75 under EITHER tier-2 option

The catalog names only Carnage Reign for this weapon:

```
Dual Toxocyst | Incarnon Mode | Projectile
  Attack Unmodded Damage ......... 75 or 135 (with Evolution II Perk 1)
  Actual CO Bonus at +100% ....... 75
  Notes .......................... CO-bonus does not use base damage increase Evolution
```

and CATALOGS' standing rule is that **ABSENCE MEANS ORDINARY**, so Fevered
Frenzy's +50 was modelled as feeding the CO term in full. It does not, and the
gap is not subtle: a CO term on the full 125 gives `125 + 125 x 2.4 = 425`
against a measured **305**.

**The rule is not repealed.** It has held for every other row and the five
remaining negative controls in
`the_eleven_evolution_exclusion_rows_reproduce_their_own_percentages` still
assert it. What is now known is that the catalog's silence is silence rather
than a statement *here*, and that on this weapon the exclusion belongs to the
WEAPON rather than to one perk. The flag stays per PERK because the Despair
still needs that granularity — one of its two tier-2 options is excluded and
the other measurably is not.

### 2. Carnage Reign's "+33% Direct Damage per Status Type" pays NOTHING

Confirmed two independent ways:

* **In the table above.** The 135 panel reads 225 at one status type and 315 at
  two, which is the expression with no room for a 33% term — carrying it gives
  250 and 364.
* **With the GunCO mod removed entirely.** A status-afflicted enemy takes
  exactly the panel. This clause is then the only CO source on the build, and it
  adds nothing at all.

Modelled as a **LIVE BUG** rather than as a gap: DE's own CO-source list carries
this perk, the card states the clause, and a hotfix restores it. The distinction
is the reader's action — an unmodelled line says wait for us, this one says do
not pick the perk for that half, because nobody here is going to implement what
DE has not shipped. It is the first EVOLUTION to carry one (`live_bug:` beside
the effect it kills, so a two-clause perk can have one working half; the +60
base damage works and is untouched).

**A CANDIDATE CAUSE, UNVERIFIED.** The wiki notes an unlisted requirement of
**energy max ≥ 200** on this clause, recorded in the yaml since the file was
written and never modelled. If it is real and the measuring frame was under it,
the perk is CONDITIONAL rather than broken and the engine should learn the gate
instead of zeroing the clause. Nothing has measured the same weapon on a frame
above 200, which is the one experiment that would tell the two apart — and it is
cheap: same build, same target, a frame with ≥ 200 max energy, read the panel
against a status-afflicted enemy with the GunCO mod off.

### RESOLVED — it was the gate (owner, 2026-08-26)

**The measurements above stand and the conclusion does not.** The unlisted
requirement is real: the clause pays at **energy max ≥ 200**, and it is
CONDITIONAL rather than broken.

Nothing above needs re-reading, because the gate explains it exactly instead of
contradicting it — the neutral Tenno this repo ships has **150** max energy, so
every run that found nothing was made UNDER the threshold. The experiment this
section asked for ("a frame with ≥ 200 max energy") is what the owner supplied.

Modelled as `gated_by_tenno` with `condition: "energy_max >= 200"` and
`grant: condition_overload`, which feeds the same CO term
`innate_co_per_type` does — so the behaviour, the base exclusion and
direct-damage-only all keep coming from where they already came from. Measured
across the threshold on a Dual Toxocyst Incarnon with four status mods:

| max energy | DPS |
| --- | --- |
| 150 (the neutral Tenno) | 33,384 |
| 199 | 33,384 |
| **200** | **53,383** |
| 300 | 53,383 |

The step is at 200 and not 201, which is why the engine gained an
`EnergyMaxAtLeast` gate rather than reusing `EnergyMaxOver(199)`: a threshold
written one off so the operator comes out right reads as a typo and is wrong the
moment a frame lands between the two.

**WHAT THE READER GETS BACK.** `live_bug` said *do not pick this perk for that
half, and a hotfix will change it*; a gate says *it pays on the right frame*,
which is a build decision rather than a warning. Saying the first when the second
is true costs a player 60% of the weapon's damage.

It was also the roster's ONLY evolution live bug, so `check_disclosure` lost its
sample. The three assertions there now run against an INJECTED one, the way
`check_board_link` injects a second mode — the claim is that the machinery can
say it, and that claim should not depend on which perks happen to be broken this
patch.

### What it moves

The first finding still pushes the Dual Toxocyst DOWN and the board rescores it:
the weapon was being credited with a CO term on up to 125/135 of base where the
game computes 75. The second no longer does — the 33%-per-status source exists,
on a frame that can reach 200 energy, and the board's neutral player cannot, so
board rows are unchanged by the correction.

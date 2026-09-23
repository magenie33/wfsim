# Shapley analysis — what each part of a build multiplies the result by

On the page it is **Shapley analysis** / **部件价值分析**, and its id is
`shapley` everywhere else (`/api/shapley`, `wfsim-shapley`, `#shapley-block`).
It is NEVER called a contribution (贡献): that word already means an open-source
contribution, and a share of the damage meter. This measures neither of those.
It asks how many times over a part multiplies the result.

## What it answers

Given the open build and the current fight, and a set of **parts** the reader
chooses, it reports for each part:

| figure | on the page | what it is |
|---|---|---|
| Shapley value φᵢ, shown as e^φᵢ | 等效倍率 / Equivalent multiplier | the part's average marginal effect over every order the chosen parts could be added in |
| leave-one-out | 拿掉 / Take it out | full build against the full build without this part |
| alone | 单装 / Alone | this part alone against none of the chosen parts |
| interaction index Iᵢⱼ | 两两交互 / Pairs | above zero the pair is worth more together (synergy), below zero one dilutes the other |

**The scope is stated on every result:** the parts the reader did not choose
stay on as a fixed background. A multiplier means "with the rest of the build
kept on", and the same part can read differently against another background.

## The value function is ln(metric)

v(S) = ln(mean metric of the build with only subset S of the chosen parts on).
In log space a Shapley value is a MULTIPLIER, so the e^φᵢ multiply out to
full / none exactly (efficiency). The page prints that product next to the
total, and `the_multipliers_multiply_to_full_over_empty` asserts it.

The metric is the SCENARIO's (`rules::metrics`), read as its run series. KPM
reads kill progress, which is above zero whenever any damage lands. A subset
that measures zero has no logarithm, and `/api/shapley` refuses it and names
it. It never prints −∞.

## Exact enumeration, and the run count is the only lever

Every one of the 2ᵏ subsets is simulated. There is no permutation sampling, so
the only error is Monte Carlo noise, and the cost is **2ᵏ × runs per subset**.
The page states that product before anything runs. It is capped at
`MAX_PARTICIPANTS` (`webapi::shapley`, served as
`/api/meta.shapley_max_participants`).

`runs per subset` is a preference of this browser (`wfsim-shapley`, default
10, at least 2). It is kept in no preset, the same as the simulator's and the
optimizer's run counts. The measured cost on a Braton Prime at 180 s is
2–7 ms per run natively, so eight mods at 10 runs is about 15 s of work. A
heavy AoE build in the 361-body ruler costs about twenty times that. The
worker lanes divide both.

## One seed, and a band that knows it

Every subset runs on the same seed (`GAIN_SEED`, the quick calc's), so run r of
every subset draws the same dice and the noise cancels in the differences.
The band on every figure is the delta method on those PAIRED runs: for
Σ c_S ln x̄_S the per-run residual is z_r = Σ c_S x_{S,r}/x̄_S, and the
standard error is sd(z)/√n. A part that scales the fight proportionally
leaves z at zero and its figure is exact. This is the property `gainOver`
states for one comparison. A figure within two bands of zero is drawn dimmed.

## What a part is

A part is anything on the build that has an OFF state:

- **a mod slot** (the Exilus, the stance and a riven included). Taking one out
  leaves the other cards in their order, so the elements recombine exactly as
  the builder would combine them. Cold, Toxin, Heat with the Cold taken out is
  Toxin + Heat, which is Gas. No other rule is needed.
- **an arcane seat**, which becomes `none`.
- **the Incarnon form**, when the build's mode transforms. It is the tier-1
  unlock (`unlock_evo`), and OFF means that evolution is not installed. The
  mode stays what it is: a mode is an APL, and a fight with nothing to
  transform into builds its list without a transmute (`apl::for_fight`), so
  the weapon fires the form the cycle returns to.
- **each later evolution tier**. Taking one out takes out that perk ALONE.

Every subset is sent with `evolutions_as_given: true`, which means the list is
exactly what is installed. The ladder does not trim it (`ladder_prefix`), and
a transforming mode does not imply its unlock. Under the ordinary rules,
taking tier 2 out would take tiers 3 and 4 with it, and the tier-1 unlock
would be put back by the mode. The lower tier would then carry every perk it
opens, and the form would carry nothing. So the page names the unlock itself
in every subset where the form is on. No build, share link or board row
carries this field. A subset with tier 2 empty and tier 3 on cannot be built
in game, but neither can "this mod and no other": both are counterfactuals.

Mode, assembly and valence have no off state, only another choice, so they
are never parts. The wielder and the fight's buffs are not parts yet.

The default choice is every mod. A choice is keyed by POSITION (`mod:3`), so it
survives swapping the card in that slot.

## Where it lives

It is a block of the Simulator tab (`#shapley-block`), shut until opened. It is
a report of the simulator, not a fourth module: it runs `theFight()`, and the
only thing it owns is its run count. Every subset goes through
`/api/simulate` on the page's worker lanes (`gainLanes`), so the simulator is
the truth here as everywhere. `/api/shapley` is arithmetic over the series the
page hands it. A result whose build, fight or choice has moved stays on screen
and says it is stale.

The agent door has `simulator.shapley.parts` and `simulator.shapley.run`.

# M28 — Primary Frostbite stacked off procs that applied no status (2026-08-02)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**The claim** (user): a Cold proc landing on an already-FROZEN target does not
refresh the arcane. Correct, and the repo said so before the code did —
`data/debuffs/frozen.yaml` has carried `refreshable: false` with the note
"cannot be extended: Cold procs are inert" since it was written, and
`apply_cold_proc` has always honoured it for the DEBUFF.

**The bug.** The arcane did not read that answer. The trigger was bumped
BEFORE the proc and unconditionally:

```rust
arc.bump_trigger(&params.arcane.buffs, ArcTrigger::ColdStatus, at);
debuffs.apply_cold_proc(at, sd, ...);          // ← may be inert
```

So an arcane whose card reads "On Cold Status Effect" earned a stack from a
proc that applied no status. `apply_cold_proc` now returns whether a status
LANDED and the bump is gated on it. A capped stack list still counts —
pushing past a cap replaces the oldest, which is an application; only Frozen
returns false.

**OPEN, and it decides most of what Frostbite is worth here.** That last
sentence is an INFERENCE, not a source: nothing says a replace-oldest at the
cap counts as "a Cold Status Effect applied" for an arcane trigger. The
alternative reading is that the trigger needs the stack COUNT to rise, and the
two disagree exactly where it matters most (user, 2026-08-02).

An OVERGUARD holder caps Freeze at 4 and can never be Frozen (sourced —
`data/debuffs/freeze.yaml`, "Bosses and Overguard holders cap at 4 stacks").
So against one, the fix above never fires: Frozen never happens, and under our
reading every Cold proc keeps stacking the arcane forever, pinning it at 40.
Measured — Cernos Prime + Primed Cryo Rounds + a crit set, Thrax Lv 300 SP,
300 s, 40 runs:

| arcane | DPS |
|---|---|
| none | 19,951 |
| Primary Frostbite | 51,684 (**x2.59**) |

Under the other reading the arcane would stall at 4 triggers' worth and then
decay on its 12 s all-drop timer, and that x2.59 largely collapses. On a normal
enemy the two readings barely differ — the 10-stack cap is reached and Frozen
takes over. On an overguard holder they differ by everything, and the roster's
only enemy is an overguard holder.

**The measurement**: in the Simulacrum, on an overguard enemy, apply Cold until
the stack display pins at 4, then keep applying it and watch the Frostbite
counter. Still climbing/refreshing ⇒ this model is right. Stuck ⇒ the trigger
needs the count to rise, and `apply_cold_proc` has to report "the count went
up" rather than "a status landed".

**Measured impact: inside the noise floor.** Cernos Prime + Primed Cryo Rounds
+ Serration + Split Chamber + Point Strike + Vital Sense + Primary Frostbite,
Thrax Steel Path, 300 s, 120 runs, seed 11 (KPM):

| level | before | after |
|---|---|---|
| 300 | 10.32916 | 10.30742 |
| 1000 | 5.62010 | 5.63318 |
| 9999 | 1.81099 | 1.81099 |

±0.2%, and not consistently in one direction — fewer stacks shift kill timing,
which re-aligns the RNG stream. M24 puts a status build's one-scenario spread
far wider than this. Frozen lasts 3 s against a 12 s all-drop buff and takes
nine Cold stacks inside their own 6 s window to reach, so few procs are ever
wasted. Lv 9999 is identical because Frozen is never reached there at all.

This is a CORRECTNESS fix, not a number fix, and it is worth having as one: the
model now says the same thing in both places, and a build that does keep a
target frozen no longer collects an arcane it is not earning.

**Worth knowing while testing Frostbite**: on a weapon with innate Toxin, a
Cold mod does not give you Cold. The Torid + Primed Cryo Rounds is Toxin +
Cold = **Viral**, so there are no Cold procs at all and Frostbite never
triggers — the two runs were byte-identical before this was noticed. It is the
same fact behind Frostbite measuring ~1.1x on the Torid earlier.

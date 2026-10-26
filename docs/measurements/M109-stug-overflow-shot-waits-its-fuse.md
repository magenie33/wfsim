# M109 — a Stug shot carrying more stacks than the cap waits out its fuse (owner, 2026-10-03) ⚠ OPEN

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**THE REPORT**, the owner's, translated from Chinese:

> When multishot is high — above 10 on the base form, above 5 on the Incarnon
> form — one stack does not detonate; it has to wait 1.5 s. Damage is
> swallowed: the explosion's damage is missing. And the stack count should
> follow the same rule as the Grimoire's multishot-buys-more-enemies count.

**WHAT THE WIKI STATES** (Stug page, verbatim):

- "Fires blobs that stick to surfaces or embed into enemies, with a lifespan
  of 1.5 seconds."
- "After reaching 10 stacks or the lifespan running out, the blob will
  detonate and deal 75 - 750 damage in a 0.3 - 2.8 meter radius, scaling with
  the number of stacks."
- "Blobs can be stacked by consecutively shooting one spot, refreshing the
  blob duration with each new stack added." · "Blobs which reach 10 stacks now
  immediately explode."
- "Multishot will not shoot additional blobs, but will instead shoot blobs
  with an equivalent amount of extra stacks. For example, +100% multishot will
  cause each blob shot to have 2 stacks, reaching max stacks after only 5
  consecutive shots."
- Incarnon form: "embedding Corrosive shots that explode after 1.5 seconds or
  reaching max stacks", "can stack up to 5 times before exploding".

**HOW THE REPORT READS AGAINST THEM.** The early detonation fires when a pile
REACHES the cap by stacking. A single shot whose own stacks already exceed the
cap (multishot over 10, or over 5 in Incarnon form) never steps through the
cap, so it does not detonate early; it waits the 1.5 s lifespan, and what is
over the cap is lost. The Grimoire's rule is `floor(3 x multishot)` bodies,
measured (M63) — so the proposal is that a shot's stacks are
`floor(multishot)`, not a roll on the fraction.

**2026-10-26 follow-up:** M127 rejects that proposed floor for base-form Stug:
at multishot 1.6, first damage varies between one and two stacks. The
single-shot overflow report remains open; a finishing shot's excess leaves no
residual pile in M127.
M127's final timing recheck confirms the wiki's 1.5 s fuse in both forms,
superseding the intermediate 2 s report. Adding stacks refreshes expiry without shifting periodic damage;
expiry does not wait for a tick. The two forms' piles never combine.
Single-shot overflow still needs readings.

## Package investigation supplied on 2026-10-26

The owner supplied the report "Stug: high multishot prevents stack-triggered
early detonation", investigating client file version 2026.10.22.13.12.
Its executable SHA-256 is
`eec0dbd7078cd6e1f2402fc953ec2741e87a6fb2658ec288be195aac75a1e76d`.
The report combines package definitions, native disassembly, Lua bytecode and
offline execution of the original native instructions. This session read the
report but did not independently inspect its linked artifacts or rerun it.

The reported mechanism refines the earlier interpretation:

- Spawn initialization sets a capped initial stack count without invoking the
  stack callback. A blob born exactly at its cap therefore also misses early
  detonation; the condition is not limited to a shot exceeding the cap.
- A later collision computes `min(cap, existing + incoming)`. The update
  returns early when the stored count is already at least the new count,
  before invoking the stack callback. A blob born full remains on that path.
- A below-cap blob that merges up to the cap does invoke the callback. Lua
  tests `stacks >= cap` and calls the projectile's death/explosion path.
  Crossing the cap does not skip an equality-only test.
- Incarnon offline examples 3 -> 5 and 4 -> 5 reach early detonation;
  5 -> 5 does not. Ordinary form shares the native class with a cap of ten,
  but the report limits its ordinary-form result to a native control group.
- The native spawn function writes back remaining projectile quantity. The
  report does not establish that all input beyond one blob's initial cap is
  discarded; it does not reproduce the entire emitter loop or fractional
  multishot allocation. This is distinct from M127's observed lack of a
  residual pile when a finishing shot merges into an existing blob.
- Both main projectile definitions specify `EmbedTime={1.5,1.5}` and zero
  direct damage, agreeing with the subsequent in-game timing and entity
  damage observations in M127. Continuous-collision timer changes were not
  dynamically measured by the package investigation.

The current implementation's `incoming_stacks > cap` guard does not express
the reported birth-versus-merge distinction. Changes must preserve the
separate timing and damage observations in M127.

**WHAT THE ENGINE DOES WITH IT.** M127 supplies discrete multishot stacks,
a 1.5 s fuse independent of the attached tick clock, and the damage formulas.
The engine distinguishes birth at the cap (no early detonation) from a growing
merge to the cap (immediate detonation), following the package investigation.
A pile already full stays on its refreshed fuse. Spawn quantities beyond one
full blob remain an admitted emitter limitation. M126 excludes charged
alternate fire; M127 directs assuming reflected child projectiles hit nobody.

**WHAT THE SESSION SETTLES**, and each answer that differs moves a rule above, one session on a Simulacrum target,
damage numbers on, unmodded but for multishot:

1. **The count.** Base form, multishot 1.0 / 1.6 / 2.0 / 2.4: how many stacks
   one shot lays (the explosion's size and radius say it). `floor` means 1.6
   always lays one; a roll means it sometimes lays two.
2. **The overflow.** Multishot 12 on the base form (and 6 in Incarnon form):
   does ONE shot detonate at once or after 1.5 s, and is the number the
   10-stack one (750) or more?
3. **The crossing.** Multishot 4: the third shot takes a pile from 8 to 12 —
   does it detonate at once, and at 10 stacks or at 12?
4. **The explosion.** Its number per stack (75 base / 200 Incarnon per the
   module), its radius per stack, and the falloff the wiki calls a UI bug
   (listed 100%-70%, said to be 100%-30%).
5. **Incarnon bounces.** How many secondary projectiles a pile of n stacks
   releases (the page says 2 to 5) and what each bounce explosion deals.

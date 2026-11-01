# M130 — direct damage quantization and status composition (owner, 2026-10-31)

## Laetum direct damage

Normal form, with the owner explicitly excluding Viral status vulnerability.
The riven's positive Viral value is damage, not a debuff multiplier.
The owner supplied these damage pop-ups:

| Build | Noncritical | Yellow critical |
| --- | ---: | ---: |
| Completely bare | 160 | 353 |
| −99.3% Impact, +96.1% Viral riven | 250 | 551 |
| Same riven, +220% and +165% base damage | 1213 | 2672 |

Laetum has 160 base damage, 64 Impact and 96 Slash. The unmodded quantum is 5.
With the riven, Impact is `64 × 0.007 = 0.448`, quantized to zero; Slash
quantizes to 95 and Viral `160 × 0.961 = 153.76` quantizes to 155, totalling
250. A one-quantum minimum would instead total 255 and does not fit.

With +385% base damage, ModifiedBase is 776 and the quantum is 24.25.
Impact still quantizes to zero, Slash to 460.75 and Viral to 751.75,
totalling 1212.5. The displayed noncritical number rounds to 1213. Apply
the quantized base critical multiplier `2.203663003663004` to 1212.5,
before rounding the critical display to 2672. Using the already rounded
noncritical pop-up as the critical hit's input would produce the wrong value.

The enemy identity and full recording were not supplied. The test uses a
neutral body to reproduce the reported totals; it does not invent a target
or claim a measured change in that target's health.

## Ocucor status presence and the Madurai confound

The owner relayed that a community member recorded Puncture status occurring
with an Ocucor riven carrying −98.2% Puncture. The original video and complete
loadout were not supplied to this session, nor was a measured proc frequency.

Ocucor's base vector is 1 Puncture and 10 Radiation. At −98.2%, Puncture is
0.018, below half its `11/32` quantum, and its direct damage rounds to zero.
Reading the unquantized vector gives Puncture a positive draw weight of
`0.018 / 10.018`, about 0.18% per successful status roll before other mods.
That is a model prediction, not a measured frequency from the video.

On 2026-11-01, the owner identified Madurai in the footage. The school was not
controlled in the original report. Its Phoenix Talons adds physical damage
after physical mods, according to the [Focus calculation notes](https://wiki.warframe.com/w/Focus/Madurai#Phoenix_Talons).
With only Puncture as Ocucor's physical type, that contribution can leave
positive Puncture even after quantization. Madurai's Rain of Arrows also has
forced Puncture procs if active. The footage therefore cannot distinguish
quantized from unquantized status weights.

## Phantasma Prime Focus comparison (owner, 2026-11-01)

The owner relayed a community comparison with a Phantasma Prime riven whose
panel displayed −100% Impact:

| Focus condition | Observation |
| --- | --- |
| Without Madurai | No Impact status observed |
| With Madurai | Impact status observed again |

The hidden precision of the riven, full loadout, sample size and proc counts
were not supplied. A value such as −99.95% is a hypothesis, not a measurement.
This comparison establishes Focus as a relevant control. Absence in a finite
sample does not by itself measure a zero probability or prove a rounding stage.

## Applied rule

The owner requested restoring quantized status weights on 2026-11-01 after
the confound was identified. Random status types use each attack part's
quantized modified composition, including live elements. A component quantized
to zero has no random weight; forced procs remain independent. No minimum
damage component is introduced. The Laetum direct-damage readings above remain
independent evidence and are unchanged.

The complete unquantized vector remains available for the ledger's quantization
breakdown and measured pre-quantization Extra Hit brackets (M40). Retaining that
input does not give it authority over status selection.

The wiki's [status distribution formula](https://wiki.warframe.com/w/Status_Effect)
states `Damage / Total Damage` without specifying the quantization stage.
A [2024 Galatine Prime experiment](https://forums.warframe.com/topic/1403973-when-does-damage-quantization-and-rounding-apply-and-what-parts-are-quantized-and-rounded/)
reported no Impact or Puncture procs under the older 1/16 grid. That old
weapon/setup is not a measurement of current Ocucor proc frequency.

Tests: `fight::tests::status_composition`. Synthetic residuals and increased
status rolls distinguish pre- and post-quantization selection across attack
parts; neither their input values nor counts are additional game measurements.

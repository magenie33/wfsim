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

## Ocucor status presence

The owner relayed that a community member recorded Puncture status occurring
with an Ocucor riven carrying −98.2% Puncture. The original video and complete
loadout were not supplied to this session, nor was a measured proc frequency.

Ocucor's base vector is 1 Puncture and 10 Radiation. At −98.2%, Puncture is
0.018, below half its `11/32` quantum, and its direct damage rounds to zero.
Reading the unquantized vector gives Puncture a positive draw weight of
`0.018 / 10.018`, about 0.18% per successful status roll before other mods.
That is a model prediction, not a measured frequency from the video.

The owner's adopted rule is to retain the unquantized modified composition for
status selection and the quantized composition for direct damage. This rule
applies to all attack parts; it introduces no weapon-specific exception or
minimum damage component. A truly removed component has no random status
weight, and status immunity still excludes a type and renormalizes the rest.

The wiki's [status distribution formula](https://wiki.warframe.com/w/Status_Effect)
states `Damage / Total Damage` without specifying the quantization stage.
A [2024 Galatine Prime experiment](https://forums.warframe.com/topic/1403973-when-does-damage-quantization-and-rounding-apply-and-what-parts-are-quantized-and-rounded/)
reported no Impact or Puncture procs under the older 1/16 grid. That old
weapon/setup is not a measurement of current Ocucor proc frequency.

Tests: `fight::tests::status_composition`. Presence tests deliberately increase
status rolls to expose rare weights; their counts are not game measurements.

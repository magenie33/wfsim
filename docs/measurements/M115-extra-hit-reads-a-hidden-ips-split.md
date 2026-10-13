# M115 — an Extra Hit reads a hidden Impact/Slash/Puncture split on many projectiles ✅ (白苓, 2026-10-13)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**THE READING.** A re-weighing Extra Hit's bracket is `1 + Σ elemental bonuses + Σ
(Impact/Slash/Puncture share × that type's bonus)` (docs/EXTRA_HIT.md). On a
large part of the roster the shares the game reads are NOT the attack's listed
damage. The Akarius Prime's rocket is 100% Impact on its card and reads
10% Impact / 10% Slash / 80% Puncture; most entries read a third of each,
whatever their own split. An Impact, Slash or Puncture mod therefore lifts the
Extra Hit by its bonus times the READ share: Pummel's +120% Impact is +12% on an
Akarius Prime Extra Hit, not +120%.

**THE MEASUREMENT.** 白苓 measured Toxic Lash on 177 weapon attacks, one IPS
mod at a time, and recorded per attack whether the direct hit and the area
part read the listed damage (正常) or a split of their own (异常), and the split
read. The table below is the ranged half of it, as applied.

**WHAT IT SETTLES.** `extra_hit_reads` on the attack (the direct hit) and on its
`radial` (the explosion), `even` or a map of the three shares
(`data/notes.yaml` `extra_hit_reads`). The engine adds
`Σ (read share − the base attack's own share) × that IPS bonus` to the bracket
of an Extra Hit off that part (`ResolvedPanel::extra_hit_reads_shift`). It is a
property of the WEAPON, so every member that re-weighs the bracket reads it; the
sheet measured Toxic Lash, and Xata's Whisper reading the same split is the
category's rule rather than a measurement of its own.

**RESUPPLY DOES NOT RE-WEIGH ANYTHING.** 白苓's second capture, one weapon on a
Heavy Gunner Eximus's head at 100% Strength (base 39.6 Puncture, 2.8 Impact,
3.7 Slash; +90% Puncture; base damage ×2.2):

| instance | predicted | on screen |
| --- | --- | --- |
| the hit, quantized (52 / 2 / 3 steps of 46/32 = 81.9375) | 786.6 | 787 |
| Resupply, `0.25 ×` the hit | 196.65 | 197 |
| Toxic Lash (毒刺), +90% Puncture / +120% Impact / +120% Slash | 144.40 / 227.87 / 180.17 | 145 / 228 / 180 |
| …the setup labelled 冰火电刺, the same three mods | 172.22 / 185.47 / 185.47 | 172 / 186 / 186 |

Resupply is a quarter of the hit as it was dealt, quantized and with no bracket
put back (`copies_the_hit` on `resupply`); the unquantized bracket would pop 196.
Toxic Lash's six numbers share one base of 132.48 and read 60/30/10
Impact/Slash/Puncture in the first setup and a third each in the second, which
is the kind of split the table above records, not the weapon's own 6/8/86.

| weapon | direct hit reads (I / S / P) | explosion reads (I / S / P) |
| --- | --- | --- |
| `acceltra` | even | even |
| `acceltra_prime` | even | even |
| `acrid` | even | — |
| `aegrit` | even | even |
| `afentis` | 0 / 0 / 0 | — |
| `akarius` | 0.1 / 0.1 / 0.8 | 0.1 / 0.1 / 0.8 |
| `akarius_prime` | 0.1 / 0.1 / 0.8 | 0.1 / 0.1 / 0.8 |
| `akbolto` | even | — |
| `akbolto_prime` | even | — |
| `alternox` | 0.75 / 0.1 / 0.15 | — |
| `alternox_alt` | even | even |
| `angstrum` | even | — |
| `astilla` | 0 / 0 / 0 | 0 / 0 / 0 |
| `astilla_prime` | 0 / 0 / 0 | 0 / 0 / 0 |
| `attica` | 0.15 / 0.15 / 0.7 | — |
| `ballistica` | even | — |
| `ballistica_prime` | 0.15 / 0.7 / 0.15 | — |
| `basmu` | 0.35 / 0.3 / 0.35 | 0.35 / 0.3 / 0.35 |
| `boltor_prime` | even | — |
| `bubonico` | 0.4 / 0.275 / 0.275 | — |
| `bubonico_burst` | even | even |
| `buzlok` | 0.8 / 0.05 / 0.15 | — |
| `catabolyst` | 0 / 0 / 0 | — |
| `catchmoon_secondary` | even | — |
| `cernos_prime` | even | — |
| `cinta` | even | — |
| `coda_bassocyst` | 1 / 0 / 0 | — |
| `corinth_airburst` | 0 / 0 / 0 | 0 / 0 / 0 |
| `corinth_prime_airburst` | even | even |
| `cortege_alt` | even | even |
| `cyanex` | even | even |
| `cyanex_burst` | even | — |
| `daikyu` | even | — |
| `daikyu_prime` | even | — |
| `dera` | even | — |
| `dera_vandal` | even | — |
| `despair` | even | — |
| `drakgoon` | even | — |
| `euphona_prime` | 0.1 / 0.1 / 0.8 | — |
| `evensong` | even | even |
| `exergis` | even | — |
| `felarx` | 0.28 / 0.3 / 0.42 | — |
| `felarx_incarnon` | 0 / 0 / 0 | — |
| `ferrox_thrown` | even | — |
| `fluctus` | 0.1 / 0.8 / 0.1 | — |
| `fulmin_prime_semi` | 0.4 / 0.55 / 0.05 | — |
| `fulmin_semi` | 0.4 / 0.55 / 0.05 | — |
| `grattler` | 0.4 / 0.25 / 0.35 | 0.4 / 0.25 / 0.35 |
| `grimoire` | even | even |
| `harpak` | 0.75 / 0.1 / 0.15 | — |
| `harpak_harpoon` | 0 / 0 / 1 | — |
| `hema` | 0.25 / 0.5 / 0.25 | — |
| `higasa` | even | — |
| `hikou_prime` | even | — |
| `javlok` | even | even |
| `javlok_throw` | even | 0.3 / 0.3 / 0.3 |
| `komorex` | 0 / 1 / 0 | 0 / 1 / 0 |
| `kompressa` | even | even |
| `kulstar` | even | even |
| `kunai` | even | — |
| `kuva_ayanga` | 0.3 / 0.3 / 0.3 | 0.3 / 0.3 / 0.3 |
| `kuva_bramma` | even | even |
| `kuva_chakkhurr` | 0.37 / 0.34 / 0.29 | 0.37 / 0.34 / 0.29 |
| `kuva_drakgoon` | even | — |
| `kuva_grattler` | 0.4 / 0.25 / 0.35 | 0.4 / 0.25 / 0.35 |
| `kuva_ogris` | 1 / 0 / 0 | 1 / 0 / 0 |
| `kuva_seer` | even | even |
| `kuva_zarr` | even | even |
| `laetum` | 0.1 / 0.1 / 0.8 | — |
| `laetum_incarnon` | even | even |
| `lanka` | 1 / 0 / 0 | — |
| `larkspur` | even | — |
| `larkspur_prime` | 0.3 / 0.3 / 0.3 | — |
| `lenz` | even | even |
| `mandonel` | 0 / 0 / 1 | — |
| `mara_detron` | 0.5 / 0.25 / 0.25 | — |
| `miter` | 1 / 0 / 0 | — |
| `morgha` | even | even |
| `morgha_alt` | even | 0.1729 / 0.017 / 0.1053 |
| `mutalist_quanta_orb` | — | even |
| `nagantaka` | 0.15 / 0.15 / 0.7 | — |
| `nagantaka_prime` | 0.15 / 0.15 / 0.7 | — |
| `ogris` | 1 / 0 / 0 | 1 / 0 / 0 |
| `panthera_prime` | 1 / 0 / 0 | — |
| `paracyst_harpoon` | even | — |
| `paris` | even | — |
| `paris_prime` | even | — |
| `phantasma_charged` | even | even |
| `phantasma_prime_charged` | even | even |
| `phenmor` | even | — |
| `phenmor_incarnon` | 1 / 0 / 0 | — |
| `plinx` | 0 / 0 / 0.5714 | — |
| `prisma_lenz` | even | even |
| `prisma_tetra` | even | — |
| `proboscis_cernos` | even | even |
| `purgator_1` | 1 / 0 / 0 | — |
| `rakta_ballistica` | even | — |
| `rakta_cernos` | 0 / 0 / 0 | — |
| `riot_848` | 0.25 / 0.5 / 0.25 | — |
| `scourge_prime_thrown` | even | even |
| `scourge_thrown` | even | even |
| `sepulcrum` | even | even |
| `shedu` | 0.4 / 0.25 / 0.35 | 0.4 / 0.25 / 0.35 |
| `simulor` | — | even |
| `spira` | 0.1 / 0.05 / 0.85 | — |
| `sporothrix` | even | even |
| `stug` | even | — |
| `supra` | even | — |
| `supra_vandal` | even | — |
| `synoid_simulor` | — | even |
| `telos_akbolto` | even | — |
| `telos_boltor` | even | — |
| `tenet_detron` | even | — |
| `tenet_diplos` | even | — |
| `tenet_envoy` | even | even |
| `tenet_ferrox_thrown` | even | — |
| `tenet_spirex` | even | even |
| `tenet_tetra` | even | — |
| `tenet_tetra_grenade` | even | even |
| `tetra` | even | — |
| `tombfinger_primary` | 0.5 / 0 / 0.5 | 0.5 / 0 / 0.5 |
| `tombfinger_secondary` | 0 / 0.5 / 0.5 | 0 / 0.5 / 0.5 |
| `trumna_grenade` | 0.4 / 0.55 / 0.05 | 0.4 / 0.55 / 0.05 |
| `trumna_prime_grenade` | 0.4 / 0.55 / 0.05 | 0.4 / 0.55 / 0.05 |
| `velocitus` | 0.36 / 0.225 / 0.315 | — |
| `zakti_prime` | 0.25 / 0.5 / 0.25 | 0.25 / 0.5 / 0.25 |
| `zarr` | even | even |
| `zhuge` | 0.15 / 0.15 / 0.7 | — |
| `zhuge_prime` | 0.15 / 0.15 / 0.7 | 0.15 / 0.15 / 0.7 |
| `zymos` | 0.43 / 0.47 / 0.1 | 0.43 / 0.47 / 0.1 |

**WHAT IT DOES NOT SETTLE**, and is not applied:

- **Melee** (the sheet's 28 melee rows, glaives' thrown and area parts among
  them). The sim does not apply Toxic Lash's melee doubling, and the sheet's
  rows also state a per-weapon base ratio (0.6, 0.3) that needs its own field.
- **A hidden BASE as well as a hidden split.** Ferrox and Tenet Ferrox throws
  read a base of 150 and 200 against the 50 their card lists; Talons reads 250;
  the Komorex scope's +100% damage reads as a +100% Slash mod. None is modelled.
- **Rows that do not say which attack they mean**, or name a form this roster
  does not split out: the Stahlta's and Quellor's area part, the Miter
  uncharged, the Basmu's reload, the Zenith's disc, the Simulors' merge, the Zarr
  barrage, the Grimoire's alt-fire sequence, the Steflos, and two rows that
  state an unknown element share beside the three physical ones.
- **Rows read as 正常** (the Afentis throw, the Higasa's second mode, the
  Euphona Prime's buckshot): nothing to apply.
- **Rows not matched to an entry here** — the Incarnon Genesis adapters, a few
  weapons named by community shorthand, and the two mods the sheet lists.

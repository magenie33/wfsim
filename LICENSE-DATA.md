# Data rights in WFSim

The [`LICENSE`](LICENSE) covers the **code**: AGPL-3.0-or-later. It does not
answer what may be done with the game data the project reads, the measurements
it is calibrated against, the figures its engine produces, or the art it shows
— and those have different owners and different terms.

This file draws that line. Four categories, and a file belongs to exactly one.

## 1. Game data — `data/`, excluding generated outputs

Weapon statistics, mod values, enemy tables, evolutions and the rest are
**facts about a videogame**: a weapon fires at 8.33 rounds per second, a mod
costs 9 capacity at rank 0, an enemy carries a given armour type. WFSim reads
those facts, and the [WARFRAME Wiki](https://wiki.warframe.com/) is where most
of them are checked.

**Facts are not copyrightable.** Copyright reaches expression — the wording of
an article, the way it is written and arranged — not the numbers an article
reports. What this tree holds is the numbers, re-expressed in WFSim's own
schema: its own ids, its own normalization, its own reference graph, its own
sourcing comments. **It reproduces no wiki page, no wiki prose and no wiki
media.** Where a source's wording is load-bearing it is quoted in a comment,
briefly and with attribution, and `build.rs` strips comments before any yaml is
embedded.

**The wiki's terms, recorded.** The wiki declares its content under
**CC BY-NC-SA 3.0** (<https://creativecommons.org/licenses/by-nc-sa/3.0/>) —
NonCommercial and ShareAlike. Those terms govern **the wiki's own expression**,
which this project does not redistribute. They are set down here because a
source is worth crediting, and attribution is given regardless: in the README,
and in the footer of every page of the site.

**The wiki's own statements disagree with each other**, and this project
records the conflict rather than choosing the most convenient reading:

| where | what it says |
| --- | --- |
| site configuration (`api.php`, `rightsinfo`) | `CC BY-NC-SA 3.0` |
| the footer rendered on every article | `CC BY-NC-SA 3.0` |
| `WARFRAME Wiki:Licensing`, prose | "Creative Commons Attribution–Share Alike License (CC BY-SA)" |
| `WARFRAME Wiki:Licensing`, its own links | `creativecommons.org/licenses/by-nc/3.0/` |

Where the wiki's terms are cited, WFSim cites **CC BY-NC-SA 3.0**, the most
restrictive of them: it is what the site configuration declares and what every
page actually carries.

## 2. Measurement baselines — `docs/MEASUREMENTS.md`, `docs/measurements/`

**Original observations, contributed to WFSim.** Someone loaded a specific
build in game, read the numbers off the screen and wrote up what they saw.
Nothing upstream is being copied; this is new work, and it is the part of the
project that cannot be reconstructed from any other source.

Licensed **CC BY-NC-SA 4.0**, attribution to the named contributor and to
WFSim. Each accepted measurement is a numbered `M<n>` entry crediting whoever
ran it, and that number is permanent.

The NonCommercial term matches §3 and is deliberate. Reading a measurement,
reproducing it in game, citing it, arguing with it, building on it — all free,
and the reason for publishing it at all. What it reserves is lifting the
calibration corpus wholesale into a commercial product, which is the one use
that takes the work without doing any of it.

Contributors grant these rights through [`CLA.md`](CLA.md).

## 3. Benchmark outputs — scores, rankings, and any engine figure

KPM scores, leaderboard rankings, optimizer results, simulated damage figures,
and anything else produced by **running the WFSim engine**.

**These are not derived from any upstream source.** They are computed by this
project's own code from its own model of the game. A wiki page states a
weapon's base damage; it does not state what that weapon achieves in a
sixty-second engagement against a level 165 Thrax under a given build. That
figure exists because this engine was written and run, which makes it original
to this project.

Licensed **CC BY-NC-SA 4.0**, attribution to WFSim required.

Quote a number, cite it, link it, read it out in a video, compare your own
build against it — all free, and all welcome. What the NonCommercial term
reserves is redistribution as part of a commercial product or service. **Bulk
scraping of wfsim.app for redistribution is not permitted** in any case.

## 4. Game art — `site/img/`

Weapon and item images are **the copyright of Digital Extremes**. They are not
licensed by this project, are not covered by the AGPL, and are not CC or MIT.
They are reproduced to identify the weapon a reader is configuring.

The wiki files DE media the same way: assets obtained with permission from
Digital Extremes, or reproduced as fair use for informative purposes. Anyone
redistributing this repository should reach their own conclusion about these
files rather than relying on a licence grant, because none is being made.

## WFCD / warframe-items

`vendor/` is **fetched locally and never committed** (see `.gitignore`), so this
repository redistributes no WFCD data. Where their export is consulted, it is
[MIT](https://github.com/WFCD/warframe-items/blob/master/LICENSE) upstream and
stays that way. `site/img/` is sourced through their CDN, but the files
themselves are DE's and belong to §4.

## Summary

| what | where | terms |
| --- | --- | --- |
| code | everything outside the rows below | AGPL-3.0-or-later |
| game data | `data/` | facts; the wiki is credited, and declares CC BY-NC-SA 3.0 for its own content |
| measurements | `docs/MEASUREMENTS.md`, `docs/measurements/` | CC BY-NC-SA 4.0 |
| engine outputs | scores, rankings, simulated figures | CC BY-NC-SA 4.0 |
| game art | `site/img/` | Digital Extremes' — no grant made |

## Commercial licensing

WFSim is published under AGPL-3.0-or-later, and every version published under
it stays available under it. As the copyright holder, the maintainer **reserves
the right to offer WFSim, and components built alongside it, under separate
terms including commercial licences** — the arrangement Qt and MySQL are
released under, and the reason [`CLA.md`](CLA.md) asks contributors for the
right to sublicense.

That reservation covers this project's own work: its code, its engine outputs,
and the contributions licensed to it. It does not extend to Digital Extremes'
art (§4), which is not the maintainer's to relicense — and it has no need to
reach the facts in §1, which are nobody's property to begin with.

The WFSim name and logo are covered by neither the code licence nor this file:
[`TRADEMARK.md`](TRADEMARK.md).

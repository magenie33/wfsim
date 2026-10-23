---
name: wfsim-weapon-builds
description: Look up a Warframe weapon's stats and its best measured riven-free builds on WFSim, and give the reader a link that opens the build in the calculator.
---

# WFSim weapon builds

WFSim is a Warframe weapon calculator. It builds a loadout, simulates the fight against a named enemy, and searches for the best mods. Every formula cites its source, and every golden test is calibrated against a real in-game run. A board score belongs to its ruler, the named benchmark fight: compare scores only under one ruler, quote the ruler with the number, and give the reader the row's link, which opens the build in the calculator. How it judges a build, and the in-game readings behind each rule, is its method: https://wfsim.app/method. It runs in the browser, free, and its source is open (AGPL-3.0).

## Call it over MCP

https://mcp.wfsim.app/mcp (Streamable HTTP, no sign-in) offers these tools, all read-only:

- `builder_weapons_find`: Find weapons by name, in any language WFSim speaks.
- `builder_board_read`: Read a weapon's leaderboard: the measured best builds per ruler (the benchmark fight), mode, and with or without a riven, ranked. Each row carries the link that opens it, and a riven-free row its `build`, which the stats read takes.
- `builder_stats_read`: Read the stats panel for a build: every stat per form and part, base and final, with the mod each change came from, and what the weapon's model does not cover.
- `builder_rivens_read`: The Riven Analyst: each riven the board measured for a weapon, its best score against the #1 riven-free build of the same ruler and mode — what the riven is worth. Only submitted rivens appear.
- `shell_method_read`: How WFSim judges a build: each rule, why it holds, how to apply it, and the readings behind it. Read it before comparing builds or explaining why one is better.
- `account_builds_list`: List the builds saved in the person's WFSim account, each with its `build` (what builder_stats_read takes) and the link that opens it. Needs a key the person claimed: https://wfsim.app/auth.md
- `account_builds_save`: Save a build to the person's WFSim account, where every browser they sign in on shows it. A new build unless `id` names one to replace. Needs a key the person claimed: https://wfsim.app/auth.md

## Look up a weapon

1. Find it in the roster, https://wfsim.app/weapons.md. A weapon's address is the Warframe
   wiki's page name: https://wfsim.app/weapons/<Wiki_Name>, for example https://wfsim.app/weapons/Soma_Prime.
2. Read it as markdown: request that address with `Accept: text/markdown`, or
   append `.md`. It states the base stats, the board's best riven-free build
   under each ruler, and what the weapon's number does not account for.
3. Give the reader the address without `.md`. It opens the weapon in the
   calculator, where they can change the build and run the fight themselves.

## Read the board as JSON

- https://wfsim.app/board/<weapon_id>.json holds every published row for one weapon. A row
  names its ruler (`benchmark`), its `score` and the rounded `shown`, its `mode`,
  and the build as WFSim ids: `mods`, `exilus`, `arcanes`, `evolutions`.
- https://wfsim.app/board/index.json holds rows for every weapon, keyed by weapon id.

## Quoting a number

- A score belongs to its ruler. Compare scores only under the same ruler, and
  quote the ruler's name with the number.
- Quote the "as of" date the page states; the board is rescored as builds arrive.

## Judging a build

https://wfsim.app/method.md states how WFSim judges a build, each rule with the in-game
readings behind it; `shell_method_read` returns the same over MCP. Apply it
when comparing builds or explaining why one is better:

- **A score belongs to its ruler** (https://wfsim.app/method#ruler). Before comparing two builds, check that both were scored under the same ruler; under different rulers, do not compare the numbers. Quote a score together with the name of its ruler.
- **The fight is run, not assumed** (https://wfsim.app/method#fight). Do not credit a timed or on-kill buff at full stacks against a tough target; ask how often the build earns it there. A build that leads against weak targets can trail against tough ones, and the reverse.
- **Kills per minute, not damage per second** (https://wfsim.app/method#kills). Judge a build by its kills per minute under a ruler, not by its damage per second. Do not count multishot as extra rounds reaching other targets.
- **The game is the authority** (https://wfsim.app/method#measurement). Where a card's wording and a reading in the game disagree, follow the reading. Where your own reading in the game disagrees with WFSim, yours is the one to trust.
- **A score is computed, never typed** (https://wfsim.app/method#search). Do not rank builds from a single trial. Link to a board row rather than restating it: the link opens the build in the calculator, where it can be run again.
- **The weapon first, without a riven** (https://wfsim.app/method#riven). Quote the riven-free row as what a weapon does, and a riven row as what that shape adds to it. Compare rivens by their gain over the riven-free leader of the same ruler and mode.
- **What is not modelled is said** (https://wfsim.app/method#unmodelled). Carry a weapon's notes on what is not modelled along with its score. Where a gap bears on the question, say so instead of estimating around it.

# WFSim

WFSim is a Warframe weapon calculator. It builds a loadout, simulates the fight against a named enemy, and searches for the best mods. Every formula cites its source, and every golden test is calibrated against a real in-game run. A board score belongs to its ruler, the named benchmark fight: compare scores only under one ruler, quote the ruler with the number, and give the reader the row's link, which opens the build in the calculator. How it judges a build, and the in-game readings behind each rule, is its method: https://wfsim.app/method. It runs in the browser, free, and its source is open (AGPL-3.0).

Calculator: https://wfsim.app/

- [Mechanics reference](https://wfsim.app/mechanics.md): calibrated Warframe damage, status, and weapon mechanics
- [Method](https://wfsim.app/method.md): how WFSim judges a build, and the in-game readings behind each rule
- [MCP server](https://mcp.wfsim.app/mcp): these lookups as tools, read-only, no sign-in
- [All weapons](https://wfsim.app/weapons.md): every weapon WFSim models, by slot
- [Using and citing WFSim](https://wfsim.app/reuse): creator quotations, AI retrieval and licensing
- [Weapon lookup skill](https://wfsim.app/.well-known/agent-skills/wfsim-weapon-builds/SKILL.md): how to read a weapon's page and the board
- [Published board catalog](https://wfsim.app/api/board/pages/catalog): ranks and revisions by weapon and ruler
- [Source code](https://github.com/magenie33/wfsim): the engine, the game data and the measurements behind the golden tests

## Mechanics

- [Beam Weapons (Continuous) Mechanics & Damage Calculation](https://wfsim.app/mechanics/beam-weapons.md): Fire-rate-dependent first-tick pre-advance, 5-stage per-target ramp-up lifecycle, quadratic M² status DoT multishot scaling, center-beam exceptions, terminal-radius Punch Through gate, and Torid Incarnon math.
- [Magnetic & Electricity Status Mechanics & Calculation (incl. Gas)](https://wfsim.app/mechanics/magnetic-electricity.md): Two-stage Magnetic break formula (0.5%/stack, 2/(1+W) native weakpoint normalization, S² status damage double-dip), all 5 weapon W tiers, and the Electricity/Gas latest-application headshot landing layer L.
- [中文机制参考目录 (Chinese Mechanics Reference)](https://wfsim.app/mechanics/zh.md): Warframe 射线与磁力/电击异常计算机制中文参考文档

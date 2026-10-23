# WFSim

WFSim is a Warframe weapon calculator. It builds a loadout, simulates the fight against a named enemy, and searches for the best mods. Every formula cites its source, and every golden test is calibrated against a real in-game run. A board score belongs to its ruler, the named benchmark fight: compare scores only under one ruler, quote the ruler with the number, and give the reader the row's link, which opens the build in the calculator. How it judges a build, and the in-game readings behind each rule, is its method: https://wfsim.app/method. It runs in the browser, free, and its source is open (AGPL-3.0).

Calculator: https://wfsim.app/

- [Method](https://wfsim.app/method.md): how WFSim judges a build, and the in-game readings behind each rule
- [MCP server](https://mcp.wfsim.app/mcp): these lookups as tools, read-only, no sign-in
- [All weapons](https://wfsim.app/weapons.md): every weapon WFSim models, by slot
- [Weapon lookup skill](https://wfsim.app/.well-known/agent-skills/wfsim-weapon-builds/SKILL.md): how to read a weapon's page and the board
- [Board index](https://wfsim.app/board/index.json): board rows for every weapon, as JSON
- [Source code](https://github.com/magenie33/wfsim): the engine, the game data and the measurements behind the golden tests

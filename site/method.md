# How WFSim judges a build

How WFSim judges a Warframe build: each rule is what the calculator does, why it does it, and the readings it rests on.

## A score belongs to its ruler

**A build is judged in a named fight, its ruler, which states the target, its level, how long the fight lasts and what is counted, and a score compares only with scores under the same ruler.**

### Why

- What a hit is worth depends on what it hits: armour, Overguard, faction, elemental weakness and how many bodies stand in reach each change it. A number without its fight cannot be reproduced or compared.
- A ruler states every term, so anyone can run the same fight and get the same score.
- The best build under one ruler can differ from the best under another; that is the fight answering a different question.

### How to apply it

- Before comparing two builds, check that both were scored under the same ruler; under different rulers, do not compare the numbers.
- Quote a score together with the name of its ruler.

### The readings behind it

- In game · The same Void instance reads 14 on a Thrax Centurion's body and 9 on another unit's body. [M98-void-weakness-overguard-and-thrax-body](https://github.com/magenie33/wfsim/blob/main/docs/measurements/M98-void-weakness-overguard-and-thrax-body.md)

### The rulers

- **Standard Single Target · Thrax Centurion Lv 9999 SP · 180 s · KPM**: Standard means one set of conditions for every build: one tough, unaffiliated target, the same fight and the same rules, so a number reproduces on any machine and any two builds compare directly. Your own conditions are set in the simulator.
- **Standard Multi Target · 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 180 s · KPM**: Standard means one set of conditions for every build: a crowd of tough, unaffiliated targets, packed as close as every elemental area effect still reaches and no larger than holds them whole, the same fight and the same rules, so a number reproduces on any machine and any two builds compare directly. Your own conditions are set in the simulator.
- **Demolisher · one target · Demolisher Devourer Lv 9999 SP · 4-player health · 180 s · KPM**: One specific fight players run, measured as it is rather than as a stand-in for most play: a Demolisher a full squad meets, taken apart by one weapon.
- **Heavy Gunner · one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 180 s · KPM**: One specific fight players run, measured as it is rather than as a stand-in for most play: the Eximus Corrupted Heavy Gunner of the Void Fissures, taken apart by one weapon.

https://wfsim.app/benchmark

## The fight is run, not assumed

**Under a ruler, stacks, buffs, status effects, Overguard and armour start where the game starts them and change hit by hit during the fight; nothing starts at its cap because a card allows one.**

### Why

- A buff that stacks on kills is worth what the fight lets it earn. Against a target that dies twice a minute it is rarely earned; against one that dies every second it is held almost all the time.
- Each hit lands on what the hits before it left: a status already on the target, armour already stripped, a pile about to detonate.
- Only a buff with no timer that nothing spends starts full, because anyone can hold it for the whole fight.

### How to apply it

- Do not credit a timed or on-kill buff at full stacks against a tough target; ask how often the build earns it there.
- A build that leads against weak targets can trail against tough ones, and the reverse.

### The readings behind it

- In the engine · One build scored 60% lower against a level 9999 Steel Path target when its timed buffs started at zero and had to be earned, and 0.9% lower at level 30. [M27-buff-seed](https://github.com/magenie33/wfsim/blob/main/docs/measurements/M27-buff-seed.md)

## Kills per minute, not damage per second

**A ruler counts kills per minute, including the part of a target already gone, so damage that reaches a body already dead, or a pile already full, adds nothing.**

### Why

- Damage per second counts every number a weapon deals, including damage on a body already dead. Kills per minute counts only what removes targets.
- Multishot adds instances to the weapon's own rounds: each its own number, crit and status, landing where the round lands.
- Counting the part of a target already gone lets a build that never finishes a kill still rank.

### How to apply it

- Judge a build by its kills per minute under a ruler, not by its damage per second.
- Do not count multishot as extra rounds reaching other targets.

### The readings behind it

- In game · A three-round burst at a body that dies to one round kills that body, and nothing of the burst reaches the body behind it. [M119-multishot-rides-the-round](https://github.com/magenie33/wfsim/blob/main/docs/measurements/M119-multishot-rides-the-round.md)
- In game · A shot that brings a Blast pile to ten detonates it once, and the rest of that shot's Blast procs are lost. [M108-blast-pile-drops-the-rest-of-its-shot](https://github.com/magenie33/wfsim/blob/main/docs/measurements/M108-blast-pile-drops-the-rest-of-its-shot.md)

## The game is the authority

**Where a reading taken in the game, the wiki and DE's public export disagree, the reading wins, then the wiki, then the export; every formula in the engine names its source.**

### Why

- A card's text and the wiki describe the game, and they can leave out how two effects combine or what a listed number means. A reading in the game settles it.
- Each reading the engine applies is published with its numbers, so anyone can take it again.
- A tested value changes only with a new reading in the game.

### How to apply it

- Where a card's wording and a reading in the game disagree, follow the reading.
- Where your own reading in the game disagrees with WFSim, yours is the one to trust.

### The readings behind it

- In game · Headshot bonuses of +50% and +30% read 1.1998 times the first in game: they add (1.2), they do not multiply (1.3). [M60-headshot-bonuses-add](https://github.com/magenie33/wfsim/blob/main/docs/measurements/M60-headshot-bonuses-add.md)
- In game · A throw reloads after every throw, so its cycle is the wind-up plus the reload, 1.6 s rather than the 1 s its fire rate suggests. [M43-throw-cycle-includes-reload](https://github.com/magenie33/wfsim/blob/main/docs/measurements/M43-throw-cycle-includes-reload.md)

## A score is computed, never typed

**A board build is one anyone submitted or a search found, and its score is the engine's: the ruler's fight run many times under a pinned seed.**

### Why

- Chance decides much of what a build does, so one trial cannot rank it: one run's spread can put a minus sign in front of a status mod worth +30%.
- A pinned seed makes every score reproducible by anyone with the source.
- A submission is a build, never a number, so no score on the board was typed in.

### How to apply it

- Do not rank builds from a single trial.
- Link to a board row rather than restating it: the link opens the build in the calculator, where it can be run again.

### The readings behind it

- In the engine · One run per candidate put a status mod at +30% give or take 39 points; four hundred runs still left it give or take 5.6. [M24-one-run-gain-cannot-rank](https://github.com/magenie33/wfsim/blob/main/docs/measurements/M24-one-run-gain-cannot-rank.md)

## The weapon first, without a riven

**The board leads with the best build that has no riven; a riven is ranked as a shape, which stats it rolled and which is the malus, scored at its best roll beside the riven-free leader.**

### Why

- A riven is one player's random item, and ranking items would rank luck. A shape is something anyone can roll for.
- Every build is scored at full investment: full Forma, every card at max rank, the maximum Valence bonus. The board ranks choices, not how much someone farmed.

### How to apply it

- Quote the riven-free row as what a weapon does, and a riven row as what that shape adds to it.
- Compare rivens by their gain over the riven-free leader of the same ruler and mode.

### Further reading

- [BOARD.md](https://github.com/magenie33/wfsim/blob/main/docs/BOARD.md): How the board stores and ranks a riven shape.

## What is not modelled is said

**A weapon, a mod, an arcane, an evolution and an enemy each state what the engine does not model of them, and a score holds only inside what is modelled.**

### Why

- A number with a gap nobody stated reads as complete. A stated gap says where to check the game yourself.

### How to apply it

- Carry a weapon's notes on what is not modelled along with its score.
- Where a gap bears on the question, say so instead of estimating around it.

### Further reading

- [UNMODELLED.md](https://github.com/magenie33/wfsim/blob/main/docs/UNMODELLED.md): Every mechanic the engine does not model, in one register.


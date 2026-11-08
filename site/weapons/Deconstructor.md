# Deconstructor

Chinese name: 分离

Sentinel Weapon · Sentinel · Mastery Rank 0. 130 base damage (impact 43.3333, puncture 43.3333, slash 43.3333), 0% crit chance, 1x crit multiplier, 25% status chance.

## Not modelled here

- The active-projectile cap, recall and return availability are not simulated; the fire rate is the nominal refire timer.
- It cannot be modded here: it uses melee and thrown melee mods (Pressure Point, Fury, Whirlwind, Power Throw), which are not offered for it. Its base numbers are right; a build is not available.
- The damage type rotation is averaged: the weapon deals 130 of one type at a time, cycling Impact, Puncture, Slash, and this entry splits it evenly. Total damage and status mix are exact; armour is not, because mitigation applies per instance.
- The projectiles bounce off walls like a thrown glaive; there are no walls here.
- The companion fires this weapon: it picks its targets, fires when it decides, and stops while reviving or out of range. Here it fires continuously at one target, so the number is the ceiling.
- This attack's spread is not in the data, so none of its shots miss. At range the number is the ceiling; at point blank it is unaffected.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Deconstructor
- Every published board row, as JSON: https://wfsim.app/board/deconstructor.json

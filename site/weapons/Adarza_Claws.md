# Adarza Claws

Beast Claws · Sentinel · Mastery Rank 0. 90 base damage (puncture 45, slash 45), 30% crit chance, 2.5x crit multiplier, 5% status chance.

## Not modelled here

- Companion melee retains its separate weapon and AI cadence model. Player melee animation parameters do not establish companion target selection, attack completion or retrieval time.
- The complete attack cycle is not calibrated. Simulation estimates the interval from a representative ordinary action for this weapon family.
- Companion movement, bone trajectories, visibility, native attack selection, target scoring and precepts are not simulated. The hit sphere is assumed centered on the selected contact target; range bonuses enlarge its radius, and nearest intersecting bodies fill the target cap. Postures use deterministic target intent.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Adarza_Claws
- Every published board row, as JSON: https://wfsim.app/board/adarza_claws.json

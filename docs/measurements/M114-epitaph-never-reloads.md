# M114 — an Epitaph never reloads ✅ (owner, 2026-10-13)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**THE READING.** Epitaph and Epitaph Prime fire with no reload between shots,
charged or uncharged: a shot draws straight from the reserve, and the
arsenal's 0.6 s reload is never paid. A full-charge shot is the draw plus the
listed rate's interval, `charge + 1 / fire rate` (1.027 s Epitaph, 0.931 s
Prime, unmodded).

**CORROBORATION.** Issue #10 timed three public videos frame by frame off the
HUD ammo counter. The one whose shots are provably fully charged (red crits a
quick shot cannot reach) has 27 intervals, median 0.867 s and minimum 0.700 s,
against 0.736 s for draw-then-rate and 1.333 s with a reload added; its
in-game buff timers put the playback speed at 1.000 ± 0.004. Wiki `Epitaph`:
*"No magazine; does not need to reload"*, *"Epitaph's displayed 0.6s reload
time has no purpose"*.

**WHAT IT SETTLES.** `attack.no_magazine: true` on `epitaph`,
`epitaph_uncharged`, `epitaph_prime` and `epitaph_prime_uncharged`. The draw
still counts (`ChargeCadence::DrawThenRate`), and reload-speed and
magazine mods and every on-reload effect do nothing on the weapon.

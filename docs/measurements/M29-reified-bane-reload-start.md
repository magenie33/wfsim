# M29 — Reified Bane starts at the reload, not at the end of it (2026-08-03)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**The claim** (user, in game): Boar Prime's REIFIED BANE evolution grants its
conditional +14 base damage **the moment an empty reload begins** — "换弹的那一
刻就有了，不需要等待换弹完成". The wiki says the opposite: the bonus is
"applied after finishing a reload while the magazine is empty".

A measurement beats the wiki, so the measurement is what the repo records.

**Why it is not pedantry.** We model this half as permanently HELD — a
`stacking_buff` of one stack, permanent, open from t = 0
(`data/evolutions/boar_prime_reified_bane.yaml`, `EvoBdBuff`). Whether that is
EXACT or an OVERSTATEMENT is decided entirely by this timing:

| reading | the buff during a reload | held is |
|---|---|---|
| measured — up at reload START | up | exact |
| wiki — up at reload END | **down for 2.75 s of every magazine** | too generous, every cycle |

Boar Prime empties 20 rounds and reloads for 2.75 s. Under the wiki's reading
the gap is a real fraction of every cycle and "held for the whole run" would
inflate the build on a schedule. Under the measured one there is no gap at
all: the magazine empties, the reload starts, the buff is already back, and it
then "lasts indefinitely until a manual reload is initiated while the magazine
is not empty" — which the sim never does, because the sim only ever reloads
empty.

**It is the EXCEPTION, and that is the part worth writing down** (user,
2026-08-03). A reload-triggered effect fires on reload COMPLETION by default.
Reified Bane needs BOTH halves of an unusual trigger — the magazine EMPTY, and
the reload's first frame rather than its last — and no other evolution is known
to work this way. So it keeps a narrow variant of its own
(`FlatBaseDamageOnEmptyReload`) instead of becoming a general "on reload" with
a flag: the next reload effect should inherit the default, not this.

**What this constrains.** The day the buff becomes earned rather than granted —
the sim currently cannot express "starts off, turns on at the first reload" —
its trigger fires at reload START. Anything that waits for the reload to
complete is reproducing the wiki's error, and the engine doc comment on
`EvoEffect::FlatBaseDamageOnEmptyReload` says so at the point where it would
be written.

**Also from the same page, and also a correction to what the game shows**: the
in-game card MISPRINTS this half as +10 ("Reload From empty bonus is
incorrectly listed as +10 in game"). The effect is +14, which is the value in
the data.

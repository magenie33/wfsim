# A form is shown under the game's own name

**The rule.** A form entry is displayed under the name the weapon's own page
gives that attack block — "Normal Shot", "Uncharged Shot", "Quick Shot". It is
written per entry as `form_name:`, and `FormKind::label()` is only the fallback.

**Why the fallback is not good enough.** `FormKind::Base.label()` is
`"Base Form"`, which is OUR word: nothing in the game is called that. It reads
as a heading on a weapon with one form and as a wrong name on a weapon with
two, where it is the label telling two modes apart in the builder, in the
optimizer's mode axis and on every board row.

**Where it is filled in.** `ballistica_prime_uncharged` only. Everything else
still falls back, which is the backlog below.

## The backlog

Every entry whose form kind is `base` while the weapon has a second form —
these are the ones where the fallback is both wrong and load-bearing. The
candidate name is DE's own (`vendor/warframe-items`, `attacks[].name`, matched
to our entry by damage + crit + status + charge), and **it is a candidate, not
a source**: the wiki wins, so each one wants its page opened before it is
written in.

| entries | candidate |
| --- | --- |
| `cernos_uncharged`, `corvas_prime_uncharged`, `drakgoon_uncharged`, `dread_uncharged`, `kuva_drakgoon_uncharged`, `mk1_paris_uncharged`, `mutalist_cernos_uncharged`, `paris_uncharged`, `paris_prime_uncharged`, `rakta_cernos_uncharged` | Uncharged Shot |
| `cernos_prime_uncharged` | Uncharged Horizontal/Vertical Shot |
| `epitaph_uncharged`, `epitaph_prime_uncharged` | Uncharged Direct Hit |
| `lanka_uncharged` | Partially Charged Shot |
| `staticor_uncharged` | Uncharged Projectile |
| `daikyu_prime_quick`, `evensong_quick`, `opticor_quick`, `nataruk` | Quick Shot |
| `kuva_quartakk_auto` | Full-Auto |
| `opticor_vandal_quick` | matched "Charged Shot AoE", which is the wrong block — read the page |
| `corvas_uncharged`, `mandonel_uncharged`, `velocitus_uncharged` | DE's export carries no attack block for these Arch-Guns — only the wiki can answer |

## Two traps, both found the expensive way

**A BULK SWEEP OF THE WHOLE ROSTER IS NOT SAFE.** Matching all 566 entries
against DE's export lands 466 of them, and among those are names that would
ship wrong: `opticor_vandal_quick` takes its sibling's AoE block, and
`pandero_alt`, `kuva_kraken_mag_burst`, `tenet_diplos_lock_on` and
`nagantaka_burst` each come back with the SAME name as the form beside them —
DE reuses a name across two attacks where the wiki's page does not. Two modes
sharing a label is the exact bug `check_mode_def` exists to catch, so a sweep
has to be read entry by entry rather than generated.

**THE SINGLE-FORM WEAPONS ARE A SEPARATE DECISION, not a smaller version of
this one.** 240 of them would go from `Base Form` to `Normal Attack`, and the
explosive ones to `Grenade Impact`, `Rocket Impact`, `Cube (direct hit)`. That
is the game's vocabulary and it is also a section heading changing on 240
pages for a weapon that has no second form to be told apart from. It is worth
doing and it is worth deciding on its own.

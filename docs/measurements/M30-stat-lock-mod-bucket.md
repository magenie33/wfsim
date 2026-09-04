# M30 — a stat LOCK stopped at the mod bucket (2026-08-04)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

From "Pistol Acuity 这个计算是不是有问题，应该要锁定的，好像没锁" (user,
2026-08-04). Five mods in the data carry a `disables:` lock, in two families
that say the same sentence:

| mod | card | locks |
|---|---|---|
| Primary Acuity / Pistol Acuity | "Multishot cannot be modified." | `multishot` |
| Semi-Rifle / Semi-Shotgun / Semi-Pistol Cannonade | "Fire Rate cannot be modified." | `fire_rate` |

And both pages state the same rule under it: **"Equipping this mod will set
weapon's <stat> to its default ignoring other bonuses, EVEN NEGATIVE
EFFECTS"** (wiki, Primary_Acuity / Semi-Rifle_Cannonade).

The implementation read that as "zero the MOD bucket". It is not what the
sentence says, and four layers of this model never pass through that bucket:

| source | stat | where it lives |
|---|---|---|
| an evolution's permanent stacks (Fevered Frenzy) | multishot | `WeaponBase::buff_multishot_bonus` |
| Final Fusillade's last-round add | multishot | `WeaponBase::multishot_on_last_round` |
| an arcane's live stacks (Primary Overcharge, Conjunction Voltage) | multishot | added per shot in the sim |
| an evolution's fire-rate bonus | fire rate | `WeaponBase::evo_fire_rate_bonus` |
| the weapon's Frenzy passive (×2.5) | fire rate | the BUFF BAR, in the sim |

All five survived the lock. The largest is the last: Dual Toxocyst + a
Semi-Pistol Cannonade kept Frenzy's ×2.5 cadence, so the sim reported roughly
two and a half times the shots the game can fire — on exactly the build the
Cannonade exists for.

The fix states the rule once and in two halves, because the panel is not the
last word on either stat. `resolve` shadows the out-of-bucket layers it can see
and publishes `ResolvedPanel::locked`; `DummyParams::locks()` is the sim's one
reader for the live ones. A locked row on the panel now says `locked_by` — base
== final with no sources is also what a build that bought nothing looks like,
and the difference is worth a line. Buff CARDS for a locked stat are gone too:
a control that moves no number is worse than no control.

### What this deserves a measurement for

Whether the lock really eats the WEAPON'S OWN PASSIVE. Frenzy is a mod in DE's
data (`/Lotus/…/FireRateOnHeadshotPistolMod`, a `default_upgrade`), so it goes
through the same stat pool the sentence says is ignored — which is why it is
modelled that way here. But "ignoring other bonuses" was written about mods you
choose to equip, and nobody has fired the combination and counted. Dual
Toxocyst + Semi-Pistol Cannonade, headshots, 60 s: the shot count settles it.

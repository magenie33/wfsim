# Companion weapons — legal carriers

A weapon's legal carrier set bounds its wielder. A prototype derives from
that set: first determine every eligible model and variant, then select the
lowest eligible attributes within the chosen scope. A weapon's acquisition
source does not establish its equipment restriction. Weapons with the same
carrier set can share a prototype; the weapon name alone does not require
another prototype.

Carrier eligibility and host mod eligibility are separate checks. Narrowing a
weapon to one eligible host also narrows the host's mod pool. A prototype must
state its carrier scope and the sources of its attributes; a pooled mod list
does not grant every carrier every model's exclusive precepts.

This is the game eligibility reference. The implemented host picker and its
shared stat block are described in [WARFRAMES.md](WARFRAMES.md#companions).
They do not enforce all these restrictions. Beast bodies and Hound weapons are
not implemented. The existing universal robotic floor is not a substitute for
a weapon-specific carrier scope.

## Robotic weapons

The general ranged set has **19 weapons**. Every one can be carried by all
17 Sentinel variants and every MOA model/component configuration. Hounds and
Beasts cannot carry them.

| Weapon | Eligible carriers |
| --- | --- |
| Artax | Sentinels and MOAs |
| Burst Laser | Sentinels and MOAs |
| Burst Laser Prime | Sentinels and MOAs |
| Cryotra | Sentinels and MOAs |
| Deth Machine Rifle | Sentinels and MOAs |
| Deth Machine Rifle Prime | Sentinels and MOAs |
| Helstrum | Sentinels and MOAs |
| Laser Rifle | Sentinels and MOAs |
| Multron | Sentinels and MOAs |
| Prime Laser Rifle | Sentinels and MOAs |
| Prisma Burst Laser | Sentinels and MOAs |
| Stinger | Sentinels and MOAs |
| Sweeper | Sentinels and MOAs |
| Sweeper Prime | Sentinels and MOAs |
| Tazicor | Sentinels and MOAs |
| Verglas | Sentinels and MOAs |
| Verglas Prime | Sentinels and MOAs |
| Vulcax | Sentinels and MOAs |
| Vulklok | Sentinels and MOAs |

The Sentinel set is Carrier, Carrier Prime, Dethcube, Dethcube Prime, Diriga,
Djinn, Helios, Helios Prime, Nautilus, Nautilus Prime, Oxylus, Shade,
Prisma Shade, Shade Prime, Taxon, Wyrm and Wyrm Prime. The MOA models are
Lambeo, Oloro, Para and Nychus; the model that awards a precept does not limit
that precept to that model.

The remaining **five robotic weapons** have narrower scopes:

| Weapon | Eligible carriers | Equipment boundary |
| --- | --- | --- |
| Deconstructor | Helios and Helios Prime | No other Sentinel or MOA; no Hound |
| Deconstructor Prime | Helios and Helios Prime | Prime weapon does not require Prime host |
| Akaten | Every Hound model/configuration | Hec awards it; other Hounds can equip it |
| Batoten | Every Hound model/configuration | Dorma awards it; other Hounds can equip it |
| Lacerten | Every Hound model/configuration | Bhaira awards it; other Hounds can equip it |

Both Deconstructors therefore share the Helios carrier scope. A lowest-stat
representative must be selected inside that pair, rather than from unrelated
Sentinels or MOAs. All three Hound weapons share one Hound scope, including
component variation. They are absent from the weapon roster; the other 21
robotic weapons are present.

Sources: [Sentinel](https://wiki.warframe.com/w/Sentinel),
[MOA](https://wiki.warframe.com/w/MOA_(Companion)),
[Deconstructor Prime](https://wiki.warframe.com/w/Deconstructor_Prime),
[Hound](https://wiki.warframe.com/w/Hound_(Companion)).
DE's [Update 30.5](https://www.warframe.com/en/patch-notes/pc/30-5-0)
documents the Hound acquisition models; acquisition is distinct from the
equipment scope.

## Beast claws

Each of the **17 claw weapons** is fixed to its named breed or Venari variant.
It is not interchangeable across other breeds in the same family. A scope
includes the eligible instances of that breed, including its permitted
variation, rather than one player-owned pet.

| Weapon | Eligible carrier scope | Client weapon record basename |
| --- | --- | --- |
| Adarza Claws | Adarza Kavat | `AdarzaPetWeapon` |
| Chesa Claws | Chesa Kubrow | `ChesaPetWeapon` |
| Crescent Claws | Crescent Vulpaphyla | `CrescentVulpaphylaPetWeapon` |
| Helminth Claws | Helminth Charger | `HelminthPetWeapon` |
| Huras Claws | Huras Kubrow | `HurasPetWeapon` |
| Medjay Claws | Medjay Predasite | `MedjayPredasitePetWeapon` |
| Panzer Claws | Panzer Vulpaphyla | `PanzerVulpaphylaPetWeapon` |
| Pharaoh Claws | Pharaoh Predasite | `PharaohPredasitePetWeapon` |
| Raksa Claws | Raksa Kubrow | `RaksaPetWeapon` |
| Sahasa Claws | Sahasa Kubrow | `SahasaPetWeapon` |
| Sly Claws | Sly Vulpaphyla | `SlyVulpaphylaPetWeapon` |
| Smeeta Claws | Smeeta Kavat | `SmeetaPetWeapon` |
| Sunika Claws | Sunika Kubrow | `SunikaPetWeapon` |
| Vasca Claws | Vasca Kavat | `VascaPetWeapon` |
| Venari Claws | Venari summoned by Khora | `VenariPetWeapon` |
| Venari Prime Claws | Venari Prime summoned by Khora Prime | `VenariPrimePetWeapon` |
| Vizier Claws | Vizier Predasite | `VizierPredasitePetWeapon` |

All weapon records in this table live under
`/Lotus/Types/Friendly/Pets/BeastWeapons/`. Each corresponding body suit fixes
that exact path in `SpecialSlotItem.SPECIAL_B_SLOT.ItemType`. Venari Prime's
weapon inherits from Venari's weapon, but the two bodies bind different paths;
inheritance does not allow either body to exchange its claws for the other's.

Sources: the client's body-to-weapon bindings,
DE's [Update 37](https://www.warframe.com/en/patch-notes/pc/37-0-0), and
[Claws (Beast)](https://wiki.warframe.com/w/Claws_(Beast)). DE specifies a
weapon for each Beast type; the client bindings establish the exact pairs.

## Evidence and prototype boundary

The closed `20261106T150853Z-client-metadata-offline` batch's `data.zip`
contains `packages_effective.jsonl`, `packages_records.jsonl` and
`dict.en.json`. Its manifest SHA-256 is
`dbeb51209c791af1692f91f087826805b8f99198ee0958ec4606170ab2badea2`.
The effective records member SHA-256 is
`feaecf8775e37e9035d2f429e0c87113281ff061a2a4b9271a33af53f432c0b0`.

Public Export supplies identities for the 24 robotic weapons and 34 body suits,
joined by `uniqueName`. It supplies no mechanical attribute in this audit.
The effective records bind all 17 claw paths and carry the robotic bodies'
`IncompatibleWeapons` lists. Sentinel and MOA exclusions name the Hound weapon
base. Only Helios and Helios Prime omit `SentGlaiveWeapon` from their robotic
exclusions. Deconstructor Prime inherits from that excluded Glaive base.
Hound bodies exclude the ranged robotic families and allow all three Hound
weapon variants. The weapon's ancestors must be checked against exclusions;
checking only an exact variant path would admit forbidden Prime weapons.

A prototype cannot use an attribute from outside its weapon's legal set.
Comparing multiple eligible classes also requires a stated comparison rule:
the classes can have lower health and higher shields or armor than each other.
Carrier eligibility alone does not define a total order. Body rank, component
effects and permitted inherited variation must be resolved before claiming a
numerical floor. This reference establishes equipment scope; it supplies no new
combat scalar or default-host selection.

# M127 — Stug's blob damage, 1.5-second fuse, discrete multishot and charge ✅ (owner, 2026-10-26)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

Base-form Stug, Evolution II Overcharge Blast (+300 base damage), against a
fully armor-stripped Corrupted Heavy Gunner. Initial readings have no added
multishot (one innate shot); the second batch has total multishot 1.6.
The owner confirms all damage readings before the Secondary Deadhead batch
were body hits; the Deadhead batch was aimed at the head. Other equipment,
enemy level and frame timings were not specified.

## The readings

| input, no added multishot | observed damage sequence |
|---|---|
| One shot | 304 at contact, 304 about 1 s later, explosion 375 about 1 s after that |
| Three rapid shots | Contact readings 304, 312; explosion 525; two intermediate 312 readings also reported |
| Three slow shots | 304, 308, 312, 312, explosion 525; reported intervals about 1 s |
| Reach ten stacks | Immediate explosion, 1050 |

The owner observes no separate projectile damage when shooting an existing
blob. The blob itself deals damage while attached and on detonation.
The owner further confirms that creating a blob immediately produces its first
damage tick. Subsequent ticks occur once per second on the entity's own clock.
Shots feeding an existing blob only add stacks; they do not produce an extra
damage tick or determine its damage cadence.
Only shots landing on the same small patch combine: an arm can hold three
separate blobs. Detonation leaves no residual blob, including when the final
shot supplies excess multishot stacks.

The owner's final timing recheck confirms a 1.5 s lifespan, agreeing with the
wiki and superseding the earlier 2 s report for both forms. The approximate
intervals in the initial damage sequences above are retained as reported,
not as precise timing measurements. Every added stack restarts the countdown
from 1.5 s; reaching ten stacks instead detonates the
blob immediately. A lone one-stack blob deals its initial attached damage at
t = 0, another tick at t = 1 s, and explodes at t = 1.5 s: two attached
damage instances and one explosion.

Adding stacks refreshes expiry without shifting the existing attached-damage
cadence. With the corrected fuse, a base blob created at t = 0 and fed again
at t = 1.4 s expires at t = 2.9 s. Its tick clock stays at t = 0, 1, 2 s;
the explosion lands at t = 2.9 s, without waiting for t = 3 s or paying a
fractional tick for the remaining 0.9 s. These timestamps illustrate the
confirmed rule, rather than a separately recorded frame-timed sequence.

Base-form and Incarnon blobs are distinct entity types. They never add stacks
to each other, including at the same attachment position.

At total multishot 1.6, the first damage number can be either 304 or 308.
Ten measured shot counts to a full explosion:

```
7 5 6 7 7 6 7 7 5 8
```

### +220% base-damage mod

With the +300 evolution retained, the owner adds a +220% base-damage mod and
reports attached damage / explosion damage:

| reported stack label | attached / explosion | formula before display rounding |
|---|---|---|
| Unlabelled first pair (consistent with one stack) | 973 / 1200 | 304 x 3.2 = 972.8 / 375 x 3.2 = 1200 |
| Two stacks | 986 / 1440 | 308 x 3.2 = 985.6 / 450 x 3.2 = 1440 |
| Unlabelled final pair (consistent with four stacks) | 1011 / 1920 | 316 x 3.2 = 1011.2 / 600 x 3.2 = 1920 |

The first and final stack counts are inferred from the two damage numbers,
not independently supplied labels. The mod's identity was not specified.

### Secondary Deadhead equipped

With the +300 evolution and +220% base-damage mod retained, equipping
Secondary Deadhead and aiming at the head gives **973 / 1560**, attached /
explosion. The prior **body-hit** pair without Deadhead was **973 / 1200**:
attached damage is unchanged and explosion damage gains exactly x1.3. Arcane
rank and active kill-stack count were not specified. The pair remains
consistent with a one-stack blob.

This is consistent with Deadhead's headshot-damage bonus reaching the
head-attached explosion but not the attached tick, without the target's normal
head-location damage multiplier. Hit location and arcane equipment changed
together: head hits without Deadhead and body hits with Deadhead remain
unmeasured controls. This does not yet establish the headshot bonus's gate or
measure the arcane's kill-triggered base-damage stacks.

The owner further confirms that when this explosion qualifies as an area
headshot, nearby targets also receive **1560** explosion damage. The headshot
bonus therefore applies to the explosion event and propagates to surrounding
victims; it is not confined to the body carrying the blob. These equal readings
do not establish radius, falloff or how the initial area-headshot qualification
is decided. Those details remain separate from the confirmed propagation.

### Direct-hit event eligibility and Pistol Acuity

The owner confirms the main blob explosion can trigger weak-point hits and,
when it kills, weak-point kills. It is a special area direct hit for those
conditions. Attached periodic damage cannot satisfy conditions requiring a
direct hit on a particular part or a direct-hit kill. Ordinary kill effects
still apply to attached-damage kills; the owner explicitly clarifies that
these are not suppressed.

With +220% base damage, Secondary Deadhead and Pistol Acuity, the owner reports:

| attached ticks | explosion |
|---|---|
| 973 / 973 | 7802, orange crit |
| 973 / 973 | 4681, yellow crit |

The preceding +300 evolution setup reproduces these exactly. With
`c = round(1.5 * 4095 / 32) * 32 / 4095 = 1.5003663003663004`:

```
explosion before crit  (300 + 75) * 3.2 * 1.3 = 1560
yellow                1560 * (1 + (2*c - 1)) = 4681.142857... -> 4681
orange                1560 * (1 + 2*(2*c - 1)) = 7802.285714... -> 7802
```

This measures the critical-headshot fold-in over a 1x damage location, without
Acuity's weak-point damage addition. The owner confirms Acuity reaches the
explosion, while attached ticks are individually rolled and take no Acuity
bonus. The model applies the card's conditional weak-point crit chance to the
area direct explosion; tier frequency was not measured in this batch. The
two colors above are supplied outcomes, not a measurement of crit chance.

Tests: `m127_explosions_trigger_weakpoint_events_but_attached_ticks_do_not`,
`m127_acuity_explosion_uses_critical_headshots_without_weakpoint_damage`,
`m127_attached_ticks_roll_their_own_critical_hits`.

### Incarnon attached damage and main explosion readings

The owner reports the following with +220% base damage. The numbers are
consistent with retaining Overcharge Blast's +300; the headshot pairs are
consistent with retaining Secondary Deadhead's +30% headshot bonus.

| input | attached / main explosion | formula |
|---|---|---|
| One shot, body | 1120 / 1600 | (300 + 50) x 3.2 / (300 + 200) x 3.2 |
| One shot, head | 1120 / 2080 | attached unchanged / 1600 x 1.3 |
| Two shots, body | 1280 / 2240 | (300 + 2 x 50) x 3.2 / (300 + 2 x 200) x 3.2 |
| Two shots, head | 1280 / 2912 | attached unchanged / 2240 x 1.3 |

The +300 evolution add is applied once to each damage instance, outside the
stack multiplier, in this form as well. The headshot condition changes the
main explosion by x1.3 and leaves attached damage unchanged in both pairs.

The owner confirms that both forms use the same blob model: Incarnon reaches
its five-stack cap and detonates without leaving a residual pile, including
excess stacks from the finishing shot. Its additional secondary projectiles
are a child-projectile stage after the main explosion, not a different stack
or lifespan mechanism.

Evolution IV applies as a weapon-wide stat change to the attack parts. The
owner explicitly directs using that default; it is not a request for separate
per-part eligibility measurements. No new crit or status numerical readings
were supplied in this clarification.

### Galvanized Shot, Cold and Puncture eligibility

The owner equips +220% base damage, +60% Cold and Galvanized Shot, retaining
the preceding +300 evolution setup. Pairs are attached damage / main explosion.

| form | no buff/debuff | active-state label supplied by owner | damage in active state |
|---|---|---|---|
| Base | 1550 / 1913 | 3-2 | 1550 / 1913 |
| Incarnon | 1785 / 2550 | 3-1 | 1785 / 2550 |

The labels refer to three Galvanized Shot buff stacks and two/one target
status types. Its per-status-type damage payload changes neither attack part.
This does not exclude the card's separate status-chance stat.

The Cold element is quantized to 19/32 of modified base, reproducing all four
values: 972.8 x 51/32 = 1550.4; 1200 x 51/32 = 1912.5;
1120 x 51/32 = 1785; 1600 x 51/32 = 2550.

The owner separately confirms that Cold status's critical-damage-received
bonus affects the main explosion but not attached ticks. No critical damage
numbers or Cold stack count were supplied for that qualitative test.
The owner explicitly generalizes the same direct-hit eligibility to Puncture's
critical-chance-received bonus. This is an instructed shared-class rule,
not a separate numerical Puncture measurement.

Stug's GunCO exclusion is a measured result for these forms and damage parts,
not a consequence inferred from an area or direct-hit tag. Other weapons use
their own GunCO rules and catalog entries ([CATALOGS.md](../CATALOGS.md)). The
[wiki catalog](https://wiki.warframe.com/w/Condition_Overload_(Mechanic)) also
lists Stug's Blob Impact and Incarnon Form separately as not applying GunCO.
Main explosions take Cold/Puncture's direct-hit bonuses; attached ticks take
neither. The exact underlying game tag names, and whether range propagation
is a separate component rather than a tag, are not established. GunCO's
failure supplies no evidence for that tag hypothesis.

Tests: `m127_galvanized_shots_damage_payload_is_excluded_from_both_parts`,
`m127_cold_critical_damage_reaches_only_the_main_explosion`,
`m127_puncture_critical_chance_is_gated_by_direct_hit_class`.

### Secondary Enervate event routing

The owner confirms that contact is the attached damage's first tick, not an
additional damage event. The entity ticks immediately on creation; every
tick independently rolls crit and can trigger eligible on-hit effects such
as Enervate. This does not grant the restricted weak-point/direct-hit bonuses
excluded by the earlier measurements.

The owner also confirms that death leaves the blob on the corpse. Its clock
continues, and a replacement enemy in that position takes the remaining
ticks and explosion. A new enemy's own blob remains separate from the old
corpse's blob. The exclusion of charge from a corpse-attached explosion is
still an unmeasured model assumption; preservation of damage does not prove
charge eligibility. Regression:
`m127_dead_hosts_leave_their_blob_for_replacement_enemies`.

The owner reports the local model's ramp growing without resets. Contact has
no separate damage instance: the creation tick earns its one stack, without
an extra contact stack; each later attached tick and main explosion does too.
The owner initially suggests counting the reset per damage number, then
explicitly adopts the [wiki rule](https://wiki.warframe.com/w/Secondary_Enervate)
after checking its simultaneous-hit qualification: an AoE earns one stack;
multiple big crits in one game tick count once towards reset. Gain and reset
each have a 30/s limit. The implementation uses separate minimum-interval
gates; the precise game-tick duration and rolling-window alternative are not
measured here. This is an adopted source rule, not a new numerical in-game
measurement.

Every blob event rolls all victims using the pre-event ramp, then submits one
hit with any victim's tier >=2 qualifying for reset. A non-big first victim
does not suppress a later big victim's reset qualification. Subsequent entity
events read the live ramp even while the weapon reloads. No damage from the
excluded child projectiles is introduced.

Tests: `m127_enervate_resets_on_blob_damage_and_reads_the_live_ramp`,
`m127_enervate_gains_once_per_blob_event_not_contact_or_victim`.

### Incarnon secondary projectile observations

The owner observes secondary projectiles released by an Incarnon detonation.
The supplied wiki excerpt states up to three bounces per projectile, an
explosion of radius 1.5 m on each bounce, a five-stack cap, and projectile
counts of 2 / 2 / 3 / 4 / 5 at 1 / 2 / 3 / 4 / 5 stacks. These numerical
properties are quoted source rules, not new measured damage readings. Its
1.5 s lifespan agrees with the owner's final timing recheck.

The owner's follow-up confirms repeated damage from the bouncing projectiles.
The earlier suggested one-return-hit ceiling is therefore not a measured
restriction on their behavior. Whether each bounce produces one of the
observed repeated hits and how many returns one projectile can make to the same
enemy remain open. The owner subsequently directs assuming all reflected
projectiles fly into the air and hit nobody, in single-target and crowd fights.
Their damage and parameters stay recorded for later spatial modeling. The reflection can miss;
the source's maximum bounce count must not become guaranteed target hits.

With the current +300 evolution and +220% base-damage setup, the owner reports
**1600** secondary-projectile damage, rarely **2080** on a headshot with
Secondary Deadhead. This is exactly x1.3. The body value is consistent with
`(200 + 300) * 3.2`; the source pile's stack count was not specified for this
pair. The rare headshot is a secondary projectile's own observed headshot;
inheritance from the main explosion is not established by these readings.

### Incarnon charge

The owner confirms one charge on a direct hit and one charge on a blob
explosion that hits an enemy. An explosion grants only one total charge,
regardless of the number of enemies it hits. Multishot does not multiply
charge: a shot with multishot 2 still grants only one direct-hit charge.
One charge supplies four Incarnon rounds.

Direct-hit and detonation charges are distinct events. Adding shots to one
pile does not turn its eventual single explosion into multiple explosions
for charging. The owner further confirms that damage from ground blobs never
charges the gauge, even when that damage reaches enemies. The shot must hit
an enemy: direct contact grants one charge and the enemy-attached explosion
grants one. Headshots are not required. Explosions after the attached enemy
dies have not been separately specified.

## What the readings support

- At the measured stack counts, attached damage is consistent with
  `300 + 4 * stacks`; explosion damage is consistent with `300 + 75 * stacks`.
  The +300 evolution add applies once per damage instance, outside the stack
  multiplier: one stack explodes for 375, three for 525, ten for 1050.
- The +300 evolution add is inside the base-damage mod multiplier. Both
  attached and explosion damage follow `(per_stack * stacks + 300) * 3.2`
  under +220% base damage. All three pairs agree after display rounding;
  multiplying only the per-stack component would not reproduce them.
- Multishot 1.6 creates discrete stack counts, consistent with one guaranteed
  stack and a 60% chance of a second. The variable first damage and variable
  shot count reject both fixed floor(1.6) and fixed 1.6 stacks.
  The ten counts average 6.5 shots. Independent 1-or-2-stack rolls at 60%
  predict about 6.483 shots to reach or cross ten; the sample is consistent
  with that rule without independently measuring its exact probability.
- A blob owns its damage clock and local attachment position. A body id alone
  cannot identify the pile a new shot should feed.
- Creation produces the initial attached tick, then the entity ticks every
  second using its current stacks. Adding stacks neither deals a separate hit
  nor starts a new damage-tick clock. Expiry and periodic damage are separate
  clocks; refreshing lifespan does not grant an immediate tick.
- Reaching the cap detonates immediately: ten stacks in base form, five in
  Incarnon. Both forms discard excess finishing-shot stacks with no new pile.
- Both forms' lifespan is 1.5 s. Adding stacks resets expiry independently of
  the existing one-second attached-damage clock. Expiry detonates at its own
  instant, with no delayed or prorated attached tick. If a tick and expiry
  coincide, only the explosion lands.
- A pile's identity includes its firing form as well as owner and local
  attachment position. An Incarnon shot cannot feed a base-form pile, and a
  base-form shot cannot feed an Incarnon pile.

## Still to distinguish

- The combine distance, full per-stack radius and damage falloff, exact
  multishot probability, and single-shot over-cap behavior remain open.
- Secondary projectile counts/radius are supplied wiki rules awaiting their
  own measurements. Repeated damage and secondary headshots (1600 / 2080)
  are observed. Collision-to-damage timing and repeat-hit attribution remain open, as do stack scaling and headshot inheritance
  from the main explosion.

The engine implements the measured attached/main-explosion model, discrete
multishot, independent clocks, both-form isolation, evolution propagation,
head-bracket explosion propagation and direct/explosion charge. Attachment
patches are approximated by body part; spawn-time emitter remainders remain
untraced. Reflected child projectiles receive no damage credit by the owner's
explicit conservative rule.

Tests: `fight::tests::m127_stug`; M109 birth/merge boundaries are covered there
and in `fight::tests::blobs`.

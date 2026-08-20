# -*- coding: utf-8 -*-
"""Write batch 8: nineteen weapons whose two modes include an explosion.

Each weapon is a transform group of two forms. `radial` on a form names the
module attack that is that form's AoE, which the engine carries as `radial:` on
the one attack rather than as a third entry.
"""
import io
import os
import sys

sys.path.insert(0, 'private/scripts')
import wiki_weapons as W  # noqa: E402

MOD = {'primary': W.load('primary'), 'secondary': W.load('secondary')}
for k in MOD:
    MOD[k] = MOD[k].get('Weapons', MOD[k])

POL = {'Madurai': 'madurai', 'Naramon': 'naramon', 'Vazarin': 'vazarin', 'Umbra': 'umbra'}
TRIG = {'Auto': 'auto', 'Semi-Auto': 'semi_auto', 'Semi': 'semi_auto', 'Burst': 'burst',
        'Auto-Spool': 'auto', 'Held': 'held', 'Charge': 'charge', 'Duplex': 'duplex',
        'Mag Burst': 'burst', 'Auto Charge': 'charged_auto', 'Active': 'semi_auto',
        'Auto Burst': 'burst'}
NOT_A_DAMAGE_TYPE = {'Knockdown', 'Ragdoll', 'Stagger'}

E = []


def add(**kw):
    for k in ('base_prose', 'base_reasons', 'alt_prose', 'alt_reasons'):
        kw.setdefault(k, [])
    for k in ('base_top', 'alt_top', 'base_attack', 'alt_attack'):
        kw.setdefault(k, '')
    for k in ('base_trigger', 'alt_trigger', 'base_radial', 'alt_radial'):
        kw.setdefault(k, None)
    E.append(kw)


RIFLE = '[primary, rifle, assault_rifle]'
PLAIN = '[primary, rifle]'
SHOT = '[primary, shotgun]'
PISTOL = '[pistol]'

AOE_R = [('explosion_ignores_cover', {}), ('self_stagger', {})]

add(wid='aeolak', wiki='Aeolak', slot='primary', klass='rifle', pools=RIFLE,
    base_i=0, base_form='base', alt_id='aeolak_alt', alt_form='alt_fire', alt_i=1, alt_radial=2,
    lead="""\
# A DUVIRI RIFLE WITH A GRENADE IN ITS ALTERNATE FIRE: 60 a shot with a metre of
# punch through, and an alt that spends ten rounds on 789 Blast in seven metres.""",
    alt_lead="""\
# THE GRENADE: ten rounds for a 97-damage impact and 789 Blast in a 7 m radius,
# after a 0.3 s charge, with a guaranteed Impact proc.""",
    alt_reasons=AOE_R)

add(wid='ambassador', wiki='Ambassador', slot='primary', klass='rifle', pools=RIFLE,
    base_i=0, base_form='base', alt_id='ambassador_charged', alt_form='charged', alt_i=1,
    alt_radial=2, alt_trigger='charge',
    lead="""\
# AN AUTOMATIC RIFLE WITH A CHARGED SNIPER SHOT IN IT. The automatic fire is
# this entry; the charge is a hit-scan shot with a 6 m explosion behind it.""",
    alt_lead="""\
# THE CHARGED SHOT: a hit-scan round after a one-second charge, with an
# explosion in a six-metre radius.""",
    alt_reasons=AOE_R)

add(wid='battacor', wiki='Battacor', slot='primary', klass='rifle', pools=RIFLE,
    base_i=0, base_form='base', alt_id='battacor_charged', alt_form='charged', alt_i=1,
    alt_radial=2, alt_trigger='charge',
    lead="""\
# A CORPUS BURST RIFLE whose secondary fire charges a hit-scan beam with a 3.4 m
# explosion. The burst is this entry.""",
    alt_lead="""\
# THE SECONDARY FIRE: a 0.4 s charge, hit-scan, with an explosion in 3.4 m.""",
    alt_reasons=AOE_R)

add(wid='cedo', wiki='Cedo', slot='primary', klass='shotgun', pools=SHOT,
    base_i=0, base_form='base', alt_id='cedo_alt', alt_form='alt_fire', alt_i=1, alt_radial=2,
    lead="""\
# A SHOTGUN THAT THROWS A GLAIVE. The primary fire is a hit-scan spread; the
# alternate throws a disc that explodes in six metres.""",
    alt_lead="""\
# THE GLAIVE: a thrown disc with a six-metre explosion behind it.""",
    alt_reasons=AOE_R)

add(wid='cedo_prime', wiki='Cedo Prime', slot='primary', klass='shotgun', pools=SHOT,
    base_i=0, base_form='base', alt_id='cedo_prime_alt', alt_form='alt_fire', alt_i=1, alt_radial=2,
    lead="""\
# THE CEDO WITH MORE OF EVERYTHING and the same glaive in its alternate fire.""",
    alt_lead="""\
# THE GLAIVE: a thrown disc with a six-metre explosion behind it.""",
    alt_reasons=AOE_R)

add(wid='enkaus', wiki='Enkaus', slot='primary', klass='rifle', pools=RIFLE,
    base_i=0, base_form='base', alt_id='enkaus_alt', alt_form='alt_fire', alt_i=1, alt_radial=2,
    lead="""\
# A 36-METRE BEAM with a grenade in its alternate fire — the arsenal shows the
# ALT (`_TooltipAttackDisplay: 2`), and this entry is the beam it is not.""",
    alt_lead="""\
# THE ALTERNATE FIRE: a projectile with an eight-metre explosion behind it —
# the arsenal's own display for this weapon.""",
    alt_reasons=AOE_R)

add(wid='stahlta', wiki='Stahlta', slot='primary', klass='rifle', pools=RIFLE,
    base_i=0, base_form='base', alt_id='stahlta_charged', alt_form='charged', alt_i=1,
    alt_radial=2, alt_trigger='charge',
    lead="""\
# A CORPUS RAILGUN RIFLE: automatic bolts, and a 1.6-second charge that fires a
# lance with a 7.2 m explosion.""",
    alt_lead="""\
# THE CHARGED LANCE: 1.6 seconds of charge for a projectile and a 7.2 m blast.""",
    alt_reasons=AOE_R)

add(wid='mutalist_quanta', wiki='Mutalist Quanta', slot='primary', klass='rifle', pools=RIFLE,
    base_i=0, base_form='base', alt_id='mutalist_quanta_orb', alt_form='alt_fire', alt_i=1,
    alt_radial=2,
    lead="""\
# AN INFESTED ENERGY RIFLE whose alternate fire lays an ORB that explodes. The
# primary fire is this entry.""",
    alt_lead="""\
# THE ORB: a slow projectile that hangs where it lands and explodes in 4.4 m.""",
    alt_prose=["the orb HANGS IN PLACE and damages what walks into it, and the primary fire can be shot THROUGH it for bonus damage (wiki) — a two-step interaction between the two modes, and this engine fires each mode on its own"],
    alt_reasons=AOE_R)

add(wid='efv_8_mars', wiki='EFV-8 Mars', slot='secondary', klass='pistol', pools=PISTOL,
    base_i=0, base_form='base', alt_id='efv_8_mars_alt', alt_form='charged', alt_i=1,
    alt_radial=2,
    lead="""\
# A 1999 PISTOL with a grenade in its alternate fire — a three-metre blast.""",
    alt_lead="""\
# THE GRENADE: a projectile with a three-metre explosion behind it.""",
    alt_reasons=AOE_R)

add(wid='corinth', wiki='Corinth', slot='primary', klass='shotgun', pools=SHOT,
    base_i=0, base_form='base', alt_id='corinth_airburst', alt_form='alt_fire', alt_i=1,
    alt_radial=2, alt_trigger='semi_auto',
    lead="""\
# A PUMP SHOTGUN WITH AN AIR-BURST GRENADE. The buckshot is this entry; the
# alternate lobs a round that detonates in a 9.4 m radius.""",
    alt_lead="""\
# THE AIR BURST: a lobbed round with the widest explosion in the shotgun roster
# — 9.4 metres.""",
    alt_reasons=AOE_R)

add(wid='corinth_prime', wiki='Corinth Prime', slot='primary', klass='shotgun', pools=SHOT,
    base_i=0, base_form='base', alt_id='corinth_prime_airburst', alt_form='alt_fire', alt_i=1,
    alt_radial=2, alt_trigger='semi_auto',
    lead="""\
# THE CORINTH AT A WIDER BLAST (9.8 m) and more of everything else. The buckshot
# is this entry.""",
    alt_lead="""\
# THE AIR BURST: a lobbed round detonating in 9.8 metres.""",
    alt_reasons=AOE_R)

add(wid='epitaph', wiki='Epitaph', slot='secondary', klass='pistol', pools=PISTOL,
    base_i=0, base_form='charged', alt_id='epitaph_uncharged', alt_form='base', alt_i=1,
    alt_radial=2, alt_trigger='semi_auto',
    lead="""\
# A ZARIMAN PISTOL WHOSE CHARGED SHOT IS THE ARSENAL'S OWN — a 0.36 s charge for
# a Cold projectile. The UNCHARGED shot is the one with the explosion.""",
    alt_lead="""\
# THE UNCHARGED SHOT: a smaller projectile with an EIGHT-METRE explosion behind
# it, which the charged shot does not have.""",
    alt_reasons=AOE_R)

add(wid='epitaph_prime', wiki='Epitaph Prime', slot='secondary', klass='pistol', pools=PISTOL,
    base_i=0, base_form='charged', alt_id='epitaph_prime_uncharged', alt_form='base', alt_i=1,
    alt_radial=2, alt_trigger='semi_auto',
    lead="""\
# THE EPITAPH WITH A TEN-METRE UNCHARGED BLAST and more of everything. The
# charged shot is the arsenal's own and is this entry.""",
    alt_lead="""\
# THE UNCHARGED SHOT: a projectile with a ten-metre explosion behind it.""",
    alt_reasons=AOE_R)

add(wid='evensong', wiki='Evensong', slot='primary', klass='bow', pools='[primary, rifle, bow]',
    base_i=1, base_form='charged', alt_id='evensong_quick', alt_form='base', alt_i=0,
    base_radial=2, alt_trigger='semi_auto',
    lead="""\
# A BOW WHOSE DRAWN ARROW EXPLODES — a four-metre blast on the charged shot,
# which is the arsenal's own (`_TooltipAttackDisplay: 2`).""",
    alt_lead="""\
# THE QUICK SHOT: fired without drawing, and with no explosion at all.""",
    base_reasons=AOE_R)

# ---- weapons whose BASE form carries the explosion -------------------------

add(wid='afentis', wiki='Afentis', slot='primary', klass='rifle', pools=PLAIN,
    base_i=0, base_form='base', base_radial=1, alt_id='afentis_throw', alt_form='alt_fire', alt_i=2,
    lead="""\
# A SPEARGUN WHOSE SPEARS EXPLODE: 100 on impact and 800 Blast in three metres.
# The arsenal shows the THROW (`_TooltipAttackDisplay: 2`), which is the other
# entry.""",
    alt_lead="""\
# THE SPEAR THROW: 400 damage at a 30% crit chance and a 3x multiplier, silent,
# and with no explosion — the arsenal's own display for this weapon.""",
    base_prose=["a direct hit forces a KNOCKDOWN and the explosion a RAGDOLL (module ForcedProcs) — neither is a damage type and this arena's enemies are neither knocked down nor thrown"],
    base_reasons=AOE_R)

add(wid='afentis_prime', wiki='Afentis Prime', slot='primary', klass='rifle', pools=PLAIN,
    base_i=0, base_form='base', base_radial=1, alt_id='afentis_prime_throw', alt_form='alt_fire',
    alt_i=2,
    lead="""\
# THE AFENTIS WITH A 5.5-METRE BLAST rather than three, and the lowest riven
# disposition in the primary roster.""",
    alt_lead="""\
# THE SPEAR THROW: the heavy silent throw, with no explosion.""",
    base_prose=["a direct hit forces a KNOCKDOWN and the explosion a RAGDOLL (module ForcedProcs) — neither is a damage type and this arena's enemies are neither knocked down nor thrown"],
    base_reasons=AOE_R)

add(wid='basmu', wiki='Basmu', slot='primary', klass='rifle', pools=RIFLE,
    base_i=0, base_form='base', base_radial=1, alt_id='basmu_beam', alt_form='alt_fire', alt_i=2,
    lead="""\
# A SENTIENT RIFLE whose rounds carry a 1.7 m Electricity blast, and whose
# alternate fire is a held beam.""",
    alt_lead="""\
# THE HELD BEAM: a continuous discharge, and the half of the weapon the primary
# fire's magazine pays for.""",
    base_reasons=AOE_R)

add(wid='cyanex', wiki='Cyanex', slot='secondary', klass='pistol', pools=PISTOL,
    base_i=0, base_form='base', base_radial=1, alt_id='cyanex_burst', alt_form='alt_fire', alt_i=2,
    base_trigger='auto',
    alt_attack='''  # THE BURST COUNT IS THE MAGAZINE, which is the one thing this entry cannot
  # say properly: `count` is a constant and the real number is however many
  # rounds are left. 11 is the UNMODDED magazine — the Tenet Detron's call.
  burst:
    count: 11
    delay_seconds: 0.08     # module BurstDelay''',
    alt_prose=["the burst empties the MAGAZINE and this entry fires a fixed count — the unmodded magazine. The page states that magazine mods lengthen it, so a build carrying one is understated here"],
    lead="""\
# HOMING FLECHETTES WITH A SMALL BLAST — 0.7 metres, the narrowest explosion in
# the roster — and an alternate fire that empties the magazine.""",
    alt_lead="""\
# BURST MODE: the magazine fired in one pull.""",
    base_prose=["the projectiles HOME on enemies (wiki) — this arena's shots already go where they are aimed, so the homing buys nothing here"],
    base_reasons=AOE_R)

add(wid='panthera_prime', wiki='Panthera Prime', slot='primary', klass='rifle', pools=RIFLE,
    base_i=0, base_form='base', base_radial=1, alt_id='panthera_prime_alt', alt_form='alt_fire',
    alt_i=2,
    lead="""\
# THE PANTHERA WITH AN EXPLOSION ON EVERY SAW BLADE — 1.6 metres — and the same
# deployed sawblade in its alternate fire.""",
    alt_lead="""\
# THE DEPLOYED SAWBLADE: six metres out, grinding continuously.""",
    base_prose=["the blades RICOCHET off surfaces (wiki) and this arena has no walls to bounce off"],
    base_reasons=AOE_R,
    alt_prose=["the sawblade is DEPLOYED six metres in front of the wielder and stays there — it is not a beam fired at a target, and this engine models it as one, so a fight where the enemy is not standing on that spot is overstated here",
               "Disarming Purity, the Panthera-exclusive augment — a weapon-exclusive card outside the pools this roster loads"])


def trim(x):
    s = ('%.6f' % float(x)).rstrip('0').rstrip('.')
    return s if s else '0'


def fmt(x, force=False):
    if force and isinstance(x, (int, float)) and float(x) == int(x):
        return '%.1f' % float(x)
    return trim(x)


def dmg_line(dm, indent, ms=1.0):
    total = sum(dm.values())
    tail = ('   # %s a pellet, %s a shot' % (trim(total), trim(total * ms))) if ms > 1 \
        else ('   # total %s' % trim(total))
    return '%sdamage: { %s }%s' % (
        indent, ', '.join('%s: %s' % (k.lower(), trim(v)) for k, v in sorted(dm.items())), tail)


def attack_block(a, rad, extra, trigger_override, fb=None, klass=None):
    fb = fb or {}
    L = ['attack:',
         '  # Cone half-angle from the reticle, degrees — wiki',
         '  # Module:Weapons/data (MinSpread/MaxSpread on this attack). An ATTACK is',
         '  # never inherited, so every form states its own.',
         '  spread:',
         '    min_deg: %s' % fmt(a.get('MinSpread', fb.get('MinSpread', 0.0)), force=True),
         '    max_deg: %s' % fmt(a.get('MaxSpread', fb.get('MaxSpread', 0.0)), force=True)]
    name = (a.get('AttackName') or '').lower()
    if trigger_override:
        trig = trigger_override
    elif a.get('Trigger'):
        trig = TRIG[a['Trigger']]
    elif 'burst' in name:
        trig = 'burst'
    elif 'semi' in name:
        trig = 'semi_auto'
    elif 'charge' in name:
        trig = 'charge'
    elif 'held' in name:
        trig = 'held'
    elif 'auto' in name:
        trig = 'auto'
    else:
        trig = TRIG[fb.get('Trigger') or 'Auto']
    L.append('  trigger: %s' % trig)
    if a.get('BurstCount'):
        L += ['  # THE BURST. The listed `fire_rate` counts BURSTS: one pull is %d rounds'
              % a['BurstCount'], '  # spaced by the delay below.',
              '  burst:', '    count: %d' % a['BurstCount'],
              '    delay_seconds: %s       # module BurstDelay'
              % fmt(a.get('BurstDelay') or 0.0, force=True)]
    shot = a.get('ShotType') or fb.get('ShotType') or 'Hit-Scan'
    L.append('  shot_type: %s' % ('projectile' if shot == 'Projectile' else 'hit_scan'))
    if a.get('ShotSpeed'):
        L.append('  projectile_speed_mps: %s' % fmt(a['ShotSpeed']))
    L.append('  fire_rate: %s' % fmt(a['FireRate']))
    if a.get('ChargeTime') and trig in ('charge', 'charged_auto'):
        L.append('  charge_seconds: %s' % fmt(a['ChargeTime']))
    elif klass == 'bow':
        # A BOW'S CADENCE IS DRAW + NOCK, so the draw is stated even when the
        # module gives none: a 0.0 draw paces on the nock, which is the one
        # arrow a bow's magazine holds.
        L.append('  charge_seconds: %s' % fmt(a.get('ChargeTime') or 0.0))
    L += ['  multishot: %s' % fmt(a.get('Multishot', 1.0), force=True),
          '  ammo_cost: %s' % trim(a.get('AmmoCost', 1)),
          '  crit_chance: %s' % fmt(round(a['CritChance'], 6), force=True),
          '  crit_multiplier: %s' % fmt(a['CritMultiplier'], force=True),
          '  status_chance: %s' % fmt(a['StatusChance'], force=True)]
    if rad:
        L += ['  # AN AoE ATTACK TAKES NO PUNCH THROUGH (MECHANICS §13).', '  punch_through_m: 0.0']
    else:
        L.append('  punch_through_m: %s' % fmt(a.get('PunchThrough') or 0.0, force=True))
    procs = [p for p in (a.get('ForcedProcs') or []) if p not in NOT_A_DAMAGE_TYPE]
    if procs:
        L += ["  # A GUARANTEED %s PROC — the module's ForcedProcs." % procs[0].upper(),
              '  forced_procs: [%s]' % ', '.join(p.lower() for p in procs)]
    if a.get('Range'):
        L += ['  # A WALL, NOT A RAMP: full damage to the end of it and nothing past.',
              '  # wiki: "Range: %s m" on this attack.' % fmt(float(a['Range']), force=True),
              '  range_m: %s' % fmt(float(a['Range']), force=True)]
    else:
        L += ['  # NO RANGE ROW on this attack — the page states none. See',
              '  # data/surveys/weapon_range.yaml.']
    fo = a.get('Falloff')
    if fo and not fo.get('Reduction'):
        # A `Reduction` OF ZERO IS THE MODULE SAYING "NO FALLOFF", not "keeps
        # nothing" — `falloff.reduction` here is the fraction KEPT, so writing a
        # zero would delete the attack past its start range.
        L += ['  # NO DAMAGE FALLOFF — the module carries a window with a reduction of',
              '  # ZERO for this attack, which is its way of saying there is none.']
        fo = None
    if fo:
        L += ['  falloff:',
              '    start_m: %s' % fmt(float(fo['StartRange']), force=True),
              '    end_m: %s' % fmt(float(fo['EndRange']), force=True),
              '    reduction: %s' % fmt(fo['Reduction'], force=True)]
    L.append(dmg_line(a['Damage'], '  ', a.get('Multishot', 1.0)))
    if rad:
        bfo = rad.get('Falloff') or {}
        L += ['  # THE EXPLOSION — rules in MECHANICS §7.',
              '  radial:',
              '    radius_m: %s' % fmt(float(rad['Range']), force=True),
              '    falloff_start_m: %s' % fmt(float(bfo.get('StartRange', 0.0)), force=True),
              '    falloff_reduction: %s%s' % (
                  fmt(bfo.get('Reduction', 0.0), force=True),
                  '   # the module states NO damage falloff' if not bfo.get('Reduction') else ''),
              '    crit_chance: %s' % fmt(round(rad['CritChance'], 6), force=True),
              '    crit_multiplier: %s' % fmt(rad['CritMultiplier'], force=True),
              '    status_chance: %s' % fmt(rad['StatusChance'], force=True),
              '    takes_multishot: false',
              '    takes_blast_radius_mods: true',
              dmg_line(rad['Damage'], '    ')]
    if extra:
        L.append(extra)
    return L


def unmodelled(prose, reasons):
    if not prose and not reasons:
        return []
    L = ['', 'unmodeled:']
    for p in prose:
        L.append('  - "%s"' % p.replace('"', "'"))
    for rid, params in reasons:
        L.append('  - reason: %s' % rid)
        for k, v in params.items():
            L.append('    %s: %s' % (k, v))
    return L


def emit(r):
    d = MOD[r['slot']][r['wiki']]
    base = d['Attacks'][r['base_i']]
    alt = d['Attacks'][r['alt_i']]
    brad = d['Attacks'][r['base_radial']] if r['base_radial'] is not None else None
    arad = d['Attacks'][r['alt_radial']] if r['alt_radial'] is not None else None

    L = ['id: %s' % r['wid'], 'name: %s' % r['wiki'], 'slot: %s' % r['slot'], r['lead'],
         'class: %s' % r['klass'], 'form: %s' % r['base_form'],
         "default_form: true          # the arsenal's form (module _TooltipAttackDisplay)",
         'transform_group: %s' % r['wid']]
    tags = d.get('CompatibilityTags')
    L.append('mod_pools: %s%s' % (
        r['pools'], '   # CompatibilityTags %s' % ', '.join(tags) if tags else ''))
    L += ['mastery_rank: %d' % d['Mastery'], 'max_rank: 30', '',
          '# Weapon-level metadata (wiki Module:Weapons/data/%s, cross-checked' % r['slot'],
          '# against WFCD warframe-items — 0 disagreements).',
          ('accuracy: %s' % fmt(d['Accuracy'], force=True)) if d.get('Accuracy') is not None
          else '# NO ACCURACY ROW — the module states none for this weapon.',
          'disposition: %-14s # riven disposition' % fmt(d['Disposition'])]
    pols = d.get('Polarities') or []
    L.append('polarities: [%s]' % ', '.join(POL[p] for p in pols) if pols
             else 'polarities: []               # the infobox states none')
    L += ['exilus_polarity: %s' % POL[d['ExilusPolarity']] if d.get('ExilusPolarity') in POL
          else '# NO EXILUS POLARITY — the module states none.',
          'riven_family: %s' % (d.get('Family') or r['wiki']),
          'traits: [%s]' % ', '.join(t.lower().replace(' ', '_') for t in (d.get('Traits') or [])),
          'introduced: "%s"' % d['Introduced'],
          'internal_name: %s' % d['InternalName'],
          'noise: %s' % ('silent' if base.get('IsSilent') else 'alarming'),
          'ammo_type: %s' % r['slot'],
          'ammo_max: %d' % d['AmmoMax'], 'ammo_pickup: %d' % d['AmmoPickup'],
          'magazine: %d' % d['Magazine'], 'reload_seconds: %s' % fmt(d['Reload']), '',
          "# Condition Overload: NO row in the wiki's CO catalog (re-read 2026-08-20) —",
          '# ordinary Additive at the full +100%. The catalog gives a class PER ATTACK',
          "# and none of this weapon's is named.",
          'co_behavior: additive_with_base_damage']
    if r['base_top']:
        L += ['', r['base_top']]
    L += ['']
    L += attack_block(base, brad, r['base_attack'], r['base_trigger'], fb=alt, klass=r['klass'])
    L += unmodelled(r['base_prose'], r['base_reasons'])
    L += ['', 'source:', '  url: https://wiki.warframe.com/w/%s' % r['wiki'].replace(' ', '_')]

    A = ['id: %s' % r['alt_id'], 'name: %s' % r['wiki'], r['alt_lead'],
         'inherits: %s' % r['wid'], 'form: %s' % r['alt_form'],
         'transform_group: %s' % r['wid'], '',
         'introduced: "%s"' % d['Introduced'], '']
    if r['alt_top']:
        A += [r['alt_top'], '']
    A += ['# NOT INHERITED — the CO catalog gives a class PER ATTACK, and none of this',
          "# weapon's is named. Absence means ORDINARY.",
          'co_behavior: additive_with_base_damage', '']
    A += attack_block(alt, arad, r['alt_attack'], r['alt_trigger'], fb=base, klass=r['klass'])
    A += unmodelled(r['alt_prose'], r['alt_reasons'])
    A += ['', 'source:', '  url: https://wiki.warframe.com/w/%s' % r['wiki'].replace(' ', '_')]
    return [(r['wid'], '\n'.join(L) + '\n'), (r['alt_id'], '\n'.join(A) + '\n')]


n = 0
for r in E:
    for wid, text in emit(r):
        path = os.path.join('data/weapons', r['slot'], wid + '.yaml')
        if os.path.exists(path):
            raise SystemExit('refusing to overwrite ' + path)
        io.open(path, 'w', encoding='utf-8', newline='\n').write(text)
        n += 1
print('wrote', n, 'entries for', len(E), 'weapons')

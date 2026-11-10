#!/usr/bin/env python3
"""Calibrate registered primary forms against pinned client inputs."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import re
import zipfile

import yaml

from import_client_cadence import blocks, canonical, fields, source

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / 'tests/fixtures/client'
ROSTER = '20261109T053558Z-weapon-roster-dossiers'
OFFLINE = '20261106T150853Z-client-metadata-offline'
PINS = {ROSTER: '7b830c6730e20621029d67d9d26e9aa8a216c998c32de73e2e22fe187c979d91',
    OFFLINE: 'dbeb51209c791af1692f91f087826805b8f99198ee0958ec4606170ab2badea2'}
TYPES = {'DT_IMPACT': 'impact', 'DT_PUNCTURE': 'puncture', 'DT_SLASH': 'slash',
    'DT_FIRE': 'heat', 'DT_FREEZE': 'cold', 'DT_ELECTRICITY': 'electricity',
    'DT_POISON': 'toxin', 'DT_EXPLOSION': 'blast', 'DT_RADIATION': 'radiation',
    'DT_GAS': 'gas', 'DT_MAGNETIC': 'magnetic', 'DT_VIRAL': 'viral',
    'DT_CORROSIVE': 'corrosive', 'DT_RADIANT': 'void', 'DT_SENTIENT': 'tau',
    'DT_FINISHER': 'true'}
# These entries describe a scripted/merged attack, rather than the launch packet.
COMPOSITE = {'buzlok_beacon', 'mutalist_quanta_orb', 'simulor', 'synoid_simulor'}


def inner(raw):
    return fields(raw.strip()[1:-1])


def number(raw, default=0):
    return canonical(raw) if raw is not None else default


def pair(raw):
    return [number(v) for v in raw.strip('{}').split(',')]


def roles(raw):
    parsed = fields(raw)
    return ({kind: inner(next(v for k, v in parsed.items()
        if k.startswith(kind + ':') and not k.endswith(':Type')))
        for kind in ('fire', 'impact', 'state')},
        {k: v for k, v in parsed.items() if k.endswith(':Type')})


def members(objects, path, key):
    merged = {}
    for ancestor in reversed(objects[path]['ancestry_leaf_first']):
        raw = objects[ancestor]['composed_fields'].get(key)
        if raw:
            merged.update(inner(raw.partition('=')[2]))
    return merged


def damage(data, factor=1):
    if not data:
        return {}
    amount = number(data.get('Amount'))
    if data.get('UseNewFormat') == '1':
        vector = {TYPES[k]: number(v) for k, v in data.items() if k in TYPES}
    elif data.get('Type', 'DT_PHYSICAL') == 'DT_PHYSICAL':
        vector = {TYPES[k]: amount * number(v) for k, v in data.items() if k in TYPES}
    else:
        vector = {TYPES[data['Type']]: amount}
    return {k: float(format(v * factor, '.12g')) for k, v in vector.items() if v != 0}


def resolve(objects, token, owner_path, scope):
    token = token.strip('"')
    if not token:
        return None
    for path in (token, owner_path.rsplit('/', 1)[0] + '/' + token):
        if path in objects:
            return path
    matches = [p for p in scope if p.endswith('/' + token)]
    if not matches:
        matches = [p for p in objects if p.endswith('/' + token)]
    assert len(matches) == 1, (owner_path, token, matches)
    return matches[0]


def supplement(batch, extra_tokens=()):
    manifest_raw = (batch / 'manifest.json').read_bytes()
    manifest = json.loads(manifest_raw)
    ready = json.loads((batch / 'READY.json').read_bytes())
    assert ready['manifest_sha256'] == hashlib.sha256(manifest_raw).hexdigest() == PINS[OFFLINE]
    archive = next(v for v in manifest['files'] if v['path'] == 'data.zip')
    archive_path = batch / 'data.zip'
    archive_hash = hashlib.sha256()
    with archive_path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(4 * 1024 * 1024), b''):
            archive_hash.update(chunk)
    assert archive_hash.hexdigest() == archive['sha256']
    assert archive_path.stat().st_size == archive['bytes']
    member = next(v for v in manifest['archive_members']['data.zip']
        if v['path'] == 'packages_effective.jsonl')
    selected, digest, size = [], hashlib.sha256(), 0
    with zipfile.ZipFile(archive_path) as archive_file, archive_file.open(member['path']) as stream:
        for line in stream:
            digest.update(line)
            size += len(line)
            if not any(t in line for t in (b'1999InfShotgunWaveProjectile', b'Modular', b'Kitgun', *extra_tokens)):
                continue
            row = json.loads(line)
            path = row['path']
            if (path.startswith('/Lotus/Weapons/') and (
                    ('Modular' in path or 'Kitgun' in path) and path.endswith('Part')
                    or path.endswith('/1999InfShotgunWaveProjectile'))
                    or path.startswith('/Lotus/Types/') and 'Modular' in path
                    or any(t.decode() in path for t in extra_tokens)):
                selected.append(row)
    assert size == member['bytes'] and digest.hexdigest() == member['sha256']
    return selected, {'batch_id': OFFLINE, 'manifest_sha256': PINS[OFFLINE],
        'archive': archive, 'member': member}


def set_path(text, path, value):
    """Replace one YAML field without rewriting surrounding source commentary."""
    lines = text.splitlines(keepends=True)
    start, end = 0, len(lines)
    for level, key in enumerate(path):
        indent = '  ' * level
        matches = [i for i in range(start, end) if lines[i].startswith(indent + key + ':')]
        assert len(matches) <= 1, path
        if not matches:
            assert level == len(path) - 1, path
            index = end
            old_end = end
        else:
            index = matches[0]
            old_end = next((i for i in range(index + 1, end) if lines[i].strip()
                and not lines[i].lstrip().startswith('#')
                and len(lines[i]) - len(lines[i].lstrip()) <= level * 2), end)
        if level < len(path) - 1:
            assert matches, path
            start, end = index + 1, old_end
            continue
        # Keep preceding/trailing comments outside the replaced value.
        value_end = index + 1
        if matches and not lines[index].partition(':')[2].strip().split('#')[0].strip():
            while value_end < old_end and (not lines[value_end].strip()
                    or len(lines[value_end]) - len(lines[value_end].lstrip()) > level * 2):
                value_end += 1
            while value_end > index + 1 and (not lines[value_end - 1].strip()
                    or lines[value_end - 1].lstrip().startswith('#')):
                value_end -= 1
        if isinstance(value, (dict, list)):
            rendered = indent + key + ':\n' + ''.join(indent + '  ' + line + '\n'
                for line in yaml.safe_dump(value, sort_keys=False, width=10000).rstrip().splitlines())
        else:
            scalar = json.dumps(value) if isinstance(value, (str, bool)) else format(value, '.15g')
            rendered = indent + key + ': ' + scalar + '\n'
        lines[index:value_end if matches else index] = [rendered]
    return ''.join(lines)


def add_reason(text, reason):
    if 'reason: ' + reason in text:
        return text
    if '\nunmodeled:\n' in text:
        return text.replace('\nunmodeled:\n', '\nunmodeled:\n  - reason: ' + reason + '\n', 1)
    return text.rstrip() + '\n\nunmodeled:\n  - reason: ' + reason + '\n'


def adopt(updates, doc, path, value, tolerance=1e-10):
    actual = doc
    for key in path:
        actual = actual.get(key) if isinstance(actual, dict) else None
    def equivalent(a, b):
        if isinstance(a, dict) and isinstance(b, dict):
            return a.keys() == b.keys() and all(equivalent(a[k], b[k]) for k in a)
        if isinstance(a, (float, int)) and isinstance(b, (float, int)):
            return abs(a - b) <= tolerance
        return a == b
    equal = equivalent(actual, value)
    # The expected value comes from the source; equivalent existing decimal spellings stay intact.
    updates['.'.join(path)] = actual if equal else value


def calibrate(objects, binding, dossier, doc):
    owner = objects[dossier['identity']['native_paths'][0]]
    outer = owner['composed_fields']
    identity = binding['id']
    row = {'id': identity, 'owner': owner['path'], 'form': binding['form'], 'applied': {}, 'limits': []}
    row['economy'] = {k: v for k, v in outer.items() if k.startswith('Battery')
        or k in ('ClipIsBattery', 'AmmoClipSize', 'AmmoCapacity', 'MaxCharges')}
    updates, limits = row['applied'], row['limits']
    if binding['behavior'] is None:
        key = binding.get('modular_override', 'PrimaryBehaviorOverrides')
        typed, types = roles(outer[key].partition('=')[2].strip()[1:-1])
        row.update(role_types=types)
        row['primary_override' if key == 'PrimaryBehaviorOverrides' else 'secondary_override'] = typed
        limits.append('client_modular_assembly' if key == 'PrimaryBehaviorOverrides' else 'client_modular_secondary_assembly')
        return row
    raw = list(blocks(outer['Behaviors'].partition('=')[2]))[binding['behavior']]
    typed, types = roles(raw)
    fire, impact, state = (typed[k] for k in ('fire', 'impact', 'state'))
    row.update(behavior_index=binding['behavior'], role_types=types)
    row['state'] = {k: v for k, v in state.items() if any(s in k for s in (
        'Time', 'time', 'Rate', 'Delay', 'Shots', 'Burst', 'Multiplier', 'Reload', 'Charge', 'Prefire'))}
    row['fire'] = {k: v for k, v in fire.items() if k in (
        'fireIterations', 'ammoRequirement', 'UseAmmo', 'ConsumesChargesForFire', 'IgnoreFireIterations',
        'AIMED_ACCURACY', 'HIP_FIRE_ACCURACY', 'SpreadRatioHorizontal', 'spreadUniform',
        'UseFanSpread', 'IgnoreSpreadOnFirstFireIteration', 'AttenuateSpreadForCharge',
        'MinChargeSpreadAtten', 'MaxChargeSpreadAtten', 'UseBeamSpread', 'CentralBeam',
        'FireIterationMode', 'InitialFireIterations', 'MaximumFireIterations', 'ProjectileChargeScale')}
    a = doc['attack']
    path = resolve(objects, fire.get(binding['projectile_key'], '""'), owner['path'], dossier['source_closure'])
    attack = inner(impact.get('AttackData', '{}'))
    crit, cd = number(impact.get('criticalHitChance')), number(impact.get('criticalHitDamageMultiplier'), 1)
    projectile = objects[path]['composed_fields'] if path else {}
    if path:
        proc = attack.get('ProcChance', '0')
        attack = members(objects, path, binding.get('damage_key', 'AttackData'))
        attack.setdefault('ProcChance', proc)
        crit = number(projectile.get('CriticalChance', '').partition('=')[2] or None, crit)
        cd = number(projectile.get('CriticalMultiplier', '').partition('=')[2] or None, cd)
        row['projectile'] = {'path': path, 'key': binding['projectile_key'], 'attack': attack}
    row['attack_data'] = attack
    row['geometry'] = {k: v for k, v in fire.items() if k in (
        'traceDistance', 'Radius', 'TraceRadius', 'tracePunctureDepth', 'InfinitePunctureDepth')}
    consumes_ammo = fire.get('UseAmmo') == '1' and binding['form'] != 'incarnon'
    cost = number(fire.get('ammoRequirement'))
    if consumes_ammo and cost > 0 and state.get('FireIterationMode') != 'INCREMENT':
        adopt(updates, doc, ('attack', 'ammo_cost'), cost)
    elif consumes_ammo and a.get('ammo_cost', 1) > 0 and cost == 0:
        limits.append('client_ammo_runtime')
    if state.get('ReloadAfterEveryShot') == '1' and cost == 0:
        limits.append('client_forced_reload')
    if binding['form'] != 'incarnon' and not a.get('no_magazine') and not a.get('meter'):
        native_clip = number(outer.get('AmmoClipSize', '').partition('=')[2] or None)
        if state.get('ReloadAfterEveryShot') == '1':
            native_clip = 1 if cost > 0 else doc.get('magazine', 1)
        if native_clip > 0:
            adopt(updates, doc, ('magazine',), native_clip)
        ordinary_reserve = outer.get('ClipIsBattery') != 'ClipIsBattery=1'
        ordinary_reserve &= binding.get('economy_kind') != 'health_conversion'
        if ordinary_reserve:
            for native, key in [('AmmoCapacity', 'ammo_max'), ('AmmoPickupCount', 'ammo_pickup')]:
                if native in outer:
                    adopt(updates, doc, (key,), number(outer[native].partition('=')[2]))
    count = number(fire.get('fireIterations'), 1)
    pellets = count
    if fire.get('CentralBeam') == '1' and count > 1:
        pellets += 1
    if state.get('FireIterationMode') == 'INCREMENT':
        pellets = number(state.get('MaximumFireIterations'), count)
        if identity.endswith('_single'):
            pellets = number(state.get('InitialFireIterations'), count)
        elif state.get('InitialFireIterations') is None or number(state['InitialFireIterations']) != pellets:
            limits.append('client_spool_pellets')
    factor = number(fire.get('playerDamageMultiplier'), 1) * number(impact.get('PlayerDamageMultiplier'), 1)
    charged = 'ChargeTime' in state
    if charged:
        if binding['charge_stage'] == 'full':
            maximum = 0 if binding.get('charge_factor') == 'state_damage' else number(state.get('MaxDamageMultiplier'))
            factor *= maximum or number(state.get('DamageMultiplier'), 1)
        elif 'ChargeBeamStateBehavior' in types['state:Type']:
            factor *= number(state.get('DamageMultiplier'), 1) * number(state.get('MinChargeRatio'))
        else:
            factor *= number(state.get('MinDamageMultiplier'), 1)
    row['damage_factor'] = factor
    if identity not in COMPOSITE:
        vector = damage(attack, factor)
        if vector:
            adopt(updates, doc, ('attack', 'damage'), vector, 3e-5)
            if 'co_base_amount' in binding:
                row['co_base_source'] = 'Condition Overload catalog: actual CO damage at +100%'
                adopt(updates, doc, ('co_base_fraction',), binding['co_base_amount'] / sum(vector.values()))
        for key, value in [('crit_chance', crit), ('crit_multiplier', cd),
                ('status_chance', number(attack.get('ProcChance')) / max(1, count if pellets > count else pellets)),
                ('multishot', pellets)]:
            # Incremented pellets share the configured shotgun status budget.
            if key == 'status_chance' and state.get('FireIterationMode') == 'INCREMENT':
                value = number(attack.get('ProcChance')) / max(1, pellets)
            adopt(updates, doc, ('attack', key), value)
    else:
        limits.append('client_composite_attack')
    rate = number(state.get('fireRate')) / 60
    if identity.endswith('_single') and 'ContinuousFireRateAtten' in state:
        rate *= pair(state['ContinuousFireRateAtten'])[0]
    if rate > 0:
        adopt(updates, doc, ('attack', 'fire_rate'), rate)
    elif charged:
        adopt(updates, doc, ('attack', 'charge_cadence'), 'draw_only')
        row['rate_anchor'] = 'positive display/mod anchor; native zero refire adds no interval'
    if 'ContinuousFireRateAtten' in state and number(state.get('ContinuousMaxShots')) > 0:
        endpoints = pair(state['ContinuousFireRateAtten'])
        if endpoints != [1, 1] and not identity.endswith('_single'):
            limits.append('client_rate_spool')
            if a.get('sustained_fire_rate'):
                ramp = dict(a['sustained_fire_rate'])
                ramp['start'], ramp['end'] = (endpoints[::-1] if state.get('ReverseFireRateAtten') == '1' else endpoints)
                adopt(updates, doc, ('attack', 'sustained_fire_rate'), ramp)
    if a.get('burst'):
        count_burst = number(state.get('NumShots'), a['burst']['count'])
        if state.get('BurstRemainingClip') == '1':
            count_burst = doc['magazine']
            limits.append('client_clip_burst')
        adopt(updates, doc, ('attack', 'burst'), {'count': int(count_burst),
            'delay_seconds': number(state.get('BurstDelay'), a['burst']['delay_seconds'])})
    if charged and (binding.get('apply_charge_time') or a.get('charge_seconds') is not None
            or 'ChargeBeamStateBehavior' in types['state:Type']):
        duration = number(state.get('ChargeTime'))
        if binding['charge_stage'] == 'minimum':
            duration *= number(state.get('MinChargeRatio'))
        elif binding['charge_stage'] == 'perfect':
            duration *= pair(fire['PerfectChargeRange'])[0]
        adopt(updates, doc, ('attack', 'charge_seconds'), duration)
    battery = doc.get('battery')
    is_battery = outer.get('ClipIsBattery') == 'ClipIsBattery=1'
    if binding['form'] != 'incarnon' and not doc.get('pseudo_reload'):
        if is_battery and doc.get('class') != 'bow' and not a.get('no_magazine'):
            b = dict(battery or {})
            for native, key in [('BatteryRegenRate', 'regen_per_second'),
                    ('BatteryRegenDelayFromEmpty', 'delay_empty_seconds'),
                    ('BatteryRegenDelay', 'delay_partial_seconds')]:
                if native in outer:
                    b[key] = number(outer[native].partition('=')[2])
                else:
                    if 'client_battery_defaults' not in limits:
                        limits.append('client_battery_defaults')
            if len(b) >= 3 and b.get('regen_per_second', 0) > 0:
                adopt(updates, doc, ('battery',), b)
                duration = b['delay_empty_seconds'] + doc['magazine'] / b['regen_per_second']
                duration = max(duration, b.get('empty_animation_seconds', 0))
                adopt(updates, doc, ('reload_seconds',), duration)
            else:
                limits.append('client_battery_defaults')
        elif state.get('NonLoopingReload') == '0' and 'reloadLoopTime' in state:
            parts = [number(state[k]) for k in ('reloadStartTime', 'reloadLoopTime', 'reloadEndTime')]
            adopt(updates, doc, ('reload_style',), 'by_round')
            for key, value in zip(('reload_start_seconds', 'reload_per_shell_seconds', 'reload_end_seconds'), parts):
                adopt(updates, doc, (key,), value)
            adopt(updates, doc, ('reload_seconds',), parts[0] + doc['magazine'] * parts[1] + parts[2])
        elif not a.get('no_magazine'):
            adopt(updates, doc, ('reload_seconds',), number(state.get('reloadTime')))
    if path and a.get('projectile_speed_mps') is not None:
        speed = number(projectile.get('KinematicInitialSpeed', '').partition('=')[2] or None)
        accel = number(projectile.get('KinematicVelocityAccel', '').partition('=')[2] or None)
        if accel != 0:
            # The current straight-flight model retains its declared constant speed.
            limits.append('client_projectile_acceleration')
            speed = 0
        override = 'ChargedProjectileMaxSpeed' if binding['charge_stage'] == 'full' else 'ChargedProjectileMinSpeed'
        if charged and number(fire.get(override), -1) > 0:
            speed = number(fire[override])
        if speed > 0:
            adopt(updates, doc, ('attack', 'projectile_speed_mps'), speed)
    if path and a.get('radial') and identity not in COMPOSITE:
        radial_key = binding.get('radial_key', 'ExplosiveAttack')
        explosion = members(objects, path, radial_key)
        if radial_key == 'EmbedDeathAttack':
            # The member omits its type; the declared collapse component supplies it.
            explosion.setdefault('Type', binding['radial_type_default'])
        vector = damage(explosion, factor)
        if vector:
            row['projectile']['explosion'] = {'key': radial_key, 'attack': explosion}
            adopt(updates, doc, ('attack', 'radial', 'damage'), vector, 3e-5)
            if 'ProcChance' in explosion:
                adopt(updates, doc, ('attack', 'radial', 'status_chance'), number(explosion['ProcChance']))
            if 'status_duration_extra_seconds' in a['radial']:
                assert 'ProcExtraTime' in explosion, (identity, 'missing status duration input')
                adopt(updates, doc, ('attack', 'radial', 'status_duration_extra_seconds'), number(explosion['ProcExtraTime']))
            for native, key in [('DamageRadius', 'radius_m'), ('ExplosionFallOff', 'falloff_reduction')]:
                if native in projectile:
                    adopt(updates, doc, ('attack', 'radial', key), number(projectile[native].partition('=')[2]))
            if radial_key == 'EmbedDeathAttack' and 'EmbedTime' in projectile:
                times = pair(projectile['EmbedTime'].partition('=')[2])
                assert times[0] == times[1], (identity, times)
                adopt(updates, doc, ('attack', 'radial', 'delay_seconds'), times[0])
    if a.get('radial') and 'RadialDamage' in impact:
        radial = inner(impact['RadialDamage'])
        row['radial_data'] = radial
        vector = damage({'UseNewFormat': '0', 'Type': 'DT_PHYSICAL',
            'Amount': radial['baseAmount'], **inner(radial['DamagePercent'])}, factor)
        if vector:
            adopt(updates, doc, ('attack', 'radial', 'damage'), vector, 3e-5)
            for native, key in [('baseProcChance', 'status_chance'), ('radius', 'radius_m'),
                    ('fallOff', 'falloff_reduction')]:
                adopt(updates, doc, ('attack', 'radial', key), number(radial[native]))
            limits.append('client_radial_critical')
    if path and a.get('embed'):
        for native, key in [('EmbedAttack', 'hit'), ('ExplosiveAttack', 'explosion')]:
            component = members(objects, path, native)
            vector = damage(component, factor)
            if vector:
                row['projectile'][native] = component
                adopt(updates, doc, ('attack', 'embed', key, 'damage'), vector, 3e-5)
    # Falloff floors are absolute unscaled damage; common shot multipliers cancel.
    span_raw = projectile.get('DamageFallOff', '').partition('=')[2] if path else impact.get('damageFallOff')
    floor_raw = projectile.get('DamageFallOffMinDamage', '').partition('=')[2] if path else impact.get('damageFallOffMinDamage')
    if span_raw and floor_raw:
        span, floor = pair(span_raw), number(floor_raw)
        base_amount = math.fsum(damage(attack).values())
        if span[1] > span[0] and base_amount > 0:
            row['falloff_data'] = {'range': span_raw, 'floor': floor_raw, 'base_amount': base_amount}
            reduction = 1 - floor / base_amount
            if 0 <= reduction <= 1:
                adopt(updates, doc, ('attack', 'falloff'), {'start_m': span[0], 'end_m': span[1],
                    'reduction': float(format(reduction, '.12g'))}, 1e-5)
            else:
                limits.append('client_falloff_runtime')
    if fire.get('traceDistance') and number(fire['traceDistance']) > 0:
        if a.get('beam'):
            adopt(updates, doc, ('attack', 'beam', 'range_m'), number(fire['traceDistance']))
            adopt(updates, doc, ('attack', 'beam', 'damage_radius_m'), number(fire.get('Radius')))
            if number(impact.get('NumChains')) > 0:
                for native, key in [('NumChains', 'hops'), ('ChainDistance', 'range_m'),
                        ('ChainDamageMultiplier', 'damage_per_hop')]:
                    value = number(impact[native])
                    adopt(updates, doc, ('attack', 'beam', 'chain', key), int(value) if key == 'hops' else value)
                adopt(updates, doc, ('attack', 'beam', 'chain', 'compounds'), impact.get('SequentialChains') == '1')
        elif not path:
            adopt(updates, doc, ('attack', 'range_m'), number(fire['traceDistance']))
        if binding.get('trace_punch_through') == 'unverified_beam_consumer':
            limits.append('client_beam_trace_punch')
        elif not a.get('radial') and not a.get('lingering') and 'tracePunctureDepth' in fire:
            adopt(updates, doc, ('attack', 'punch_through_m'), number(fire['tracePunctureDepth']))
    if path and any(k in projectile for k in ('InitialWidth', 'SweepRadius', 'KinematicGravityMultiplier')):
        row['projectile']['collision_inputs'] = {k: v for k, v in projectile.items() if k in (
            'InitialWidth', 'InitialRadius', 'SweepRadius', 'KinematicGravityMultiplier')}
        if 'InitialWidth' in projectile or number(projectile.get('SweepRadius', '').partition('=')[2] or None) > 0:
            limits.append('client_projectile_collision')
    if path and a.get('attractor_seconds') is not None:
        script = members(objects, path, 'ScriptInstance')
        attractor = resolve(objects, script['_attractorType'], path, dossier['source_closure'])
        life = objects[attractor]['composed_fields']['LifeSpan']
        row['attractor'] = {'path': attractor, 'LifeSpan': life}
        adopt(updates, doc, ('attack', 'attractor_seconds'), number(life.partition('=')[2]))
    profile = {}
    for aim, key in [('aimed', 'AIMED_ACCURACY'), ('hip', 'HIP_FIRE_ACCURACY')]:
        if fire.get(key) in objects:
            shooting = {}
            for ancestor in reversed(objects[fire[key]]['ancestry_leaf_first']):
                spread = objects[ancestor]['composed_fields'].get('Spread')
                if spread:
                    raw_shooting = inner(spread.partition('=')[2]).get('SHOOTING')
                    if raw_shooting:
                        shooting.update(inner(raw_shooting))
            if 'range' in shooting:
                profile[aim] = {'path': fire[key], 'shooting': shooting, 'range': pair(shooting['range'])}
    if profile:
        row['accuracy'] = profile
        limits.append('client_accuracy_runtime')
        special = fire.get('UseFanSpread') == '1' or fire.get('IgnoreSpreadOnFirstFireIteration') == '1'
        special |= fire.get('AttenuateSpreadForCharge') == '1'
        beam_without_spread = 'Beam' in types['fire:Type'] and fire.get('UseBeamSpread') != '1'
        if special:
            limits.append('client_special_spread')
        elif not beam_without_spread and 'aimed' in profile and 'hip' in profile:
            lo, hi = profile['aimed']['range']
            spread = {'min_deg': lo, 'max_deg': hi, 'sampling': {
                'horizontal_ratio': number(fire.get('SpreadRatioHorizontal'), 0.5),
                'uniform': fire.get('spreadUniform') == '1',
                'hip_min_deg': profile['hip']['range'][0], 'hip_max_deg': profile['hip']['range'][1]}}
            adopt(updates, doc, ('attack', 'spread'), spread)
    if a.get('beam') and 'ContinuousDamageAttenuationTime' in impact:
        ramp = {'rise_seconds': number(impact['ContinuousDamageAttenuationTime']),
            'cooldown_delay_seconds': number(impact.get('ContinuousDamageAttenuationCooldownDelay')),
            'cooldown_divisor': number(impact.get('ContinuousDamageAttenuationCooldownTime'), 1)}
        if ramp['rise_seconds'] > 0 and ramp['cooldown_divisor'] > 0:
            adopt(updates, doc, ('attack', 'beam', 'ramp'), ramp)
            if 'ContinuousDamageAttenuation' in impact:
                adopt(updates, doc, ('beam_ramp_floor',), pair(impact['ContinuousDamageAttenuation'])[0])
            if impact.get('AttenuateContinuousDamagePerTarget') == '1':
                limits.append('client_beam_target_ramp')
    if path and any(k in projectile for k in ('OnProjectileStoppedScript', 'ScriptInstance')):
        # Only nonempty scripts carry a custom callback to review.
        callbacks = {k: v for k, v in projectile.items() if k in ('OnProjectileStoppedScript', 'ScriptInstance')
            and inner(v.partition('=')[2]).get('Script', '""') != '""'}
        if callbacks:
            row['projectile']['callbacks'] = callbacks
            limits.append('client_projectile_callbacks')
    return row


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exchange', type=Path, required=True)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    bindings = json.loads((FIXTURES / 'primary-bindings.json').read_text())['groups']
    batches = args.exchange / 'batches'
    for batch, expected in PINS.items():
        assert hashlib.sha256((batches / batch / 'manifest.json').read_bytes()).hexdigest() == expected
    payload, binding = source(batches / ROSTER, 'weapon-roster-dossiers.zip',
        ['shared/objects.jsonl'] + [f'weapons/{g["group"]}/dossier.json' for g in bindings])
    objects = {o['path']: o for line in payload['shared/objects.jsonl'].splitlines() if (o := json.loads(line))}
    extra, extra_binding = supplement(batches / OFFLINE)
    for obj in extra:
        if obj['path'] not in objects:
            objects[obj['path']] = {'path': obj['path'], 'parent': obj['parent'],
                'composed_fields': obj['fields'], 'ancestry_leaf_first': [obj['path']]}
    # Effective records materialize top-level fields; nested attack members still inherit.
    for obj in extra:
        target = objects[obj['path']]
        if len(target['ancestry_leaf_first']) == 1 and obj['parent'] in objects:
            target['ancestry_leaf_first'] += objects[obj['parent']]['ancestry_leaf_first']
    catalog = {}
    for path in (ROOT / 'data/weapons').rglob('*.yaml'):
        doc = yaml.safe_load(path.read_text())
        if isinstance(doc, dict) and 'id' in doc:
            catalog[doc['id']] = doc
    fixture = {'source': binding, 'supplement_source': extra_binding, 'forms': []}
    changes = {}
    for group in bindings:
        dossier = json.loads(payload[f'weapons/{group["group"]}/dossier.json'])
        assert dossier['identity']['native_paths'] == [group['owner']]
        entries = {e['id']: e for e in dossier['entries']}
        assert entries.keys() == {f['id'] for f in group['forms']}
        for form in group['forms']:
            entry = entries[form['id']]
            assert entry['declared_form'] == form['form']
            doc = dict(catalog[form['id']])
            if doc.get('inherits'):
                doc = {**catalog[doc['inherits']], **doc}
            row = calibrate(objects, form, dossier, doc)
            path = ROOT / entry['yaml_file']
            text = original = path.read_text()
            for key, value in row['applied'].items():
                actual = yaml.safe_load(text)
                for part in key.split('.'):
                    actual = actual.get(part) if isinstance(actual, dict) else None
                if actual != value:
                    text = set_path(text, key.split('.'), value)
            for reason in row['limits']:
                text = add_reason(text, reason)
            if form.get('radial_key') == 'EmbedDeathAttack':
                text = re.sub(r'(?m)^  - "The collapse\x27s 1.3 s delay[^\n]+\n', '', text)
            if 'client_primary_calibration' not in text:
                text = text.rstrip() + '\n\n# see notes: client_primary_calibration\n'
            changes[path] = (original, text)
            fixture['forms'].append(row)
    # A form inherits identical weapon-level timing; only a differing timer is stored.
    by_id = {yaml.safe_load(new)['id']: yaml.safe_load(new) for _, new in changes.values()}
    for path, (old, text) in changes.items():
        doc = yaml.safe_load(text)
        if doc.get('inherits') in by_id:
            for key in ('reload_seconds', 'magazine', 'ammo_max', 'ammo_pickup'):
                if doc.get(key) == by_id[doc['inherits']].get(key):
                    text = re.sub(r'(?m)^' + key + r': [^\n]*\n', '', text)
        changes[path] = old, text
    assert len(fixture['forms']) == 318
    fixture_path = FIXTURES / 'primary-calibration-2026-10-28.json'
    changes[fixture_path] = (fixture_path.read_text() if fixture_path.exists() else '', json.dumps(fixture, indent=2) + '\n')
    different = [str(p.relative_to(ROOT)) for p, (old, new) in changes.items() if old != new]
    if args.check and different:
        raise SystemExit('primary calibration differs: ' + ', '.join(different))
    if not args.check:
        for path, (_, text) in changes.items():
            path.write_text(text)
    print(json.dumps({'groups': len(bindings), 'forms': len(fixture['forms']),
        'adopted_fields': sum(len(r['applied']) for r in fixture['forms']), 'changed_files': len(different),
        'check': args.check}))


if __name__ == '__main__':
    main()

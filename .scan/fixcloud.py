# -*- coding: utf-8 -*-
"""Give the Mutalist Cernos its toxin cloud, and admit the Trumna's alt-fire row."""
import io

# 1. THE CLOUD. The lead comment promised it and no block existed.
p = 'data/weapons/primary/mutalist_cernos.yaml'
s = io.open(p, encoding='utf-8').read()
assert chr(10) + '  lingering:' not in s
anchor = '\n  # PRIMARY COMPRESSION'
assert anchor in s
cloud = '''
  # THE SPORE CLOUD — "on contact with surfaces, arrows spawn a small Toxin
  # cloud that deals damage over 2.5 meters and has a chance to apply a Toxin
  # proc every second for 10 seconds" (wiki), and "initial hit and spore cloud
  # apply status separately".
  #
  # NO DAMAGE FALLOFF: the module carries a window with a reduction of ZERO,
  # which is its way of saying there is none — full damage to the rim.
  #
  # ONE TICK A SECOND for ten seconds, from the module's FireRate of 1.00 on
  # the cloud and the page's ten-second life: ten ticks of 5 Toxin.
  lingering:
    damage: { toxin: 5.0 }
    tick_rate: 1.0
    duration_seconds: 10.0
    radius_m: 2.5
    falloff_start_m: 0.0
    falloff_reduction: 0.0
    crit_chance: 0.15
    crit_multiplier: 2.0
    status_chance: 0.49
    stacking: stack
'''
s = s.replace(anchor, cloud + anchor, 1)
io.open(p, 'w', encoding='utf-8', newline='').write(s)
print('cloud added to mutalist_cernos')

# 2. THE TRUMNA'S ALT-FIRE ROW, which the vocabulary cannot hold.
NOTE = ('  - "PRIMARY COMPRESSION\'s row for this attack is not transcribed. The published table '
        'reads \'127% / Both / Merged\' — the effectiveness is over 100%, the stacking is BOTH '
        'brackets at once, and the radius calculation is a fourth kind (\'Alt-Fire gains the '
        'damage % bonus from primary fire\'s radius and a unique multiplier\'). `stacking` here is '
        'one of two values and `radius_calculation` one of four, so a shape the vocabulary cannot '
        'hold is left out rather than flattened — and the arcane is therefore worth NOTHING on this '
        'form here, where in game it is worth more than on the primary fire"')
for wid in ('trumna_grenade', 'trumna_prime_grenade'):
    q = 'data/weapons/primary/%s.yaml' % wid
    t = io.open(q, encoding='utf-8').read()
    assert 'unmodeled:' in t, wid
    t = t.replace('unmodeled:\n', 'unmodeled:\n' + NOTE + '\n', 1)
    io.open(q, 'w', encoding='utf-8', newline='').write(t)
    print('admission added to', wid)

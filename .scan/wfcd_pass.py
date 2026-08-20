# -*- coding: utf-8 -*-
"""Append the WFCD pass to scripts/audit_weapon_stats.py."""
import io

p = 'scripts/audit_weapon_stats.py'
s = io.open(p, encoding='utf-8').read()

DOC_OLD = '''An entry with no matching attack at all is the finding this exists for.
'''
DOC_NEW = '''An entry with no matching attack at all is the finding this exists for.

THE SECOND PASS IS WFCD, which is what every weapon yaml's header claims
("cross-checked against WFCD warframe-items"). It is the CROSS-CHECK and not a
peer: where the two sources disagree, THE WIKI WINS (data/README.md), so a
disagreement here is recorded rather than fixed — but it has to be recorded,
because a header that claims a check nobody ran is worse than no header.

WFCD carries three quantities under names that look like ours and are not:
`magazineSize` is in SHOTS where ours is in ROUNDS (the Panthera spends 2 a
shot, so 60 rounds read as 30), `reloadTime` is sometimes the PARTIAL reload,
and `omegaAttenuation`/`masteryReq` are an older snapshot than the wiki's.
Every one of those is in `EXPECTED_WFCD` with its reason.
'''
assert DOC_OLD in s
s = s.replace(DOC_OLD, DOC_NEW, 1)

TAIL_OLD = '''if __name__ == '__main__':
    raise SystemExit(main(set(sys.argv[1:])))'''

TAIL_NEW = '''# THE SECOND SOURCE, and where it is allowed to differ. Each entry names the
# QUANTITY that differs, not just the weapon — a divergence with no reason is
# indistinguishable from a transcription error.
EXPECTED_WFCD = {
    # `magazineSize` IS IN SHOTS. Ours is in ROUNDS, which is the wiki module's
    # own quantity; divide by the attack's `ammo_cost` and the two agree.
    ('angstrum', 'magazine'): '3 rounds = 1 charged shot',
    ('prisma_angstrum', 'magazine'): '3 rounds = 1 charged shot',
    ('fulmin', 'magazine'): '60 rounds = 6 semi-auto shots at 10 apiece',
    ('fulmin_prime', 'magazine'): '80 rounds = 8 semi-auto shots at 10 apiece',
    ('panthera', 'magazine'): '60 rounds = 30 shots at 2 apiece',
    ('panthera_prime', 'magazine'): '80 rounds = 40 shots at 2 apiece',
    ('staticor', 'magazine'): '48 rounds = 12 charged throws at 4 apiece',
    ('twin_grakatas', 'magazine'): '120 rounds = 60 shots at 2 apiece',
    # `reloadTime` IS SOMETIMES THE PARTIAL ONE. The Basmu's page says it
    # outright — "a 2 second reload animation from empty… if there are still
    # rounds left, there is a delay of 0.x" — and this sim always empties the
    # magazine, so the FULL reload is the one that applies.
    ('basmu', 'reload_seconds'): "WFCD holds the PARTIAL reload; ours is the wiki's full one",
    ('shedu', 'reload_seconds'): "WFCD holds the PARTIAL reload; ours is the wiki's full one",
    ('nataruk', 'reload_seconds'): "WFCD holds the nock, not the wiki's reload",
    ('flux_rifle', 'reload_seconds'): 'the two sources disagree; the wiki wins',
    ('efv_8_mars', 'reload_seconds'): 'the two sources disagree; the wiki wins',
    ('riot_848', 'reload_seconds'): 'the two sources disagree; the wiki wins',
    ('tenet_detron', 'reload_seconds'): 'the two sources disagree; the wiki wins',
    ('grimoire', 'reload_seconds'): "a Tome does not reload; WFCD's 0.01 is a floor",
    # AN OLDER SNAPSHOT. Riven disposition moves with DE's balance passes and
    # mastery ranks are re-set; the wiki module is the current one.
    ('coda_bassocyst', 'disposition'): 'WFCD is an older snapshot',
    ('coda_bubonico', 'disposition'): 'WFCD is an older snapshot',
    ('tenet_diplos', 'disposition'): 'WFCD is an older snapshot',
    ('tenet_plinx', 'disposition'): 'WFCD is an older snapshot',
    ('thornbak', 'disposition'): 'WFCD is an older snapshot',
    ('vinquibus', 'disposition'): 'WFCD is an older snapshot',
    ('furis', 'mastery_rank'): 'WFCD is an older snapshot',
}

WFCD_FIELDS = [('mastery_rank', 'masteryReq'), ('disposition', 'omegaAttenuation'),
               ('magazine', 'magazineSize'), ('reload_seconds', 'reloadTime')]


def wfcd_index():
    """Every export item by `uniqueName` — the ONLY join (never the name)."""
    out = {}
    for f in glob.glob(os.path.join(ROOT, 'vendor/warframe-items/data/json/*.json')):
        try:
            arr = json.load(io.open(f, encoding='utf-8'))
        except Exception:
            continue
        if not isinstance(arr, list):
            continue
        for it in arr:
            if isinstance(it, dict) and it.get('uniqueName'):
                out.setdefault(it['uniqueName'], it)
    return out


def wfcd_pass(only):
    idx = wfcd_index()
    if not idx:
        print('(vendor/warframe-items is not present — the WFCD pass is skipped)')
        return 0
    checked, findings = 0, []
    for f in sorted(glob.glob(os.path.join(ROOT, 'data/weapons/*/*.yaml'))):
        d = yaml.safe_load(io.open(f, encoding='utf-8'))
        if only and d['id'] not in only:
            continue
        # ARCH-GUNS and COMPANION weapons carry the ARCHWING column in WFCD,
        # and the roster ships the ATMOSPHERE one — the same reason the module
        # pass skips them.
        path = f.replace('\\\\', '/')
        if d.get('inherits') or '/archgun/' in path or '/sentinel/' in path:
            continue
        it = idx.get(d.get('internal_name'))
        if it is None:
            continue
        checked += 1
        for ours, theirs in WFCD_FIELDS:
            if ours not in d or it.get(theirs) is None:
                continue
            if not near(d[ours], it[theirs]) and (d['id'], ours) not in EXPECTED_WFCD:
                findings.append('%s.%s: yaml %s vs WFCD %s' % (d['id'], ours, d[ours], it[theirs]))
    print('%d entries joined to WFCD, %d unexplained disagreement(s)'
          % (checked, len(findings)))
    for f in findings:
        print('  ', f)
    return 1 if findings else 0


if __name__ == '__main__':
    rc = main(set(sys.argv[1:]))
    rc |= wfcd_pass(set(sys.argv[1:]))
    raise SystemExit(rc)'''
assert TAIL_OLD in s
s = s.replace(TAIL_OLD, TAIL_NEW, 1)
s = s.replace('import glob\nimport io\nimport os\nimport sys',
              'import glob\nimport io\nimport json\nimport os\nimport sys')
io.open(p, 'w', encoding='utf-8', newline='').write(s)
print('ok')

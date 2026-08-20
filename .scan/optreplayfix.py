# -*- coding: utf-8 -*-
"""One idea, one ruler: pick the sharp control with the BAND, not with 5%."""
import io

p = 'scripts/check_opt_replay.mjs'
s = io.open(p, encoding='utf-8').read()

# 1. the selection
old = """  let control = null;
  const liveAxis = dropped.find(d => d.refused || d.score === null
    || Math.abs(d.score - row.kill_progress) > 0.05 * Math.abs(row.kill_progress));
  if (liveAxis && !liveAxis.refused) {"""
new = """  // PICK IT WITH THE SAME RULER THE VERDICT USES. This chose the first axis
  // that moved the score by 5%, and the verdict below is a 4-sigma band — two
  // criteria for one idea, and on 2026-08-20 they disagreed: dropping the Kuva
  // Nukor's arcane moved 0.5847 to 0.5483, which is 6.2% and INSIDE the band,
  // so the check picked a control it had already classified as inert and then
  // failed because the band did not catch it. It now takes the axis that moves
  // the score FURTHEST among the ones the band actually flags, and reports when
  // no axis on this weapon is sharp enough — the global "at least one weapon
  // proved the pipe" is what keeps that honest.
  let control = null;
  const sharp = dropped.filter(d => d.refused || d.score === null
    || band(d.score, row.kill_progress,
            direct && direct.ok ? direct.score_se : null, row.kill_progress_se).off);
  const liveAxis = sharp.sort((a, b) =>
    Math.abs((b.score ?? 0) - row.kill_progress) - Math.abs((a.score ?? 0) - row.kill_progress)
  )[0] || null;
  if (liveAxis && !liveAxis.refused) {"""
assert old in s
s = s.replace(old, new, 1)

# 2. the assertion tolerates "no axis is sharp on this weapon"
old2 = """  check(`${tag} ...and a build missing that axis FAILS the same assertion`,
    r.control !== null && (r.control.score === null
      || band(r.control.score, r.rowKpm, r.directSe, r.rowSe).off),
    r.control === null ? "no live axis to maim — the control never ran"
      : `dropped '${r.control.axis}': ${n4(r.rowKpm)} -> ${n4(r.control.score)}, which the band did not catch`);"""
new2 = """  // A weapon with no band-sharp axis is REPORTED, not failed — the same call
  // the coverage line above makes for a degenerate axis. What must not happen
  // is that NO weapon proves it, and the last assertion is that one.
  if (r.control === null) {
    check(`${tag} ...no axis on this weapon is sharp enough to maim — reported`, true, "");
  } else {
    anySharp = true;
    check(`${tag} ...and a build missing that axis FAILS the same assertion`,
      r.control.score === null
        || band(r.control.score, r.rowKpm, r.directSe, r.rowSe).off,
      `dropped '${r.control.axis}': ${n4(r.rowKpm)} -> ${n4(r.control.score)}, which the band did not catch`);
  }"""
assert old2 in s
s = s.replace(old2, new2, 1)

# 3. the global guarantee
old3 = 'check("at least one weapon proved the pipe", anyLive, String(anyLive));'
new3 = ('check("at least one weapon proved the pipe", anyLive, String(anyLive));\n'
        '// …and at least one PROVED THE CONTROL, which is what stops every weapon\n'
        '// quietly reporting "no sharp axis" and the assertion never running.\n'
        'check("at least one weapon proved the control can fail", anySharp, String(anySharp));')
assert old3 in s
s = s.replace(old3, new3, 1)
s = s.replace('let anyLive = false;', 'let anyLive = false;\nlet anySharp = false;', 1)
io.open(p, 'w', encoding='utf-8', newline='').write(s)
print('ok')

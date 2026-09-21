// THE COPY IS MADE HERE, not asked of the model. "You work on a copy" is a
// promise to the reader, and a promise kept by instruction alone is kept only as
// often as the model obeys. Before her first change to a build, fight, search,
// riven or target she did not make herself, the document is branched — decided
// from the action's own `writes` and the observation's `open`, both the door's.

const COPY = {
  target: { action: "enemies.target.copy", field: "target" },
  riven: { action: "rivens.card.copy", field: "riven" },
};
const BARS = ["build", "scenario", "search"];

/// Which documents a successful action made, so she never branches her own:
/// a new or copied build, scenario, search, riven or target is hers.
export function madeBy(id, args, r) {
  if (!r || !r.ok) return null;
  if ((id === "shell.preset.new" || id === "shell.preset.copy") && r.preset) return `${args.bar}:${r.preset}`;
  if ((id === "rivens.card.new" || id === "rivens.card.copy") && r.riven) return `riven:${r.riven}`;
  if ((id === "enemies.target.new" || id === "enemies.target.copy") && r.target) return `target:${r.target}`;
  return null;
}

/// Branch before action `id` if it writes a document of the reader's. Returns
/// null when nothing was branched, else `{ copy, owned, pair? }` — `pair` for a
/// build, naming the reader's build the copy came from, so the change card can
/// compare the two and hand the result back.
export async function branch(door, id, owned) {
  const act = door.actions.find((x) => x.id === id);
  const writes = act && !act.query ? act.writes : null;
  if (!writes) return null;
  const open = (door.observe().open || {})[writes] || null;
  if (COPY[writes]) {
    if (!open || owned.has(`${writes}:${open}`)) return null;
    const r = await door.do(COPY[writes].action, {});
    const copy = r && r.ok ? r[COPY[writes].field] : null;
    return copy ? { copy, owned: `${writes}:${copy}` } : null;
  }
  if (!BARS.includes(writes)) return null;
  if (open && owned.has(`${writes}:${open}`)) return null;
  // NOTHING SAVED YET: the page shows a document nobody owns, so she starts one
  // of her own rather than copying one that does not exist.
  const r = await door.do(open ? "shell.preset.copy" : "shell.preset.new", { bar: writes });
  if (!r || !r.ok || !r.preset) return null;
  return { copy: r.preset, owned: `${writes}:${r.preset}`, ...(writes === "build" && open ? { pair: { copy: r.preset, from: open } } : {}) };
}

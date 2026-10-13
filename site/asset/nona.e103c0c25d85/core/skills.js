// SKILLS: the door's table, sent in parts. She always sees the catalogue — one
// line per module and its actions' names — and loads a module's document when
// she needs it. Both are generated from the door (`wfsim.skills`, `tools()`),
// so nothing here lists what the page can do. docs/NONA.md §"Skills".

/// An argument that is one of more than this many values names how many, not
/// which: a list of every weapon is the finder's job, not the document's.
const ENUM_MAX = 12;

/// THE CATALOGUE, in zone T: what each skill is for and the names of its
/// actions, short enough to ride every request.
export const catalogue = (skills) => skills.map((s) =>
  `${s.id} — ${s.what}. Actions: ${s.actions.map((a) => a.slice(s.id.length + 1)).join(", ")}`).join("\n");

const typeOf = (p) => (p.type ? [].concat(p.type).join("|") : "any");

/// One argument as a line: its name (starred when required), its type or its
/// values, and what it is.
function argLine(name, p, required) {
  const values = p.enum
    ? (p.enum.length <= ENUM_MAX ? `one of ${p.enum.map((v) => JSON.stringify(v)).join("|")}` : `one of ${p.enum.length} values`)
    : typeOf(p);
  return `    ${name}${required ? "*" : ""}: ${values}${p.description ? ` — ${p.description}` : ""}`;
}

/// A SKILL'S DOCUMENT: each of its actions as a signature, what it does, and
/// its arguments, one per line. Taken at the moment it is loaded and kept in
/// the record as it was, so a request that carries it repeats the same bytes.
export function skillDoc(skill, tools) {
  const lines = [`<skill ${skill.id}>`, `${skill.what}. Call an action with act(id, args); * marks a required argument.`];
  for (const id of skill.actions) {
    const t = tools.find((x) => x.name === id);
    if (!t) continue;
    const props = (t.input_schema && t.input_schema.properties) || {};
    const req = new Set((t.input_schema && t.input_schema.required) || []);
    lines.push(`${id} — ${t.description}`);
    for (const [k, p] of Object.entries(props)) lines.push(argLine(k, p, req.has(k)));
  }
  lines.push(`</skill>`);
  return lines.join("\n");
}

/// Which skill an action belongs to.
export const skillOf = (id) => String(id).split(".")[0];

/// The documents a skill result or attachment holds, one per skill.
export const docsOf = (text) => String(text || "").split(/(?=<skill )/).map((d) => d.trim())
  .map((d) => ({ id: (d.match(/^<skill ([\w-]+)>/) || [])[1], text: d })).filter((d) => d.id);
